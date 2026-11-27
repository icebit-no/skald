//! Engine probe for the M1a engine check.
//!
//! Loads a model once and prints one JSON line per WAV file (16 kHz mono).
//!
//! ```text
//! engine-probe asr --model M.gguf [--lang no] [--prompt TEXT] [--carry | --condition]
//!                  [--timestamps none|auto] [--backend metal|cpu]
//!                  [--max-prev TOKENS] [--no-speech-thold P|inf]
//!                  [--warmup N] [--repeat N] FILE...
//! engine-probe vad --model silero.gguf [--threshold P] [--min-speech-ms MS] FILE...
//! engine-probe tokens --model M.gguf TEXT-FILE...
//! ```
//!
//! `asr` runs Whisper through the C API, because the Whisper run extension's
//! prompt placement and the per-window decoding trace (no-speech probability)
//! are not part of the safe binding. `--prompt` is Whisper's initial prompt,
//! which by itself primes only the first 30-second window. `--condition`
//! conditions each window on the text decoded before it, starting from the
//! prompt, so the prompt slides out once that text fills the window's 223
//! prompt tokens. `--carry` puts the prompt at the head of every window, ahead
//! of the earlier text (`ALL_SEGMENTS`, which the library accepts only together
//! with that conditioning). Times are wall clock around `transcribe_run`, with
//! the model loaded and `--warmup` runs done first.
//!
//! `vad` runs Silero VAD through the safe binding on the CPU, the backend the
//! library picks for VAD.
//!
//! `tokens` counts the tokens of each file's text as the model's tokenizer
//! writes a Whisper prompt: with one leading space.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::process::exit;
use std::ptr;
use std::time::Instant;

use transcribe_cpp::{Model, VadOptions};
use transcribe_cpp_sys as sys;

const SAMPLE_RATE: u32 = 16_000;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    unsafe { sys::transcribe_log_set(Some(log_warnings), ptr::null_mut()) };
    match args.first().map(String::as_str) {
        Some("asr") => asr(&args[1..]),
        Some("vad") => vad(&args[1..]),
        Some("tokens") => tokens(&args[1..]),
        _ => fail("usage: engine-probe asr|vad|tokens --model PATH [options] FILE..."),
    }
}

fn tokens(args: &[String]) {
    let o = parse(args);
    let model = Model::load(&o.model).unwrap_or_else(|e| fail(&format!("model: {e}")));
    for file in &o.files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| fail(&format!("{file}: {e}")));
        let ids = model.tokenize(&format!(" {}", text.trim())).unwrap_or_else(|e| fail(&format!("tokenize: {e}")));
        let mut line = Json::new();
        line.str("file", &file_name(file));
        line.num("tokens", ids.len() as f64);
        println!("{}", line.finish());
    }
}

unsafe extern "C" fn log_warnings(level: sys::transcribe_log_level, msg: *const c_char, _: *mut c_void) {
    let shown = level == sys::transcribe_log_level::TRANSCRIBE_LOG_LEVEL_WARN
        || level == sys::transcribe_log_level::TRANSCRIBE_LOG_LEVEL_ERROR;
    if shown && !msg.is_null() {
        eprint!("{}", CStr::from_ptr(msg).to_string_lossy());
    }
}

struct Options {
    model: String,
    lang: String,
    prompt: Option<String>,
    carry: bool,
    condition: bool,
    timestamps_none: bool,
    cpu: bool,
    warmup: usize,
    repeat: usize,
    threshold: f64,
    min_speech_ms: i32,
    no_speech_thold: Option<f32>,
    max_prev: Option<i32>,
    files: Vec<String>,
}

fn parse(args: &[String]) -> Options {
    let mut o = Options {
        model: String::new(),
        lang: "no".into(),
        prompt: None,
        carry: false,
        condition: false,
        timestamps_none: true,
        cpu: false,
        warmup: 0,
        repeat: 1,
        threshold: 0.5,
        min_speech_ms: 250,
        no_speech_thold: None,
        max_prev: None,
        files: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_else(|| fail(&format!("{a} needs a value")));
        match a.as_str() {
            "--model" => o.model = value(),
            "--lang" => o.lang = value(),
            "--prompt" => o.prompt = Some(value()),
            "--carry" => o.carry = true,
            "--condition" => o.condition = true,
            "--timestamps" => o.timestamps_none = value() == "none",
            "--backend" => o.cpu = value() == "cpu",
            "--warmup" => o.warmup = number::<usize>(&value()),
            "--repeat" => o.repeat = number::<usize>(&value()).max(1),
            "--threshold" => o.threshold = number(&value()),
            "--min-speech-ms" => o.min_speech_ms = number(&value()),
            "--no-speech-thold" => o.no_speech_thold = Some(number(&value())),
            "--max-prev" => o.max_prev = Some(number(&value())),
            _ if a.starts_with("--") => fail(&format!("unknown option {a}")),
            _ => o.files.push(a.clone()),
        }
    }
    if o.model.is_empty() || o.files.is_empty() {
        fail("--model and at least one FILE are required");
    }
    o
}

