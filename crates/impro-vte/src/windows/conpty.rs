//! ConPTY ownership and blocking I/O. No Win32 call here executes on GTK's thread.
use crate::{
    diag::Diag,
    emu::quoting::command_line,
    lifecycle::{Action, CprScanner, Lifecycle, Phase, Timing, CPR_REPLY},
};
use std::{
    collections::{BTreeMap, HashMap},
    ffi::{c_void, OsStr},
    fs::File,
    io::{self, Read, Write},
    mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
        mpsc, Arc, Mutex, OnceLock, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::{
    core::HRESULT,
    Win32::{
        Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE},
        System::{
            Console::{COORD, HPCON},
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Pipes::CreatePipe,
            Threading::*,
            IO::CancelSynchronousIo,
        },
    },
};

type Create = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type Resize = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type Close = unsafe extern "system" fn(HPCON);
#[derive(Clone, Copy)]
struct Api {
    create: Create,
    resize: Resize,
    close: Close,
}
impl Api {
    fn load() -> io::Result<Self> {
        unsafe {
            let module = GetModuleHandleW(wide(OsStr::new("kernel32.dll")).as_ptr());
            let create = GetProcAddress(module, c"CreatePseudoConsole".as_ptr().cast());
            let resize = GetProcAddress(module, c"ResizePseudoConsole".as_ptr().cast());
            let close = GetProcAddress(module, c"ClosePseudoConsole".as_ptr().cast());
            match (create, resize, close) {
                (Some(a), Some(b), Some(c)) => Ok(Self {
                    create: mem::transmute::<unsafe extern "system" fn() -> isize, Create>(a),
                    resize: mem::transmute::<unsafe extern "system" fn() -> isize, Resize>(b),
                    close: mem::transmute::<unsafe extern "system" fn() -> isize, Close>(c),
                }),
                _ => Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "ConPTY requires Windows 10 version 1809 or newer",
                )),
            }
        }
    }
}
struct Console {
    handle: HPCON,
    api: Api,
}
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            (self.api.close)(self.handle);
        }
    }
}
struct Attributes {
    initialized: bool,
    storage: Vec<usize>,
}
impl Attributes {
    fn new(console: HPCON) -> io::Result<Self> {
        unsafe {
            let mut bytes = 0;
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut bytes);
            let mut value = Self {
                initialized: false,
                storage: vec![0; bytes.div_ceil(mem::size_of::<usize>())],
            };
            if InitializeProcThreadAttributeList(value.pointer(), 1, 0, &mut bytes) == 0 {
                return Err(io::Error::last_os_error());
            }
            value.initialized = true;
            if UpdateProcThreadAttribute(
                value.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const c_void,
                mem::size_of::<HPCON>(),
                ptr::null_mut(),
                ptr::null(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(value)
        }
    }
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            if self.initialized {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let (mut read, mut write) = (ptr::null_mut(), ptr::null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreatePipe produced two new, independently owned handles.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}
fn duplicate(handle: HANDLE) -> io::Result<OwnedHandle> {
    let mut copy = ptr::null_mut();
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut copy,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(copy) })
}

pub struct Packet {
    pub bytes: Vec<u8>,
    session: Arc<Session>,
}
impl Drop for Packet {
    fn drop(&mut self) {
        self.session
            .unread
            .fetch_sub(self.bytes.len(), Ordering::AcqRel);
    }
}
pub enum Output {
    Aborted(gio::IOErrorEnum),
    Ready(Result<Arc<Session>, String>),
    Data(Packet, mpsc::Sender<()>),
    Exited(i32),
}
enum Control {
    Resize,
    Stop,
    ChildExit,
    /// The reader obtained or delivered output (extends the quiet period after the exit).
    Output,
    ReaderEof,
    CloseReturned,
}
#[derive(Debug)]
pub struct Session {
    pub process: OwnedHandle,
    pub pid: u32,
    pub diag: Diag,
    pub unread: AtomicUsize,
    pub columns: AtomicI32,
    pub rows: AtomicI32,
    requested_columns: AtomicI32,
    requested_rows: AtomicI32,
    stopping: AtomicBool,
    input: mpsc::Sender<Option<Vec<u8>>>,
    control: mpsc::Sender<Control>,
}
static SESSIONS: OnceLock<Mutex<HashMap<u32, Weak<Session>>>> = OnceLock::new();
fn sessions() -> &'static Mutex<HashMap<u32, Weak<Session>>> {
    SESSIONS.get_or_init(Default::default)
}
pub fn pid_number(pid: glib::Pid) -> i32 {
    (unsafe { GetProcessId(pid.0) }) as i32
}
pub fn hangup_pid(pid: i32) {
    let session = sessions()
        .lock()
        .unwrap()
        .get(&(pid as u32))
        .and_then(Weak::upgrade);
    if let Some(session) = session {
        session.stop();
    }
}
impl Session {
    pub fn stop(&self) {
        if !self.stopping.swap(true, Ordering::AcqRel) {
            let _ = self.control.send(Control::Stop);
        }
    }
    pub fn write(&self, bytes: &[u8]) {
        // The worker owns the pipe; enqueueing never waits for the child to read.
        let _ = self.input.send(Some(bytes.to_vec()));
    }
    pub fn resize(&self, columns: i32, rows: i32) {
        self.requested_columns.store(columns, Ordering::Release);
        self.requested_rows.store(rows, Ordering::Release);
        let _ = self.control.send(Control::Resize);
    }
}

