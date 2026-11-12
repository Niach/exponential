//! #ERR-12 end to end: this binary re-runs itself as the crashing child
//! (`EXP_FORCE_CRASH`), whose monitor (this binary again) leaves a report
//! with the release and a walked stack through this crate's own code.

use std::process::Command;

fn main() {
    crash_monitor::run_monitor_if_requested();
    if let Ok(dir) = std::env::var("CRASH_TEST_DIR") {
        crash_monitor::install(crash_monitor::Setup {
            dir: dir.into(),
            release: "crash-test@1.2.3".into(),
            component: "test".into(),
        });
        if std::env::var_os("CRASH_TEST_EXIT").is_some() {
            std::process::exit(0);
        }
        // The install thread crashes the process (`EXP_FORCE_CRASH`).
        std::thread::sleep(std::time::Duration::from_secs(30));
        eprintln!("never crashed");
        std::process::exit(3);
    }
    if !cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
        return;
    }
    for (kind, expected) in [("segv", &["SIGSEGV", "EXC_BAD_ACCESS", "EXCEPTION_ACCESS_VIOLATION_WRITE"][..]), ("abort", &["SIGABRT", "EXC_CRASH", "EXCEPTION_STACK_BUFFER_OVERRUN"][..])] {
        let dir = tempdir(kind);
        let status = Command::new(std::env::current_exe().unwrap())
            .env("CRASH_TEST_DIR", &dir)
            .env("EXP_FORCE_CRASH", kind)
            .status()
            .unwrap();
        assert!(!status.success(), "{kind}: the child must die");
        // The monitor finishes the report before acking the crash, so it
        // is there once the child is gone.
        let pending = crash_monitor::pending(&dir);
        assert_eq!(pending.len(), 1, "{kind}: one report in {}", dir.display());
        let crash = &pending[0].1;
        assert_eq!(crash.release, "crash-test@1.2.3");
        assert!(expected.contains(&crash.kind.as_str()), "{kind}: kind {}", crash.kind);
        let main = crash.modules.iter().position(|m| m.main).expect("the main module is in the stack");
        assert!(crash.modules[main].debug_id.is_some(), "{kind}: the main module has a debug id");
        let ours = crash.frames.iter().filter(|f| f.module == Some(main)).count();
        assert!(ours >= 3, "{kind}: at least 3 frames in this binary, got {}: {:#?}", ours, crash.frames);
        let event = crash.to_event();
        assert_eq!(event.release.as_deref(), Some("crash-test@1.2.3"));
        assert!(!dir.read_dir().unwrap().any(|e| e.unwrap().path().extension().is_some_and(|x| x == "dmp")), "{kind}: the dump is deleted");
        println!("{kind}: {} ({} frames, {} in this binary)", crash.reason, crash.frames.len(), ours);
        let _ = std::fs::remove_dir_all(&dir);
    }
    no_monitor_outlives_its_parent();
    println!("native_crash: ok");
}

fn tempdir(kind: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("exp-crash-test-{}-{kind}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A parent that exits right after `install` (before or after the monitor
/// connected) leaves no monitor behind.
fn no_monitor_outlives_its_parent() {
    if !cfg!(unix) {
        return;
    }
    let dir = tempdir("exit");
    let status = Command::new(std::env::current_exe().unwrap()).env("CRASH_TEST_DIR", &dir).env("CRASH_TEST_EXIT", "1").status().unwrap();
    assert!(status.success());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let tag = dir.to_string_lossy().into_owned();
    loop {
        let alive = std::fs::read_dir("/proc")
            .map(|procs| {
                procs.flatten().any(|p| std::fs::read(p.path().join("cmdline")).is_ok_and(|c| String::from_utf8_lossy(&c).contains(&tag)))
            })
            .unwrap_or(false);
        if !alive {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "a monitor outlived its parent");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("exit: no monitor left");
    let _ = std::fs::remove_dir_all(&dir);
}
