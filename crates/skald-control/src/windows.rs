//! The local socket between the command line and the running Skald, a named pipe on Windows.
//!
//! The pipe's name is made from the socket's path in Skald's folders, so each user's Skald, and a Skald
//! given a folder of its own, has a pipe of its own. Only the user can open it: its access list names the
//! user alone. The first instance is created with the flag that fails when the name is taken, and the
//! command line checks that the process answering runs as the same user before it sends anything, so no
//! other program can stand in for Skald. A pipe goes away with its last handle, so a Skald that crashed
//! leaves nothing behind.

use std::cell::Cell;
use std::ffi::c_void;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING,
    ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE,
    GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, INFINITE, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
};

use crate::commands;
use crate::protocol::{self, Error, ErrorCode, Output, Request, read_line};
use crate::skald::Skald;

/// How long a connection may sit idle before the server closes it.
const IDLE: Duration = Duration::from_secs(60);

/// How long the command line waits for an answer. Transcribing a long file takes longer than the rest.
const ANSWER: Duration = Duration::from_secs(30);
const TRANSCRIBE_ANSWER: Duration = Duration::from_secs(600);

/// How long the command line waits for a free instance while every instance answers someone else.
const BUSY_WAIT: Duration = Duration::from_secs(5);

/// What the system keeps for each direction of an instance; a longer line simply takes more turns.
const BUFFER: u32 = 4_096;

/// How often the thread waiting for a connection looks whether Skald is stopping, in milliseconds.
const STOP_CHECK: u32 = 250;

/// A pipe created under its name, not yet answering.
#[derive(Debug)]
pub struct Server {
    listening: Pipe,
    name: Vec<u16>,
    path: PathBuf,
    access: Access,
}

