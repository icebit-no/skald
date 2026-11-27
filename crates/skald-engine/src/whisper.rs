//! A Whisper model and its session, driven through the raw C API: the safe binding cannot put the prompt
//! at the head of every 30-second window, and its session keeps the native handle to itself.

use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::ptr::{self, NonNull};
use std::time::Duration;

use transcribe_cpp_sys as sys;

use crate::Error;

pub(crate) struct Model {
    ptr: NonNull<sys::transcribe_model>,
}

// SAFETY: the C library allows a model to be queried and tokenized from any thread; the only compute on
// it runs through a `Session`, which takes `&mut self`.
unsafe impl Send for Model {}
unsafe impl Sync for Model {}

impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a successful load and is freed once, after every session on it
        // (`Transcriber` declares its session before its model, so the session drops first).
        unsafe { sys::transcribe_model_free(self.ptr.as_ptr()) };
    }
}

impl Model {
    /// Loads the model on the best backend the build has: Metal on a Mac.
    pub(crate) fn load(path: &Path) -> Result<Self, Error> {
        let load_error = |reason: String| Error::Load {
            path: path.to_owned(),
            reason,
        };
        let c_path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| load_error("the path holds a NUL byte".into()))?;
        let mut params: sys::transcribe_model_load_params = unsafe { std::mem::zeroed() };
        // SAFETY: `params` is a valid, writable struct of the type the call fills.
        unsafe { sys::transcribe_model_load_params_init(&mut params) };
        params.backend = sys::transcribe_backend_request::TRANSCRIBE_BACKEND_AUTO;
        let mut out = ptr::null_mut();
        // SAFETY: the path is NUL-terminated and outlives the call; `out` receives the model.
        let status = unsafe { sys::transcribe_model_load_file(c_path.as_ptr(), &params, &mut out) };
        check(status).map_err(load_error)?;
        let ptr = NonNull::new(out).ok_or_else(|| load_error("no model came back".into()))?;
        Ok(Self { ptr })
    }

    /// The model's family, such as «whisper».
    pub(crate) fn arch(&self) -> String {
        // SAFETY: the model is alive; the string is owned by it and copied before it can go.
        owned(unsafe { sys::transcribe_model_arch_string(self.ptr.as_ptr()) })
    }

    /// The device the model computes on, such as «MTL0» or «CPU».
    pub(crate) fn device(&self) -> String {
        // SAFETY: as for `arch`.
        owned(unsafe { sys::transcribe_model_backend(self.ptr.as_ptr()) })
    }

    /// The model's token ids for `text`, without special tokens.
    pub(crate) fn tokenize(&self, text: &str) -> Result<Vec<i32>, Error> {
        let c_text = CString::new(text).map_err(|_| Error::Nul)?;
        let mut tokens = vec![0i32; text.len().max(16)];
        loop {
            // SAFETY: the text is NUL-terminated, and `tokens` has room for the length passed.
            let n = unsafe {
                sys::transcribe_tokenize(
                    self.ptr.as_ptr(),
                    c_text.as_ptr(),
                    tokens.as_mut_ptr(),
                    tokens.len(),
                )
            };
            match n {
                i32::MIN => {
                    return Err(Error::Transcribe {
                        reason: "the model has no tokenizer for text".into(),
                    });
                }
                // The buffer was too small; the negative count is the size needed.
                n if n < 0 => tokens.resize(n.unsigned_abs() as usize, 0),
                n => {
                    tokens.truncate(n as usize);
                    return Ok(tokens);
                }
            }
        }
    }
}

pub(crate) struct Session {
    ptr: NonNull<sys::transcribe_session>,
}

// SAFETY: a session is used by one thread at a time, which `&mut self` on every call that computes
// ensures.
unsafe impl Send for Session {}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a successful init and is freed once.
        unsafe { sys::transcribe_session_free(self.ptr.as_ptr()) };
    }
}

/// What one run wrote.
pub(crate) struct Run {
    pub(crate) text: String,
    /// False when the run stopped early, at the output limit or because the text began to repeat; the
    /// text is what came before.
    pub(crate) complete: bool,
    pub(crate) mel: Duration,
    pub(crate) encode: Duration,
    pub(crate) decode: Duration,
}