fn asr(args: &[String]) {
    let o = parse(args);
    let started = Instant::now();
    let model = load_model(&o.model, o.cpu);
    let session = new_session(model);
    let load_ms = ms_since(started);

    let lang = CString::new(o.lang.clone()).unwrap();
    let prompt = o.prompt.as_ref().map(|p| CString::new(p.as_str()).unwrap());
    let mut ext: sys::transcribe_whisper_run_ext = unsafe { std::mem::zeroed() };
    unsafe { sys::transcribe_whisper_run_ext_init(&mut ext) };
    if let Some(p) = &prompt {
        ext.initial_prompt = p.as_ptr();
    }
    if o.carry {
        ext.prompt_condition = sys::transcribe_whisper_prompt_condition::TRANSCRIBE_WHISPER_PROMPT_ALL_SEGMENTS;
    }
    ext.condition_on_prev_tokens = o.carry || o.condition;
    if let Some(tokens) = o.max_prev {
        ext.max_prev_context_tokens = tokens;
    }
    if let Some(thold) = o.no_speech_thold {
        ext.no_speech_thold = thold;
    }
    let mut params: sys::transcribe_run_params = unsafe { std::mem::zeroed() };
    unsafe { sys::transcribe_run_params_init(&mut params) };
    params.language = lang.as_ptr();
    params.family = &ext as *const sys::transcribe_whisper_run_ext as *const sys::transcribe_ext;
    if o.timestamps_none {
        params.timestamps = sys::transcribe_timestamp_kind::TRANSCRIBE_TIMESTAMPS_NONE;
    }

    let mut first = true;
    for file in &o.files {
        let pcm = read_wav(file);
        let mut first_run_ms = None;
        for _ in 0..o.warmup {
            let t = Instant::now();
            run(session, &pcm, &params);
            first_run_ms.get_or_insert(ms_since(t));
        }
        let mut runs = Vec::with_capacity(o.repeat);
        let mut status = "ok";
        for _ in 0..o.repeat {
            let t = Instant::now();
            status = run(session, &pcm, &params);
            runs.push(ms_since(t));
        }
        let text = unsafe { CStr::from_ptr(sys::transcribe_full_text(session)) }.to_string_lossy().into_owned();
        let mut timings: sys::transcribe_timings = unsafe { std::mem::zeroed() };
        unsafe {
            sys::transcribe_timings_init(&mut timings);
            sys::transcribe_get_timings(session, &mut timings);
        }
        let mut line = Json::new();
        line.str("file", &file_name(file));
        line.num("audio_s", pcm.len() as f64 / SAMPLE_RATE as f64);
        line.str("text", text.trim());
        line.str("status", status);
        if first {
            line.num("load_ms", load_ms);
        }
        if let Some(ms) = first_run_ms {
            line.num("first_run_ms", ms);
        }
        line.nums("runs_ms", &runs);
        line.num("median_ms", median(&runs));
        line.num("mel_ms", timings.mel_ms as f64);
        line.num("encode_ms", timings.encode_ms as f64);
        line.num("decode_ms", timings.decode_ms as f64);
        line.raw("windows", &window_traces(session));
        println!("{}", line.finish());
        first = false;
    }
    unsafe {
        sys::transcribe_session_free(session);
        sys::transcribe_model_free(model);
    }
}

fn load_model(path: &str, cpu: bool) -> *mut sys::transcribe_model {
    let c_path = CString::new(path).unwrap();
    let mut load: sys::transcribe_model_load_params = unsafe { std::mem::zeroed() };
    unsafe { sys::transcribe_model_load_params_init(&mut load) };
    load.backend = if cpu {
        sys::transcribe_backend_request::TRANSCRIBE_BACKEND_CPU
    } else {
        sys::transcribe_backend_request::TRANSCRIBE_BACKEND_METAL
    };
    let mut model = ptr::null_mut();
    check(unsafe { sys::transcribe_model_load_file(c_path.as_ptr(), &load, &mut model) }, "model load");
    model
}

fn new_session(model: *mut sys::transcribe_model) -> *mut sys::transcribe_session {
    let mut params: sys::transcribe_session_params = unsafe { std::mem::zeroed() };
    unsafe { sys::transcribe_session_params_init(&mut params) };
    let mut session = ptr::null_mut();
    check(unsafe { sys::transcribe_session_init(model, &params, &mut session) }, "session init");
    session
}

/// Runs once. A decode the library cut short (budget reached, or stopped when
/// it began repeating) keeps its partial text and is reported, not fatal.
fn run(session: *mut sys::transcribe_session, pcm: &[f32], params: &sys::transcribe_run_params) -> &'static str {
    let status = unsafe { sys::transcribe_run(session, pcm.as_ptr(), pcm.len() as i32, params) };
    match status {
        sys::transcribe_status::TRANSCRIBE_ERR_OUTPUT_TRUNCATED => "truncated",
        sys::transcribe_status::TRANSCRIBE_ERR_OUTPUT_REPETITION => "stopped repeating",
        _ => {
            check(status, "run");
            "ok"
        }
    }
}