#[derive(Debug)]
pub enum BindError {
    /// Another Skald answers on the pipe, or another program holds its name.
    AlreadyRunning,
    Io(PathBuf, io::Error),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => f.write_str("another Skald is answering on the pipe"),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for BindError {}

impl Server {
    pub fn bind(path: &Path) -> Result<Self, BindError> {
        let io = |error| BindError::Io(path.to_path_buf(), error);
        let name = pipe_name(path);
        if open(&name).map_err(io)?.is_some() {
            return Err(BindError::AlreadyRunning);
        }
        let access = Access::for_this_user().map_err(io)?;
        let listening = create_instance(&name, &access, true).map_err(|error| {
            if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                BindError::AlreadyRunning
            } else {
                io(error)
            }
        })?;
        Ok(Self {
            listening,
            name,
            path: path.to_path_buf(),
            access,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Answers on a thread of its own until the returned [`Serving`] is stopped or dropped. Each
    /// connection gets a thread, so a long transcription does not hold up a quick command.
    pub fn serve(self, skald: Arc<dyn Skald + Send + Sync>) -> Serving {
        let stopping = Arc::new(AtomicBool::new(false));
        let name = self.name.clone();
        let thread = {
            let stopping = Arc::clone(&stopping);
            thread::Builder::new()
                .name("skald-control".to_owned())
                .spawn(move || accept(self, &skald, &stopping))
                .expect("a thread can be started")
        };
        Serving {
            name,
            stopping,
            thread: Some(thread),
        }
    }
}

/// The server answering. Once it is stopped, the pipe is gone.
#[derive(Debug)]
pub struct Serving {
    name: Vec<u16>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Serving {
    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        self.stopping.store(true, Ordering::SeqCst);
        // Wakes the thread waiting for the next connection, so it sees it is stopping.
        drop(open(&self.name));
        let _ = thread.join();
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Keeps one instance listening at every moment, so the command line never finds the pipe missing while
/// Skald runs: the next instance exists before the connected one is handed on.
fn accept(server: Server, skald: &Arc<dyn Skald + Send + Sync>, stopping: &AtomicBool) {
    let Server {
        mut listening,
        name,
        access,
        ..
    } = server;
    loop {
        let connected = listening.wait_for_client(stopping);
        if stopping.load(Ordering::SeqCst) {
            return;
        }
        let next = loop {
            match create_instance(&name, &access, false) {
                Ok(next) => break next,
                Err(_) if stopping.load(Ordering::SeqCst) => return,
                // Out of handles, for one: wait rather than spin.
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        };
        let client = std::mem::replace(&mut listening, next);
        if connected.is_ok() {
            let skald = Arc::clone(skald);
            let _ = thread::Builder::new()
                .name("skald-control-connection".to_owned())
                .spawn(move || answer(client, &*skald));
        }
    }
}

fn answer(pipe: Pipe, skald: &dyn Skald) {
    pipe.timeout.set(Some(IDLE));
    let mut reader = BufReader::new(&pipe);
    loop {
        let line = match read_line(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) | Err(_) => return,
        };
        let result =
            protocol::decode_request(&line).and_then(|request| commands::execute(&request, skald));
        let mut response = protocol::encode_response(&result);
        response.push('\n');
        if (&pipe).write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

/// A connection to the running Skald.
#[derive(Debug)]
pub struct Client {
    reader: BufReader<Pipe>,
}

impl Client {
    /// Connects to the Skald answering on the pipe; `None` when no Skald is running.
    pub fn connect(path: &Path) -> Result<Option<Self>, Error> {
        let pipe = match open(&pipe_name(path)) {
            Ok(Some(pipe)) => pipe,
            Ok(None) => return Ok(None),
            Err(error) => {
                return Err(Error::new(
                    ErrorCode::Io,
                    format!("Får ikke kontakt med Skald på {}: {error}", path.display()),
                ));
            }
        };
        if !pipe.served_by_this_user() {
            return Err(Error::new(
                ErrorCode::Io,
                format!(
                    "Et program som ikke kjører som deg, svarer der Skald skulle svart ({}).",
                    path.display()
                ),
            ));
        }
        Ok(Some(Self {
            reader: BufReader::new(pipe),
        }))
    }

    pub fn send(&mut self, request: &Request) -> Result<Output, Error> {
        let wait = match request {
            Request::Transcribe { .. } => TRANSCRIBE_ANSWER,
            _ => ANSWER,
        };
        let mut pipe = self.reader.get_ref();
        pipe.timeout.set(Some(wait));
        let mut line = protocol::encode_request(request);
        line.push('\n');
        pipe.write_all(line.as_bytes()).map_err(lost)?;
        match read_line(&mut self.reader) {
            Ok(Some(line)) => protocol::decode_response(&line),
            Ok(None) => Err(Error::new(
                ErrorCode::Io,
                "Skald lukket forbindelsen uten å svare.",
            )),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {
                Err(Error::new(ErrorCode::Io, "Skald svarer ikke."))
            }
            Err(error) => Err(lost(error)),
        }
    }
}

fn lost(error: io::Error) -> Error {
    Error::new(
        ErrorCode::Io,
        format!("Mistet forbindelsen med Skald: {error}"),
    )
}

/// `\\.\pipe\skald-` and a hash of the path, NUL-terminated. Windows paths ignore case, so the hash
/// does too.
fn pipe_name(path: &Path) -> Vec<u16> {
    // FNV-1a: stable across Rust versions, so the command line and the app always agree.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.to_string_lossy().to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    wide(&format!(r"\\.\pipe\skald-{hash:016x}"))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Opens the pipe as a client; `None` when nobody has created it. While every instance is busy it waits for
/// one and tries again, since another client waiting at the same time may take the instance that comes free.
fn open(name: &[u16]) -> io::Result<Option<Pipe>> {
    let deadline = Instant::now() + BUSY_WAIT;
    loop {
        // Safety: a NUL-terminated name; the server may identify the client but not act as it.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Pipe::new(handle).map(Some);
        }
        // Safety: reads this thread's last error, set by the call above.
        match unsafe { GetLastError() } {
            ERROR_FILE_NOT_FOUND => return Ok(None),
            ERROR_PIPE_BUSY => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::Error::from_raw_os_error(ERROR_PIPE_BUSY as i32));
                }
                let wait = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
                // Safety: a NUL-terminated name.
                unsafe { WaitNamedPipeW(name.as_ptr(), wait) };
            }
            error => return Err(io::Error::from_raw_os_error(error as i32)),
        }
    }
}

fn create_instance(name: &[u16], access: &Access, first: bool) -> io::Result<Pipe> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: access.0,
        bInheritHandle: 0,
    };
    let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // Safety: a NUL-terminated name and attributes whose descriptor lives as long as `access`.
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            &attributes,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Pipe::new(handle)
}

/// A kernel handle, closed when dropped. It may be used from any thread.
#[derive(Debug)]
struct Handle(HANDLE);

// Safety: a kernel handle is not tied to the thread that opened it.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // Safety: a handle this module opened, closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// One end of a pipe instance, read and written with a time limit. Reading and writing take turns on one
/// thread, so they share the event that tells when an operation is done.
#[derive(Debug)]
struct Pipe {
    handle: Handle,
    done: Handle,
    /// `None` waits for ever.
    timeout: Cell<Option<Duration>>,
}

impl Pipe {
    fn new(handle: HANDLE) -> io::Result<Self> {
        let handle = Handle(handle);
        // Safety: a manual-reset event without a name, closed on drop.
        let done = unsafe { CreateEventW(null(), 1, 0, null()) };
        if done.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle,
            done: Handle(done),
            timeout: Cell::new(None),
        })
    }

