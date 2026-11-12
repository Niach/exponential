//! A walked crash on disk (`<dir>/<id>.json`) and the Sentry event it
//! becomes on the next launch.
//!
//! Frames of the main executable carry `instruction_addr` + `image_addr`
//! and the image's debug id, so GlitchTip symbolicates them from the debug
//! files every release uploads. GlitchTip resolves a stack trace with ONE
//! debug file and would read every other image's address against it, so a
//! frame in another module (libc, a system framework) carries no address:
//! it reads `libc.so.6+0x9eb2c` and stays as sent.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sentry_types::protocol::v7 as proto;
use serde::{Deserialize, Serialize};

/// What the monitor writes for one crash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crash {
    /// Event id (also the file stem).
    pub id: String,
    /// The CRASHED process's release (`exponential-desktop@0.14.150`), not
    /// the one that sends it.
    pub release: String,
    /// What reports (`desktop`, `cli`, `compositor`, …): a tag.
    pub component: String,
    /// Seconds since the epoch.
    pub timestamp: u64,
    /// `Linux`, `macOS`, `Windows`.
    pub os: String,
    pub os_version: Option<String>,
    /// Sentry's arch names: `x86_64`, `arm64`, `x86`, `arm`.
    pub arch: String,
    /// `SIGSEGV`, `SIGABRT`, `EXCEPTION_ACCESS_VIOLATION_READ`, …
    pub kind: String,
    /// The full crash reason (`SIGSEGV / SEGV_MAPERR`).
    pub reason: String,
    pub address: u64,
    /// The crashing thread, innermost frame first.
    pub thread_id: Option<u32>,
    pub thread_name: Option<String>,
    pub frames: Vec<Frame>,
    /// The modules the frames point into.
    pub modules: Vec<Module>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// The return address (the crashing frame: the program counter).
    pub address: u64,
    /// Index into [`Crash::modules`].
    pub module: Option<usize>,
    /// How the unwinder found it (`context`, `cfi`, `frame_pointer`, `scan`).
    pub trust: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub code_file: String,
    pub base: u64,
    pub size: u64,
    pub debug_id: Option<String>,
    pub code_id: Option<String>,
    pub debug_file: Option<String>,
    /// The process's main executable: the image whose debug files exist.
    pub main: bool,
}

/// At most this many frames (the innermost) go into an event.
pub const MAX_FRAMES: usize = 200;

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

impl Crash {
    /// The event: level fatal, unhandled, the crashed release.
    pub fn to_event(&self) -> proto::Event<'static> {
        let main = self.modules.iter().position(|m| m.main);
        // Sentry wants oldest first; the crashing frame last.
        let frames: Vec<proto::Frame> = self
            .frames
            .iter()
            .take(MAX_FRAMES)
            .rev()
            .map(|frame| {
                let module = frame.module.and_then(|i| self.modules.get(i));
                let ours = frame.module.is_some() && frame.module == main;
                let mut out = proto::Frame { in_app: Some(ours), ..Default::default() };
                match module {
                    Some(module) if ours => {
                        out.instruction_addr = Some(proto::Addr(frame.address));
                        out.image_addr = Some(proto::Addr(module.base));
                        out.package = Some(module.code_file.clone());
                    }
                    Some(module) => {
                        out.package = Some(module.code_file.clone());
                        out.function = Some(format!("{}+{:#x}", basename(&module.code_file), frame.address.wrapping_sub(module.base)));
                    }
                    None => out.function = Some(format!("{:#x}", frame.address)),
                }
                out
            })
            .collect();

        let images = self
            .modules
            .iter()
            .filter_map(|module| {
                let id = module.debug_id.as_deref()?.parse().ok()?;
                Some(proto::DebugImage::Symbolic(proto::SymbolicDebugImage {
                    name: module.code_file.clone(),
                    arch: Some(self.arch.clone()),
                    image_addr: proto::Addr(module.base),
                    image_size: module.size,
                    image_vmaddr: proto::Addr(0),
                    id,
                    code_id: module.code_id.as_deref().and_then(|c| c.parse().ok()),
                    debug_file: module.debug_file.clone(),
                }))
            })
            .collect();

        let mut mechanism = proto::Mechanism {
            ty: "minidump".into(),
            handled: Some(false),
            synthetic: Some(true),
            ..Default::default()
        };
        if let Some(number) = signal_number(&self.kind) {
            mechanism.meta.signal = Some(proto::PosixSignal { number, code: None, name: Some(self.kind.clone()), code_name: None });
        }

        let mut contexts = proto::Map::new();
        contexts.insert(
            "os".into(),
            proto::Context::Os(Box::new(proto::OsContext { name: Some(self.os.clone()), version: self.os_version.clone(), ..Default::default() })),
        );
        contexts.insert("device".into(), proto::Context::Device(Box::new(proto::DeviceContext { arch: Some(self.arch.clone()), ..Default::default() })));

        let mut tags = proto::Map::new();
        tags.insert("component".into(), self.component.clone());
        tags.insert("crash".into(), "native".into());

