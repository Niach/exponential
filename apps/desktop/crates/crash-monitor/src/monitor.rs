//! The monitor process: `<exe> __exp-crash-monitor <socket> <dir> <release>
//! <component>`. Serves one client (its parent); exits when that client
//! disconnects (a normal exit), after the crash report is written, when
//! the parent is gone (Unix: reparented) or when nobody connected within
//! [`CONNECT_TIMEOUT`] (the parent exited before connecting).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::report::{self, Crash, Frame, Module};

pub(crate) fn run(args: &[OsString]) -> i32 {
    let [name, dir, release, component] = args else { return 2 };
    let (Some(name), Some(release), Some(component)) = (name.to_str(), release.to_str(), component.to_str()) else {
        return 2;
    };
    let Ok(mut server) = minidumper::Server::with_name(crate::socket(name)) else { return 1 };
    let connected = Arc::new(AtomicBool::new(false));
    let handler = Handler { dir: PathBuf::from(dir), release: release.to_string(), component: component.to_string(), connected: connected.clone() };
    let shutdown = Arc::new(AtomicBool::new(false));
    watch(connected, shutdown.clone());
    match server.run(Box::new(handler), &shutdown, None) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// The parent connects within milliseconds; one that never does is gone.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Stops the server when nobody connected in time or (Unix) the parent
/// died without disconnecting. A crash being handled is synchronous in the
/// server loop, so this never cuts a report short.
fn watch(connected: Arc<AtomicBool>, shutdown: Arc<AtomicBool>) {
    #[cfg(unix)]
    let parent = std::os::unix::process::parent_id();
    let started = Instant::now();
    let _ = std::thread::Builder::new().name("exp-crash-watch".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_millis(500));
        #[cfg(unix)]
        let orphaned = std::os::unix::process::parent_id() != parent;
        #[cfg(not(unix))]
        let orphaned = false;
        if orphaned || (!connected.load(Ordering::Relaxed) && started.elapsed() > CONNECT_TIMEOUT) {
            shutdown.store(true, Ordering::Relaxed);
            return;
        }
    });
}

struct Handler {
    dir: PathBuf,
    release: String,
    component: String,
    connected: Arc<AtomicBool>,
}

impl minidumper::ServerHandler for Handler {
    fn create_minidump_file(&self) -> Result<(std::fs::File, PathBuf), std::io::Error> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.dmp", uuid::Uuid::new_v4()));
        Ok((std::fs::File::create(&path)?, path))
    }

    /// Runs while the crashed process still waits for the ack: its
    /// binaries are still where the dump says they are.
    fn on_minidump_created(&self, result: Result<minidumper::MinidumpBinary, minidumper::Error>) -> minidumper::LoopAction {
        if let Ok(binary) = result {
            let mut file = binary.file;
            let _ = std::io::Write::flush(&mut file);
            drop(file);
            if let Some(crash) = walk(&binary.path, &self.release, &self.component) {
                let _ = report::write(&self.dir, &crash);
            }
            let _ = std::fs::remove_file(&binary.path);
        }
        minidumper::LoopAction::Exit
    }

    fn on_message(&self, _kind: u32, _buffer: Vec<u8>) {}

    fn on_client_connected(&self, _clients: usize) -> minidumper::LoopAction {
        self.connected.store(true, Ordering::Relaxed);
        minidumper::LoopAction::Continue
    }

    fn on_client_disconnected(&self, clients: usize) -> minidumper::LoopAction {
        if clients == 0 {
            minidumper::LoopAction::Exit
        } else {
            minidumper::LoopAction::Continue
        }
    }
}

fn arch(cpu: minidump::system_info::Cpu) -> String {
    use minidump::system_info::Cpu;
    match cpu {
        Cpu::X86 => "x86".into(),
        Cpu::X86_64 => "x86_64".into(),
        Cpu::Arm => "arm".into(),
        Cpu::Arm64 => "arm64".into(),
        other => format!("{other}"),
    }
}

fn os_name(os: minidump::system_info::Os) -> String {
    use minidump::system_info::Os;
    match os {
        Os::Linux => "Linux".into(),
        Os::MacOs => "macOS".into(),
        Os::Windows => "Windows".into(),
        other => format!("{other}"),
    }
}

/// The dump → a report: the crashing thread's frames (innermost first) and
/// the modules they point into.
pub(crate) fn walk(path: &Path, release: &str, component: &str) -> Option<Crash> {
    use minidump::Module as _;

    let dump = minidump::Minidump::read_path(path).ok()?;
    let system_info = dump.get_stream::<minidump::MinidumpSystemInfo>().ok()?;
    let modules = dump.get_stream::<minidump::MinidumpModuleList>().unwrap_or_default();
    let provider = futures_executor::block_on(minidump_unwind::debuginfo::DebugInfoSymbolProvider::new(&system_info, &modules));
    let state = futures_executor::block_on(minidump_processor::process_minidump(&dump, &provider)).ok()?;

    let main_base = state.modules.main_module().map(|m| m.base_address());
    let thread = state.requesting_thread.and_then(|i| state.threads.get(i));
    let mut used: Vec<Module> = Vec::new();
    let frames = thread
        .map(|thread| {
            thread
                .frames
                .iter()
                .take(report::MAX_FRAMES)
                .map(|frame| {
                    let module = frame.module.as_ref().map(|module| {
                        let base = module.base_address();
                        match used.iter().position(|m| m.base == base) {
                            Some(i) => i,
                            None => {
                                used.push(Module {
                                    code_file: module.code_file().into_owned(),
                                    base,
                                    size: module.size(),
                                    debug_id: module.debug_identifier().map(|id| id.to_string()),
                                    code_id: module.code_identifier().map(|id| id.to_string()),
                                    debug_file: module.debug_file().map(|f| f.into_owned()),
                                    main: Some(base) == main_base,
                                });
                                used.len() - 1
                            }
                        }
                    });
                    Frame { address: frame.resume_address, module, trust: frame.trust.description().to_string() }
                })
                .collect()
        })
        .unwrap_or_default();

    let (reason, address) = match &state.exception_info {
        Some(info) => (info.reason.to_string(), info.address.0),
        None => ("crash".to_string(), 0),
    };
    let kind = reason.split(" / ").next().unwrap_or("crash").trim().to_string();
    Some(Crash {
        id: uuid::Uuid::new_v4().to_string(),
        release: release.to_string(),
        component: component.to_string(),
        timestamp: state.time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        os: os_name(state.system_info.os),
        os_version: state.system_info.os_version.clone(),
        arch: arch(state.system_info.cpu),
        kind,
        reason,
        address,
        thread_id: thread.map(|t| t.thread_id),
        thread_name: thread.and_then(|t| t.thread_name.clone()),
        frames,
        modules: used,
    })
}