impl Session {
    pub(crate) fn new(model: &Model) -> Result<Self, Error> {
        let mut params: sys::transcribe_session_params = unsafe { std::mem::zeroed() };
        // SAFETY: `params` is a valid, writable struct of the type the call fills.
        unsafe { sys::transcribe_session_params_init(&mut params) };
        let mut out = ptr::null_mut();
        // SAFETY: the model is alive, and `out` receives the session.
        let status = unsafe { sys::transcribe_session_init(model.ptr.as_ptr(), &params, &mut out) };
        check(status).map_err(|reason| Error::Transcribe { reason })?;
        let ptr = NonNull::new(out).ok_or_else(|| Error::Transcribe {
            reason: "no session came back".into(),
        })?;
        Ok(Self { ptr })
    }

    /// Transcribes 16 kHz mono samples in `language`, a Whisper language code, decoding greedily at
    /// temperature 0 with Whisper's own fallback to higher temperatures. The prompt heads every 30-second
    /// window, followed by as much of the text before the window as the prompt leaves room for.
    pub(crate) fn run(
        &mut self,
        samples: &[f32],
        language: &CStr,
        prompt: Option<&CStr>,
    ) -> Result<Run, Error> {
        let n_samples = i32::try_from(samples.len()).map_err(|_| Error::Transcribe {
            reason: "the take is too long".into(),
        })?;

        let mut whisper: sys::transcribe_whisper_run_ext = unsafe { std::mem::zeroed() };
        // SAFETY: `whisper` is a valid, writable struct of the type the call fills.
        unsafe { sys::transcribe_whisper_run_ext_init(&mut whisper) };
        whisper.initial_prompt = prompt.map_or(ptr::null(), CStr::as_ptr);
        whisper.prompt_condition =
            sys::transcribe_whisper_prompt_condition::TRANSCRIBE_WHISPER_PROMPT_ALL_SEGMENTS;
        whisper.condition_on_prev_tokens = true;

        let mut params: sys::transcribe_run_params = unsafe { std::mem::zeroed() };
        // SAFETY: as above.
        unsafe { sys::transcribe_run_params_init(&mut params) };
        params.language = language.as_ptr();
        params.timestamps = sys::transcribe_timestamp_kind::TRANSCRIBE_TIMESTAMPS_NONE;
        params.family = (&raw const whisper).cast::<sys::transcribe_ext>();

        // SAFETY: the session is alive and used by this thread alone; the samples, the language, the
        // prompt and both parameter structs outlive the call, which copies what it keeps.
        let status =
            unsafe { sys::transcribe_run(self.ptr.as_ptr(), samples.as_ptr(), n_samples, &params) };
        use sys::transcribe_status as S;
        let complete = match status {
            S::TRANSCRIBE_ERR_OUTPUT_TRUNCATED | S::TRANSCRIBE_ERR_OUTPUT_REPETITION => false,
            status => {
                check(status).map_err(|reason| Error::Transcribe { reason })?;
                true
            }
        };

        // SAFETY: the session is alive; the text is owned by it and copied here.
        let text = owned(unsafe { sys::transcribe_full_text(self.ptr.as_ptr()) })
            .trim()
            .to_owned();
        let mut timings: sys::transcribe_timings = unsafe { std::mem::zeroed() };
        // SAFETY: `timings` is a valid, writable struct of the type the calls fill.
        unsafe {
            sys::transcribe_timings_init(&mut timings);
            sys::transcribe_get_timings(self.ptr.as_ptr(), &mut timings);
        }
        Ok(Run {
            text,
            complete,
            mel: millis(timings.mel_ms),
            encode: millis(timings.encode_ms),
            decode: millis(timings.decode_ms),
        })
    }
}

/// The library's own words for a status other than success.
fn check(status: sys::transcribe_status) -> Result<(), String> {
    if status == sys::transcribe_status::TRANSCRIBE_OK {
        return Ok(());
    }
    // SAFETY: the call returns a static string for any status value.
    let text = owned(unsafe { sys::transcribe_status_string(status.0 as i32) });
    Err(format!("{text} (status {})", status.0 as i32))
}

fn owned(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the library returns NUL-terminated strings that stay valid until the next call on the
    // same object, and this copies the string at once.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

fn millis(ms: f32) -> Duration {
    Duration::from_secs_f32(ms.max(0.0) / 1000.0)
}