        proto::Event {
            event_id: self.id.parse().unwrap_or_default(),
            level: proto::Level::Fatal,
            platform: "native".into(),
            release: Some(self.release.clone().into()),
            timestamp: UNIX_EPOCH + Duration::from_secs(self.timestamp),
            exception: vec![proto::Exception {
                ty: self.kind.clone(),
                value: Some(format!("{} at {:#x}", self.reason, self.address)),
                stacktrace: (!frames.is_empty()).then_some(proto::Stacktrace { frames, ..Default::default() }),
                thread_id: self.thread_id.map(|id| proto::ThreadId::Int(u64::from(id))),
                mechanism: Some(mechanism),
                ..Default::default()
            }]
            .into(),
            debug_meta: std::borrow::Cow::Owned(proto::DebugMeta { images, ..Default::default() }),
            contexts,
            tags,
            ..Default::default()
        }
    }
}

/// POSIX numbers for the signals a crash reason names (Linux/macOS share these).
fn signal_number(kind: &str) -> Option<i32> {
    Some(match kind {
        "SIGILL" => 4,
        "SIGTRAP" => 5,
        "SIGABRT" => 6,
        "SIGBUS" => if cfg!(target_os = "macos") { 10 } else { 7 },
        "SIGFPE" => 8,
        "SIGSEGV" => 11,
        "SIGSYS" => if cfg!(target_os = "macos") { 12 } else { 31 },
        _ => return None,
    })
}

/// Unsent crashes kept at most (the newest).
pub const MAX_PENDING: usize = 5;
/// Older ones are dropped unsent.
pub const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);

/// The crashes waiting in `dir`, oldest first, after pruning: beyond
/// [`MAX_PENDING`], older than [`MAX_AGE`], unreadable, and dumps the
/// monitor never finished (`*.dmp` without a report) are deleted.
pub fn pending(dir: &Path) -> Vec<(std::path::PathBuf, Crash)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut crashes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => match std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Crash>(&bytes).ok()) {
                Some(crash) if now.saturating_sub(crash.timestamp) <= MAX_AGE.as_secs() => crashes.push((path, crash)),
                _ => {
                    let _ = std::fs::remove_file(&path);
                }
            },
            // A dump is only kept while its monitor walks it.
            Some("dmp") => {
                let stale = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age > Duration::from_secs(600));
                if stale {
                    let _ = std::fs::remove_file(&path);
                }
            }
            _ => {}
        }
    }
    crashes.sort_by_key(|(_, crash)| crash.timestamp);
    let excess = crashes.len().saturating_sub(MAX_PENDING);
    for (path, _) in crashes.drain(..excess) {
        let _ = std::fs::remove_file(path);
    }
    crashes
}

