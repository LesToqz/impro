//! ConPTY ownership and blocking I/O. No Win32 call here executes on GTK's thread.
use crate::emu::quoting::command_line;
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
    thread,
    time::{Duration, Instant},
};
use windows_sys::{
    core::HRESULT,
    Win32::{
        Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, WAIT_OBJECT_0},
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
}
#[derive(Debug)]
pub struct Session {
    pub process: OwnedHandle,
    pub pid: u32,
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

#[allow(deprecated)]
pub fn spawn(request: Request, output: glib::Sender<Output>) -> io::Result<Arc<Session>> {
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
    // Start draining before CreateProcess/ClosePseudoConsole: closing can itself
    // write a final frame, even on the process-creation failure path.
    let drain = Arc::new(AtomicBool::new(false));
    let drain_reader = drain.clone();
    let (attach_tx, attach_rx) = mpsc::channel::<Weak<Session>>();
    let reader_output = output.clone();
    let reader = thread::spawn(move || {
        let mut reader = File::from(output_read);
        let mut buffer = [0u8; 2048];
        let session = attach_rx.recv().ok();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let session = session.as_ref().and_then(Weak::upgrade);
                    if drain_reader.load(Ordering::Acquire)
                        || session
                            .as_ref()
                            .is_none_or(|s| s.stopping.load(Ordering::Acquire))
                    {
                        continue;
                    }
                    let session = session.unwrap();
                    session.unread.fetch_add(n, Ordering::AcqRel);
                    let (ack, wait) = mpsc::channel();
                    if reader_output
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
                        if drain_reader.load(Ordering::Acquire)
                            || session.stopping.load(Ordering::Acquire)
                        {
                            break;
                        }
                    }
                }
            }
        }
    });
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
            drain.store(true, Ordering::Release);
            drop(console);
            let _ = reader.join();
            return Err(error);
        }
    };
    let monitor_process = duplicate(process.as_raw_handle())?;
    let (control_tx, control_rx) = mpsc::channel();
    let (input_tx, input_rx) = mpsc::channel::<Option<Vec<u8>>>();
    let session = Arc::new(Session {
        process,
        pid,
        unread: AtomicUsize::new(0),
        columns: AtomicI32::new(request.columns),
        rows: AtomicI32::new(request.rows),
        requested_columns: AtomicI32::new(request.columns),
        requested_rows: AtomicI32::new(request.rows),
        stopping: AtomicBool::new(false),
        input: input_tx,
        control: control_tx,
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
    let done = session.control.clone();
    let wait_handle = duplicate(monitor_process.as_raw_handle())?;
    thread::spawn(move || {
        unsafe {
            WaitForSingleObject(wait_handle.as_raw_handle(), INFINITE);
        }
        let _ = done.send(Control::ChildExit);
    });
    let (activate_tx, activate_rx) = mpsc::channel();
    let controller_output = output.clone();
    thread::spawn(move || {
        let advertised = activate_rx.recv().unwrap_or(false);
        let mut console = Some(console);
        while let Ok(control) = control_rx.recv() {
            match control {
                Control::Resize => {
                    if let Some(s) = state.upgrade() {
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
                                    hpc,
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
                }
                Control::Stop | Control::ChildExit => break,
            }
        }
        if state
            .upgrade()
            .is_none_or(|s| s.stopping.load(Ordering::Acquire))
        {
            drain.store(true, Ordering::Release);
        }
        // Older Windows versions can block inside ClosePseudoConsole. Start close
        // independently so the process grace period is still enforced.
        let console = console.take();
        let (closing, started) = mpsc::channel();
        let closer = thread::spawn(move || {
            let _ = closing.send(());
            drop(console);
        });
        let _ = started.recv();
        unsafe {
            if WaitForSingleObject(monitor_process.as_raw_handle(), 250) != WAIT_OBJECT_0 {
                TerminateProcess(monitor_process.as_raw_handle(), 1);
                WaitForSingleObject(monitor_process.as_raw_handle(), INFINITE);
            }
            CancelSynchronousIo(writer.as_raw_handle());
        }
        // Disconnect input even if external Pty references outlive the widget.
        // The writer may be idle in recv, so its queue also receives a closing marker.
        if let Some(s) = state.upgrade() {
            let _ = s.input.send(None);
        }
        let _ = writer.join();
        let _ = reader.join();
        let _ = closer.join();
        let mut status = 1;
        unsafe {
            GetExitCodeProcess(monitor_process.as_raw_handle(), &mut status);
        }
        sessions().lock().unwrap().remove(&pid);
        if advertised {
            let _ = controller_output.send(Output::Exited(status as i32));
        }
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