pub struct Request {
    pub cwd: Option<PathBuf>,
    pub argv: Vec<String>,
    pub env: Vec<String>,
    pub columns: i32,
    pub rows: i32,
    pub timeout: i32,
    pub cancellable: Option<gio::Cancellable>,
    pub flags: glib::SpawnFlags,
}

fn exit_code(process: HANDLE) -> u32 {
    let mut status = 1;
    unsafe {
        GetExitCodeProcess(process, &mut status);
    }
    status
}
/// `ClosePseudoConsole` may block until the output buffered in the pipe was read and conhost's
/// shutdown requests were answered, so it only ever runs on a thread that is not the reader.
fn close_console(console: Console, diag: &Diag) {
    diag.log(format_args!("ClosePseudoConsole called"));
    let started = Instant::now();
    drop(console);
    diag.log(format_args!(
        "ClosePseudoConsole returned after {} ms",
        started.elapsed().as_millis()
    ));
}
/// Failed start: close the console off-thread while the reader keeps draining, give up on a stuck
/// reader after the lifecycle timeouts, and never block the caller indefinitely.
fn abort_start(
    console: Console,
    reader: JoinHandle<()>,
    drain: &AtomicBool,
    closing: &AtomicBool,
    diag: &Diag,
) {
    let timing = Timing::default();
    drain.store(true, Ordering::Release);
    closing.store(true, Ordering::Release);
    let task = diag.clone();
    let closer = thread::spawn(move || close_console(console, &task));
    let started = Instant::now();
    while !reader.is_finished() {
        let waited = started.elapsed();
        if waited >= timing.close_timeout + timing.abandon {
            diag.log(format_args!("reader abandoned after failed start"));
            return;
        }
        if waited >= timing.close_timeout {
            unsafe {
                CancelSynchronousIo(reader.as_raw_handle());
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = reader.join();
    if closer.is_finished() {
        let _ = closer.join();
    }
}

#[allow(deprecated)]
pub fn spawn(request: Request, output: glib::Sender<Output>) -> io::Result<Arc<Session>> {
    let diag = Diag::new();
    diag.log(format_args!(
        "spawn requested argv={:?} size={}x{}",
        request.argv, request.columns, request.rows
    ));
    let result = start(request, output, &diag);
    if let Err(error) = &result {
        diag.log(format_args!("spawn failed: {error}"));
    }
    result
}

#[allow(deprecated)]
fn start(request: Request, output: glib::Sender<Output>, diag: &Diag) -> io::Result<Arc<Session>> {
    use gio::prelude::*;
    let start = Instant::now();
    let cancelled = || {
        request
            .cancellable
            .as_ref()
            .is_some_and(|c| c.is_cancelled())
            || (request.timeout >= 0
                && start.elapsed() >= Duration::from_millis(request.timeout as u64))
    };
    if cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Terminal spawn cancelled or timed out",
        ));
    }
    let api = Api::load()?;
    let (input_read, input_write) = pipe()?;
    let (output_read, output_write) = pipe()?;
    let mut hpc = 0;
    // No PSEUDOCONSOLE_* flags: PSEUDOCONSOLE_INHERIT_CURSOR would make conhost wait for a
    // cursor-position reply at startup.
    let result = unsafe {
        (api.create)(
            COORD {
                X: request.columns as i16,
                Y: request.rows as i16,
            },
            input_read.as_raw_handle(),
            output_write.as_raw_handle(),
            0,
            &mut hpc,
        )
    };
    if result < 0 {
        return Err(io::Error::other(format!(
            "CreatePseudoConsole failed (0x{:08x}); Windows 10 version 1809 or newer is required",
            result as u32
        )));
    }
    let console = Console { handle: hpc, api };
    let (control_tx, control_rx) = mpsc::channel::<Control>();
    let (input_tx, input_rx) = mpsc::channel::<Option<Vec<u8>>>();
    // Start draining before CreateProcess/ClosePseudoConsole: closing can itself
    // write a final frame, even on the process-creation failure path.
    let drain = Arc::new(AtomicBool::new(false));
    // Set once the console is being closed: the reader then answers conhost's shutdown
    // cursor-position request itself, independent of the main thread and the widget.
    let closing = Arc::new(AtomicBool::new(false));
    let (attach_tx, attach_rx) = mpsc::channel::<Weak<Session>>();
    let reader = {
        let (drain, closing, diag) = (drain.clone(), closing.clone(), diag.clone());
        let (control, input, output) = (control_tx.clone(), input_tx.clone(), output.clone());
        thread::spawn(move || {
            let mut reader = File::from(output_read);
            let mut buffer = [0u8; 2048];
            let mut cpr = CprScanner::default();
            let mut total = 0usize;
            // Attached lazily so the pipe is drained while the process is still being created.
            let mut attached: Option<Option<Weak<Session>>> = None;
            let end = loop {
                let n = match reader.read(&mut buffer) {
                    Ok(0) => break "end of file".to_owned(),
                    Err(error) => break format!("read failed: {error}"),
                    Ok(n) => n,
                };
                if total == 0 {
                    diag.log(format_args!("first output byte ({n} bytes)"));
                }
                total += n;
                let _ = control.send(Control::Output);
                if cpr.scan(&buffer[..n]) > 0 && closing.load(Ordering::Acquire) {
                    diag.log(format_args!(
                        "cursor-position request answered by the reader"
                    ));
                    let _ = input.send(Some(CPR_REPLY.to_vec()));
                }
                let session = attached
                    .get_or_insert_with(|| attach_rx.recv().ok())
                    .as_ref()
                    .and_then(Weak::upgrade);
                if drain.load(Ordering::Acquire)
                    || session
                        .as_ref()
                        .is_none_or(|s| s.stopping.load(Ordering::Acquire))
                {
                    continue;
                }
                let session = session.unwrap();
                session.unread.fetch_add(n, Ordering::AcqRel);
                let (ack, wait) = mpsc::channel();
                if output
                    .send(Output::Data(
                        Packet {
                            bytes: buffer[..n].to_vec(),
                            session: session.clone(),
                        },
                        ack,
                    ))
                    .is_err()
                {
                    continue;
                }
                while matches!(
                    wait.recv_timeout(Duration::from_millis(50)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    if drain.load(Ordering::Acquire) || session.stopping.load(Ordering::Acquire) {
                        break;
                    }
                }
                let _ = control.send(Control::Output);
            };
            diag.log(format_args!("reader EOF ({end}; {total} bytes read)"));
            let _ = control.send(Control::ReaderEof);
        })
    };
    let created = (|| {
        let mut attributes = Attributes::new(hpc)?;
        let mut info: STARTUPINFOEXW = unsafe { mem::zeroed() };
        info.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
        info.lpAttributeList = attributes.pointer();
        let mut process: PROCESS_INFORMATION = unsafe { mem::zeroed() };
        let argv = if request.flags.contains(glib::SpawnFlags::FILE_AND_ARGV_ZERO)
            && request.argv.len() > 1
        {
            &request.argv[1..]
        } else {
            &request.argv[..]
        };
        let mut command = wide(OsStr::new(&command_line(argv)));
        let executable = wide(OsStr::new(&request.argv[0]));
        let cwd = request.cwd.as_ref().map(|p| wide(p.as_os_str()));
        let mut env = BTreeMap::new();
        for (k, v) in std::env::vars_os() {
            env.insert(k.to_string_lossy().to_uppercase(), (k, v));
        }
        for pair in &request.env {
            if let Some((k, v)) = pair.split_once('=') {
                env.insert(k.to_uppercase(), (k.into(), v.into()));
            }
        }
        for (k, v) in [("TERM_PROGRAM", "impro"), ("COLORTERM", "truecolor")] {
            env.insert(k.into(), (k.into(), v.into()));
        }
        let mut environment = Vec::new();
        for (_, (k, v)) in env {
            environment.extend(k.encode_wide());
            environment.push('=' as u16);
            environment.extend(wide(&v));
        }
        environment.push(0);
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Terminal spawn cancelled or timed out",
            ));
        }
        // No handles are inherited: ConPTY is passed only through the attribute list.
        let application = if request.flags.contains(glib::SpawnFlags::FILE_AND_ARGV_ZERO) {
            executable.as_ptr()
        } else {
            ptr::null()
        };
        if unsafe {
            CreateProcessW(
                application,
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                cwd.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                &info.StartupInfo,
                &mut process,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
        drop(thread);
        Ok((
            unsafe { OwnedHandle::from_raw_handle(process.hProcess) },
            process.dwProcessId,
        ))
    })();
    // Release the ConPTY pipe ends after process creation, as the API requires.
    drop(input_read);
    drop(output_write);
    let (process, pid) = match created {
        Ok(value) => value,
        Err(error) => {
            drop(attach_tx);
            abort_start(console, reader, &drain, &closing, diag);
            return Err(error);
        }
    };
    diag.set_pid(pid);
    diag.log(format_args!("process created"));
    let duplicates = duplicate(process.as_raw_handle())
        .and_then(|monitor| Ok((duplicate(monitor.as_raw_handle())?, monitor)));
    let (wait_handle, monitor_process) = match duplicates {
        Ok(handles) => handles,
        Err(error) => {
            // The child exists but nothing will supervise it: do not leave it running.
            unsafe {
                TerminateProcess(process.as_raw_handle(), 1);
            }
            drop(attach_tx);
            abort_start(console, reader, &drain, &closing, diag);
            return Err(error);
        }
    };
    let session = Arc::new(Session {
        process,
        pid,
        diag: diag.clone(),
        unread: AtomicUsize::new(0),
        columns: AtomicI32::new(request.columns),
        rows: AtomicI32::new(request.rows),
        requested_columns: AtomicI32::new(request.columns),
        requested_rows: AtomicI32::new(request.rows),
        stopping: AtomicBool::new(false),
        input: input_tx.clone(),
        control: control_tx.clone(),
    });
    sessions()
        .lock()
        .unwrap()
        .insert(pid, Arc::downgrade(&session));
    let writer = thread::spawn(move || {
        let mut writer = File::from(input_write);
        while let Ok(Some(bytes)) = input_rx.recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
        }
    });
    let state = Arc::downgrade(&session);
    // A dedicated waiter wakes the controller on child exit; idle terminals poll nothing.
    thread::spawn(move || {
        unsafe {
            WaitForSingleObject(wait_handle.as_raw_handle(), INFINITE);
        }
        let _ = control_tx.send(Control::ChildExit);
    });
    let (activate_tx, activate_rx) = mpsc::channel();
    let controller_output = output.clone();
    let controller_diag = diag.clone();
    let controller_session = session.control.clone();
    thread::spawn(move || {
        let diag = controller_diag;
        let advertised = activate_rx.recv().unwrap_or(false);
        let mut console = Some(console);
        let mut closer: Option<JoinHandle<()>> = None;
        let mut life = Lifecycle::new(Timing::default());
        loop {
            let event = match life.next_deadline() {
                Some(deadline) => {
                    control_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                }
                None => control_rx
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
            };
            let now = Instant::now();
            let actions = match event {
                Ok(Control::Resize) => {
                    if let (Some(s), Some(c)) = (state.upgrade(), console.as_ref()) {
                        let columns = s.requested_columns.load(Ordering::Acquire);
                        let rows = s.requested_rows.load(Ordering::Acquire);
                        if (columns, rows)
                            != (
                                s.columns.load(Ordering::Acquire),
                                s.rows.load(Ordering::Acquire),
                            )
                        {
                            let result = unsafe {
                                (api.resize)(
                                    c.handle,
                                    COORD {
                                        X: columns as i16,
                                        Y: rows as i16,
                                    },
                                )
                            };
                            if result >= 0 {
                                s.columns.store(columns, Ordering::Release);
                                s.rows.store(rows, Ordering::Release);
                            }
                        }
                    }
                    Vec::new()
                }
                Ok(Control::Stop) => {
                    diag.log(format_args!("stop requested (widget closed or hang-up)"));
                    drain.store(true, Ordering::Release);
                    life.stop(now)
                }
                Ok(Control::ChildExit) => {
                    diag.log(format_args!(
                        "process exit observed (exit code {})",
                        exit_code(monitor_process.as_raw_handle())
                    ));
                    life.process_exited(now)
                }
                Ok(Control::Output) => {
                    life.output(now);
                    Vec::new()
                }
                Ok(Control::ReaderEof) => life.reader_eof(now),
                Ok(Control::CloseReturned) => life.close_returned_at(now),
                Err(mpsc::RecvTimeoutError::Timeout) => life.poll(now),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            for action in actions {
                match action {
                    Action::StartClose => {
                        diag.log(format_args!("drain started; closing the pseudo console"));
                        closing.store(true, Ordering::Release);
                        if let Some(console) = console.take() {
                            let (task, done) = (diag.clone(), controller_session.clone());
                            closer = Some(thread::spawn(move || {
                                close_console(console, &task);
                                let _ = done.send(Control::CloseReturned);
                            }));
                        }
                    }
                    Action::Terminate => {
                        diag.log(format_args!("grace period over; terminating the child"));
                        unsafe {
                            TerminateProcess(monitor_process.as_raw_handle(), 1);
                        }
                    }
                    Action::CancelRead => {
                        diag.log(format_args!(
                            "reader did not reach EOF; cancelling its read"
                        ));
                        drain.store(true, Ordering::Release);
                        unsafe {
                            CancelSynchronousIo(reader.as_raw_handle());
                        }
                    }
                    Action::Emit { forced } => {
                        let status = exit_code(monitor_process.as_raw_handle());
                        sessions().lock().unwrap().remove(&pid);
                        diag.log(format_args!(
                            "child-exited posted (status {status}{})",
                            if forced { ", reader abandoned" } else { "" }
                        ));
                        if advertised {
                            let _ = controller_output.send(Output::Exited(status as i32));
                        }
                    }
                }
            }
            if life.phase() == Phase::Done {
                break;
            }
        }
        // Release: disconnect input even if external Pty references outlive the widget. The
        // writer may be idle in recv, so its queue also receives a closing marker.
        sessions().lock().unwrap().remove(&pid);
        let _ = input_tx.send(None);
        let waited = Instant::now();
        while !writer.is_finished() && waited.elapsed() < Duration::from_secs(2) {
            unsafe {
                CancelSynchronousIo(writer.as_raw_handle());
            }
            thread::sleep(Duration::from_millis(10));
        }
        if writer.is_finished() {
            let _ = writer.join();
        }
        let waited = Instant::now();
        while !reader.is_finished() && waited.elapsed() < Duration::from_millis(500) {
            thread::sleep(Duration::from_millis(5));
        }
        let reader_joined = reader.is_finished();
        if reader_joined {
            let _ = reader.join();
        }
        let close_joined = closer.is_some() && life.close_returned();
        if let (Some(closer), true) = (closer, close_joined) {
            let _ = closer.join();
        }
        diag.log(format_args!(
            "resources released (reader joined: {reader_joined}, close returned: {})",
            life.close_returned()
        ));
    });
    if cancelled() {
        session.stop();
        let _ = attach_tx.send(Arc::downgrade(&session));
        let _ = activate_tx.send(false);
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Terminal spawn cancelled or timed out",
        ));
    }
    if output.send(Output::Ready(Ok(session.clone()))).is_err() {
        session.stop();
    }
    let _ = attach_tx.send(Arc::downgrade(&session));
    let _ = activate_tx.send(true);
    Ok(session)
}
