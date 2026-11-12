//! #ERR-12: native crashes (SIGSEGV, SIGABRT, a stack overflow, a Mach
//! exception, Windows SEH) of the desktop IDE, the CLI daemon and eDE.
//! sentry-rust only sees Rust panics; a crash in gpui, Metal, DirectX,
//! Wayland or an `abort()` used to leave nothing behind.
//!
//! - [`install`]: spawns the monitor (this same binary, [`MONITOR_ARG`])
//!   and attaches the crash handler (`crash-handler`: signal handlers on an
//!   alternate stack, Mach exception ports, a vectored exception handler).
//! - On a crash the handler asks the monitor for a minidump (`minidumper`)
//!   and waits. The monitor writes it, walks the crashing thread with the
//!   unwind info of the binaries still on disk (rust-minidump) and keeps a
//!   small report (`<dir>/<id>.json`, [`report::Crash`]) with the CRASHED
//!   release; the dump itself is deleted.
//! - The next launch, once its Sentry client is up, sends each report as a
//!   fatal native event ([`report::send_pending`]); GlitchTip symbolicates
//!   the main executable's frames from the release's uploaded debug files.
//!   (GlitchTip's own minidump ingest is unfinished upstream: one frame.)
//!
//! `EXP_FORCE_CRASH=segv|abort` crashes the process right after the
//! handler is attached: the proof that the path works end to end.

pub mod report;
pub mod wire;

mod monitor;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub use report::{discard_pending, panic_path, pending, pending_panics, send_pending, write_panic, Crash, PANIC_EXTENSION};
pub use wire::native_debug_images;

/// `argv[1]` of a monitor process.
pub const MONITOR_ARG: &str = "__exp-crash-monitor";

/// First thing in `main`: when this process was started as a monitor, it
/// runs the monitor and exits here.
pub fn run_monitor_if_requested() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(MONITOR_ARG)) {
        return;
    }
    let rest: Vec<OsString> = args.collect();
    std::process::exit(monitor::run(&rest));
}

/// What [`install`] needs.
#[derive(Debug, Clone)]
pub struct Setup {
    /// Where reports wait for the next launch.
    pub dir: PathBuf,
    /// `exponential-desktop@0.14.150`.
    pub release: String,
    /// A tag on the event: `desktop`, `cli`, `compositor`, …
    pub component: String,
}

static INSTALLED: OnceLock<()> = OnceLock::new();

/// Spawn the monitor and attach the handler, on a background thread (the
/// monitor's socket takes a few ms to come up; startup does not wait). Once
/// per process; a failure logs and leaves the process without native crash
/// reports.
pub fn install(setup: Setup) {
    if INSTALLED.set(()).is_err() {
        return;
    }
    let spawned = std::thread::Builder::new().name("exp-crash-monitor".into()).spawn(move || {
        match attach(&setup) {
            Ok(mut monitor) => {
                force_crash_from_env();
                // Reap the monitor when it exits (the app's own exit
                // disconnects it), so it never lingers as a zombie.
                let _ = monitor.wait();
            }
            Err(error) => log::warn!(target: "observability", "native crash reports off: {error}"),
        }
    });
    if let Err(error) = spawned {
        log::warn!(target: "observability", "native crash reports off: {error}");
    }
}

static HANDLER: OnceLock<crash_handler::CrashHandler> = OnceLock::new();

fn socket_name() -> String {
    let unique = uuid::Uuid::new_v4().simple().to_string();
    format!("exp-crash-{}-{}", std::process::id(), &unique[..12])
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn socket_path(name: &str) -> String {
    name.to_string()
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn socket_path(name: &str) -> String {
    std::env::temp_dir().join(format!("{name}.sock")).to_string_lossy().into_owned()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn socket(name: &str) -> minidumper::SocketName<'_> {
    minidumper::SocketName::abstract_namespace(name)
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn socket(path: &str) -> minidumper::SocketName<'_> {
    minidumper::SocketName::path(path)
}

/// The binary to run as the monitor. Linux: `/proc/self/exe`, which still
/// resolves when an update replaced the file on disk.
fn monitor_program() -> std::io::Result<PathBuf> {
    if cfg!(any(target_os = "linux", target_os = "android")) && Path::new("/proc/self/exe").exists() {
        return Ok(PathBuf::from("/proc/self/exe"));
    }
    std::env::current_exe()
}

fn attach(setup: &Setup) -> Result<std::process::Child, String> {
    std::fs::create_dir_all(&setup.dir).map_err(|e| format!("{}: {e}", setup.dir.display()))?;
    let name = socket_path(&socket_name());
    let mut command = std::process::Command::new(monitor_program().map_err(|e| e.to_string())?);
    command
        .arg(MONITOR_ARG)
        .arg(&name)
        .arg(&setup.dir)
        .arg(&setup.release)
        .arg(&setup.component)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| format!("spawn: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    let client = loop {
        match minidumper::Client::with_name(socket(&name)) {
            Ok(client) => break client,
            Err(error) => {
                if Instant::now() > deadline || child.try_wait().ok().flatten().is_some() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("monitor unreachable: {error}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };

    // SAFETY: the closure runs in a crashed context. It only sends the
    // crash context over the already connected socket and waits for the
    // ack (`request_dump`), no allocation and no locks.
    let handler = crash_handler::CrashHandler::attach(unsafe {
        crash_handler::make_crash_event(move |context: &crash_handler::CrashContext| {
            crash_handler::CrashEventResult::Handled(client.request_dump(context).is_ok())
        })
    })
    .map_err(|e| format!("attach: {e}"))?;
    // Yama (`ptrace_scope` 1, Ubuntu's default) lets only an ancestor
    // ptrace us; the monitor is our child.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    handler.set_ptracer(Some(child.id()));
    let _ = HANDLER.set(handler);
    Ok(child)
}

/// `EXP_FORCE_CRASH=segv|abort`: crash now (the end-to-end proof).
fn force_crash_from_env() {
    match std::env::var("EXP_FORCE_CRASH").as_deref() {
        Ok("segv") => force_segv(),
        Ok("abort") => std::process::abort(),
        _ => {}
    }
}

#[inline(never)]
fn force_segv() {
    // SAFETY: none; this is the point (a write through a null pointer).
    unsafe { std::ptr::null_mut::<u32>().write_volatile(0xdead) };
}