/// Hand every pending crash to `send` (an event per crash), then `flush`;
/// the reports are deleted only once `flush` says they went out (a crash
/// right after startup must not lose the last one). A resend after a
/// failed flush carries the same event id. Returns how many were handed
/// over.
pub fn send_pending(dir: &Path, mut send: impl FnMut(proto::Event<'static>), flush: impl FnOnce() -> bool) -> usize {
    let crashes = pending(dir);
    let count = crashes.len();
    if count == 0 {
        return 0;
    }
    for (_, crash) in &crashes {
        send(crash.to_event());
    }
    if flush() {
        for (path, _) in crashes {
            let _ = std::fs::remove_file(path);
        }
    }
    count
}

/// Delete every pending crash unsent (diagnostics are off).
pub fn discard_pending(dir: &Path) {
    for (path, _) in pending(dir) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn write(dir: &Path, crash: &Crash) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{}.json.tmp", crash.id));
    std::fs::write(&tmp, serde_json::to_vec(crash).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, dir.join(format!("{}.json", crash.id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crash() -> Crash {
        Crash {
            id: "6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11".into(),
            release: "exponential-desktop@0.14.150".into(),
            component: "desktop".into(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            os: "Linux".into(),
            os_version: Some("6.17.0".into()),
            arch: "x86_64".into(),
            kind: "SIGSEGV".into(),
            reason: "SIGSEGV / SEGV_MAPERR".into(),
            address: 0,
            thread_id: Some(42),
            thread_name: Some("main".into()),
            frames: vec![
                Frame { address: 0x5555_0000_1234, module: Some(0), trust: "context".into() },
                Frame { address: 0x7fff_0009_eb2c, module: Some(1), trust: "cfi".into() },
                Frame { address: 0x5555_0000_0100, module: Some(0), trust: "cfi".into() },
                Frame { address: 0xdead, module: None, trust: "scan".into() },
            ],
            modules: vec![
                Module {
                    code_file: "/usr/bin/exp-desktop".into(),
                    base: 0x5555_0000_0000,
                    size: 0x100_0000,
                    debug_id: Some("b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf".into()),
                    code_id: Some("b3b2b1b0b5b4b7b6b8b9babbbcbdbebf".into()),
                    debug_file: Some("/usr/bin/exp-desktop".into()),
                    main: true,
                },
                Module {
                    code_file: "/usr/lib/x86_64-linux-gnu/libc.so.6".into(),
                    base: 0x7fff_0000_0000,
                    size: 0x20_0000,
                    debug_id: Some("c0c1c2c3-c4c5-c6c7-c8c9-cacbcccdcecf".into()),
                    code_id: None,
                    debug_file: None,
                    main: false,
                },
            ],
        }
    }

    #[test]
    fn the_event_carries_the_crashed_release_and_a_native_stack() {
        let event = crash().to_event();
        assert_eq!(event.release.as_deref(), Some("exponential-desktop@0.14.150"));
        assert_eq!(event.level, proto::Level::Fatal);
        assert_eq!(event.platform, "native");
        assert_eq!(event.event_id.to_string(), "6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11");
        let exception = &event.exception.values[0];
        assert_eq!(exception.ty, "SIGSEGV");
        assert_eq!(exception.value.as_deref(), Some("SIGSEGV / SEGV_MAPERR at 0x0"));
        let mechanism = exception.mechanism.as_ref().unwrap();
        assert_eq!(mechanism.handled, Some(false));
        assert_eq!(mechanism.meta.signal.as_ref().unwrap().number, 11);
        assert_eq!(event.tags.get("component").map(String::as_str), Some("desktop"));
    }

    #[test]
    fn frames_go_oldest_first_and_only_the_main_image_keeps_addresses() {
        let event = crash().to_event();
        let frames = &event.exception.values[0].stacktrace.as_ref().unwrap().frames;
        assert_eq!(frames.len(), 4);
        // Oldest first: the unknown frame, then main, libc, the crash.
        assert_eq!(frames[0].function.as_deref(), Some("0xdead"));
        assert_eq!(frames[0].instruction_addr, None);
        assert_eq!(frames[1].instruction_addr, Some(proto::Addr(0x5555_0000_0100)));
        assert_eq!(frames[1].image_addr, Some(proto::Addr(0x5555_0000_0000)));
        assert_eq!(frames[1].in_app, Some(true));
        assert_eq!(frames[2].instruction_addr, None);
        assert_eq!(frames[2].function.as_deref(), Some("libc.so.6+0x9eb2c"));
        assert_eq!(frames[2].in_app, Some(false));
        assert_eq!(frames[3].instruction_addr, Some(proto::Addr(0x5555_0000_1234)));
    }

    #[test]
    fn debug_meta_lists_the_images_by_debug_id() {
        let event = crash().to_event();
        assert_eq!(event.debug_meta.images.len(), 2);
        let proto::DebugImage::Symbolic(image) = &event.debug_meta.images[0] else { panic!("symbolic image") };
        assert_eq!(image.id.to_string(), "b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf");
        assert_eq!(image.image_addr, proto::Addr(0x5555_0000_0000));
        assert_eq!(image.arch.as_deref(), Some("x86_64"));
        assert!(image.code_id.is_some());
    }

    #[test]
    fn pending_sends_once_then_deletes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &crash()).unwrap();
        let mut sent = Vec::new();
        assert_eq!(send_pending(dir.path(), |event| sent.push(event), || true), 1);
        assert_eq!(sent[0].release.as_deref(), Some("exponential-desktop@0.14.150"));
        assert_eq!(send_pending(dir.path(), |_| panic!("sent twice"), || true), 0);
    }

    #[test]
    fn an_unflushed_send_keeps_the_report_with_the_same_event_id() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &crash()).unwrap();
        let mut ids = Vec::new();
        assert_eq!(send_pending(dir.path(), |event| ids.push(event.event_id), || false), 1);
        assert_eq!(send_pending(dir.path(), |event| ids.push(event.event_id), || true), 1);
        assert_eq!(ids[0], ids[1]);
        assert!(pending(dir.path()).is_empty());
    }

    #[test]
    fn pending_keeps_the_newest_and_drops_the_old_and_the_broken() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(MAX_PENDING as u64 + 3) {
            let mut c = crash();
            c.id = uuid::Uuid::new_v4().to_string();
            c.timestamp -= 100 - i;
            write(dir.path(), &c).unwrap();
        }
        let mut old = crash();
        old.id = uuid::Uuid::new_v4().to_string();
        old.timestamp -= MAX_AGE.as_secs() + 60;
        write(dir.path(), &old).unwrap();
        std::fs::write(dir.path().join("broken.json"), b"{").unwrap();

        let kept = pending(dir.path());
        assert_eq!(kept.len(), MAX_PENDING);
        assert!(kept.windows(2).all(|w| w[0].1.timestamp <= w[1].1.timestamp));
        assert!(kept.iter().all(|(_, c)| c.id != old.id));
        assert!(!dir.path().join("broken.json").exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), MAX_PENDING);
    }

    #[test]
    fn discard_deletes_unsent() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &crash()).unwrap();
        discard_pending(dir.path());
        assert!(pending(dir.path()).is_empty());
    }
}