fn window_traces(session: *mut sys::transcribe_session) -> String {
    let count = unsafe { sys::transcribe_get_whisper_chunk_count(session) };
    let mut items = Vec::new();
    for i in 0..count {
        let mut t: sys::transcribe_whisper_chunk_trace = unsafe { std::mem::zeroed() };
        unsafe { sys::transcribe_whisper_chunk_trace_init(&mut t) };
        check(unsafe { sys::transcribe_get_whisper_chunk_trace(session, i, &mut t) }, "chunk trace");
        let mut w = Json::new();
        w.num("t0_ms", t.t0_ms as f64);
        w.num("t1_ms", t.t1_ms as f64);
        w.num("no_speech_prob", t.no_speech_prob as f64);
        w.num("avg_logprob", t.avg_logprob as f64);
        w.num("compression_ratio", t.compression_ratio as f64);
        w.num("temperature", t.temperature_used as f64);
        w.num("fallbacks", t.n_fallbacks as f64);
        w.bool("no_speech_triggered", t.no_speech_triggered);
        items.push(w.finish());
    }
    format!("[{}]", items.join(","))
}

fn vad(args: &[String]) {
    let o = parse(args);
    let model = Model::load(&o.model).unwrap_or_else(|e| fail(&format!("vad model: {e}")));
    let mut session = model.vad_session().unwrap_or_else(|e| fail(&format!("vad session: {e}")));
    let frame_samples = model.vad_info().map(|i| i.frame_samples).unwrap_or(512) as f64;
    let options = VadOptions { threshold: o.threshold, min_speech_ms: o.min_speech_ms, ..VadOptions::default() };
    for file in &o.files {
        let pcm = read_wav(file);
        let result = session.run(&pcm, &options).unwrap_or_else(|e| fail(&format!("vad run: {e}")));
        let to_ms = |samples: i64| samples as f64 * 1000.0 / SAMPLE_RATE as f64;
        let segments: Vec<String> = result
            .segments
            .iter()
            .map(|s| format!("[{:.0},{:.0}]", to_ms(s.start_sample), to_ms(s.end_sample)))
            .collect();
        let speech_ms: f64 = result.segments.iter().map(|s| to_ms(s.end_sample - s.start_sample)).sum();
        let frames_over = result.probs.iter().filter(|&&p| p as f64 >= o.threshold).count();
        let max_prob = result.probs.iter().cloned().fold(0.0f32, f32::max);
        let mut line = Json::new();
        line.str("file", &file_name(file));
        line.num("audio_s", pcm.len() as f64 / SAMPLE_RATE as f64);
        line.num("speech_ms", speech_ms);
        line.num("frames_over_threshold_ms", frames_over as f64 * frame_samples * 1000.0 / SAMPLE_RATE as f64);
        line.num("max_prob", max_prob as f64);
        line.raw("segments_ms", &format!("[{}]", segments.join(",")));
        println!("{}", line.finish());
    }
}

fn read_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
    let spec = reader.spec();
    if spec.channels != 1 || spec.sample_rate != SAMPLE_RATE {
        fail(&format!("{path}: {} channels at {} Hz, expected mono 16 kHz", spec.channels, spec.sample_rate));
    }
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap() as f32 / scale).collect()
        }
        hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
    }
}

fn check(status: sys::transcribe_status, what: &str) {
    if status != sys::transcribe_status::TRANSCRIBE_OK {
        let msg = unsafe { CStr::from_ptr(sys::transcribe_status_string(status.0 as i32)) };
        fail(&format!("{what}: {}", msg.to_string_lossy()));
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("engine-probe: {msg}");
    exit(1)
}

fn number<T: std::str::FromStr>(s: &str) -> T {
    s.parse().unwrap_or_else(|_| fail(&format!("not a number: {s}")))
}

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn file_name(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A flat JSON object written field by field.
struct Json(Vec<String>);

impl Json {
    fn new() -> Self {
        Json(Vec::new())
    }
    fn str(&mut self, key: &str, value: &str) {
        self.0.push(format!("{}:{}", quote(key), quote(value)));
    }
    fn num(&mut self, key: &str, value: f64) {
        let v = if value.is_finite() { format!("{value:.4}") } else { "null".into() };
        self.0.push(format!("{}:{v}", quote(key)));
    }
    fn nums(&mut self, key: &str, values: &[f64]) {
        let v: Vec<String> = values.iter().map(|x| format!("{x:.1}")).collect();
        self.0.push(format!("{}:[{}]", quote(key), v.join(",")));
    }
    fn bool(&mut self, key: &str, value: bool) {
        self.0.push(format!("{}:{value}", quote(key)));
    }
    fn raw(&mut self, key: &str, json: &str) {
        self.0.push(format!("{}:{json}", quote(key)));
    }
    fn finish(self) -> String {
        format!("{{{}}}", self.0.join(","))
    }
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