    fn overlapped(&self) -> OVERLAPPED {
        OVERLAPPED {
            hEvent: self.done.0,
            ..Default::default()
        }
    }

    /// Waits for a client to connect to this instance, or for `stopping`, which is looked at every
    /// [`STOP_CHECK`] in case the connection meant to wake the wait never came.
    fn wait_for_client(&self, stopping: &AtomicBool) -> io::Result<()> {
        let mut overlapped = self.overlapped();
        // Safety: the overlapped structure lives until the operation is finished or cancelled below.
        unsafe {
            let started = ConnectNamedPipe(self.handle.0, &mut overlapped);
            if started == 0 {
                match GetLastError() {
                    ERROR_PIPE_CONNECTED => return Ok(()),
                    ERROR_IO_PENDING => {}
                    error => return Err(io::Error::from_raw_os_error(error as i32)),
                }
                while WaitForSingleObject(self.done.0, STOP_CHECK) == WAIT_TIMEOUT {
                    if stopping.load(Ordering::SeqCst) {
                        CancelIoEx(self.handle.0, &overlapped);
                        let mut moved = 0;
                        GetOverlappedResult(self.handle.0, &overlapped, &mut moved, 1);
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                }
            }
            let mut moved = 0;
            if GetOverlappedResult(self.handle.0, &overlapped, &mut moved, 0) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Waits for an operation started with `overlapped` and returns the bytes it moved. Past the time
    /// limit it is cancelled, and the cancellation waited for, since the system writes into `overlapped`
    /// until then.
    fn finish(
        &self,
        started: i32,
        overlapped: &OVERLAPPED,
        timeout: Option<Duration>,
    ) -> io::Result<u32> {
        // Safety: the handle and the overlapped structure are the operation's own, and both outlive it.
        unsafe {
            if started == 0 {
                let error = GetLastError();
                if error != ERROR_IO_PENDING {
                    return Err(io::Error::from_raw_os_error(error as i32));
                }
                let wait = timeout.map_or(INFINITE, |timeout| {
                    u32::try_from(timeout.as_millis()).unwrap_or(INFINITE - 1)
                });
                if WaitForSingleObject(self.done.0, wait) == WAIT_TIMEOUT {
                    CancelIoEx(self.handle.0, overlapped);
                    let mut moved = 0;
                    GetOverlappedResult(self.handle.0, overlapped, &mut moved, 1);
                    return Err(io::ErrorKind::TimedOut.into());
                }
            }
            let mut moved = 0;
            if GetOverlappedResult(self.handle.0, overlapped, &mut moved, 0) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(moved)
        }
    }

    /// Whether the process at the other end runs as the same user as this one.
    fn served_by_this_user(&self) -> bool {
        let mut process_id = 0;
        // Safety: a pipe handle and a valid out pointer.
        if unsafe { GetNamedPipeServerProcessId(self.handle.0, &mut process_id) } == 0 {
            return false;
        }
        // Safety: asks only for the right to read the process's limited information.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return false;
        }
        let process = Handle(process);
        match (user_of(process.0), this_user()) {
            (Some(theirs), Some(ours)) => theirs == ours,
            _ => false,
        }
    }
}

impl Read for &Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut overlapped = self.overlapped();
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // Safety: the buffer holds `len` bytes and, like the overlapped structure, outlives the read.
        let started = unsafe {
            ReadFile(
                self.handle.0,
                buf.as_mut_ptr(),
                len,
                null_mut(),
                &mut overlapped,
            )
        };
        match self.finish(started, &overlapped, self.timeout.get()) {
            Ok(read) => Ok(read as usize),
            // The other end has closed: the end of the stream.
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(code) if code == ERROR_BROKEN_PIPE as i32
                        || code == ERROR_PIPE_NOT_CONNECTED as i32
                ) =>
            {
                Ok(0)
            }
            Err(error) => Err(error),
        }
    }
}

impl Read for Pipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}

impl Write for &Pipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut overlapped = self.overlapped();
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // Safety: the buffer holds `len` bytes and, like the overlapped structure, outlives the write.
        let started = unsafe {
            WriteFile(
                self.handle.0,
                buf.as_ptr(),
                len,
                null_mut(),
                &mut overlapped,
            )
        };
        self.finish(started, &overlapped, self.timeout.get())
            .map(|written| written as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The pipe's security descriptor: full access for this user and nobody else.
#[derive(Debug)]
struct Access(*mut c_void);

// Safety: the descriptor is plain memory, read by the system and freed once.
unsafe impl Send for Access {}

impl Access {
    fn for_this_user() -> io::Result<Self> {
        let user = this_user().ok_or_else(|| io::Error::other("this process's user is unknown"))?;
        // A protected list with one entry: generic all for the user.
        let sddl = wide(&format!("D:P(A;;GA;;;{user})"));
        let mut descriptor = null_mut();
        // Safety: a NUL-terminated string and a valid out pointer; the descriptor is freed on drop.
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for Access {
    fn drop(&mut self) {
        // Safety: memory the system allocated for the descriptor, freed once.
        unsafe { LocalFree(self.0) };
    }
}

fn this_user() -> Option<String> {
    // Safety: the pseudo handle of this process, which needs no closing.
    user_of(unsafe { GetCurrentProcess() })
}

/// The SID of the user a process runs as, in its string form.
fn user_of(process: HANDLE) -> Option<String> {
    let mut token = null_mut();
    // Safety: an open process handle and a valid out pointer; the token is closed on drop.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = Handle(token);
    let mut needed = 0;
    // Safety: asks only for the size the user needs.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed) };
    if needed == 0 {
        return None;
    }
    // u64s keep the structure aligned for the pointer it holds.
    let mut buffer = vec![0u64; (needed as usize).div_ceil(size_of::<u64>())];
    // Safety: the buffer holds at least `needed` bytes; the SID is read only while the buffer lives, and
    // its string is freed once copied.
    unsafe {
        if GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &mut needed,
        ) == 0
        {
            return None;
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut string = null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut string) == 0 {
            return None;
        }
        let len = (0..).take_while(|&i| *string.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
        LocalFree(string.cast::<c_void>());
        Some(sid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    fn unique_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "skald-pipe-{name}-{}/skald.sock",
            std::process::id()
        ))
    }

    /// The pipe's access list in its string form.
    fn access_list(pipe: &Pipe) -> String {
        let mut descriptor = null_mut();
        // Safety: a pipe handle opened with the right to read its access list and a valid out pointer;
        // the descriptor is freed once read.
        unsafe {
            let read = GetSecurityInfo(
                pipe.handle.0,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            );
            assert_eq!(read, 0, "GetSecurityInfo");
            let text = string_form(descriptor);
            LocalFree(descriptor);
            text
        }
    }

    /// An access list as the system writes it back, which names well-known accounts by their short
    /// names, as `LA` for the built-in administrator.
    fn as_written_back(sddl: &str) -> String {
        let mut descriptor = null_mut();
        // Safety: a NUL-terminated string and a valid out pointer; the descriptor is freed once read.
        unsafe {
            let made = ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide(sddl).as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            );
            assert_ne!(
                made, 0,
                "ConvertStringSecurityDescriptorToSecurityDescriptorW"
            );
            let text = string_form(descriptor);
            LocalFree(descriptor);
            text
        }
    }

    /// The access list of a security descriptor in its string form.
    ///
    /// Safety: `descriptor` is a valid security descriptor.
    unsafe fn string_form(descriptor: *mut c_void) -> String {
        let mut string = null_mut();
        // Safety: as the caller promises, and a valid out pointer; the string is freed once copied.
        unsafe {
            let converted = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut string,
                null_mut(),
            );
            assert_ne!(
                converted, 0,
                "ConvertSecurityDescriptorToStringSecurityDescriptorW"
            );
            let len = (0..).take_while(|&i| *string.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
            LocalFree(string.cast::<c_void>());
            text
        }
    }

    #[test]
    fn only_the_user_can_open_the_pipe() {
        let server = Server::bind(&unique_path("access")).unwrap();
        let user = this_user().unwrap();
        assert_eq!(
            access_list(&server.listening),
            as_written_back(&format!("D:P(A;;FA;;;{user})"))
        );
    }

    #[test]
    fn the_name_follows_the_path_whatever_its_case() {
        let name = |path: &str| String::from_utf16(&pipe_name(Path::new(path))).unwrap();
        assert_eq!(
            name(r"C:\Users\A\Skald\skald.sock"),
            name(r"c:\users\a\skald\SKALD.SOCK")
        );
        assert_ne!(
            name(r"C:\Users\A\Skald\skald.sock"),
            name(r"C:\Users\B\Skald\skald.sock")
        );
        assert!(name(r"C:\x").starts_with(r"\\.\pipe\skald-"));
        assert!(name(r"C:\x").ends_with('\0'));
    }

    #[test]
    fn this_process_answers_as_this_user() {
        let path = unique_path("user");
        let _server = Server::bind(&path).unwrap();
        let client = open(&pipe_name(&path)).unwrap().expect("the pipe is there");
        assert!(client.served_by_this_user());
    }
}
