//! #ERR-29: a helper process of the binary (the speech worker) reports its
//! native crash like the binary: into the host's `{data_dir}/crashes`, with
//! the binary's release and the helper's component; nothing when the
//! person's "Send diagnostics" switch is off. This binary re-runs itself as
//! the helper (`EXP_FORCE_CRASH` crashes it once the monitor is attached).

use std::process::Command;
use std::time::Duration;

fn main() {
    observability::run_crash_monitor_if_requested();
    if let Some(dir) = std::env::var_os("HELPER_DATA_DIR") {
        observability::watch_helper_crashes(observability::Product::Desktop, "1.2.3", dir.as_ref(), "speech-worker");
        // With consent the monitor's thread crashes the process; without,
        // nothing does.
        let wait = if std::env::var_os("HELPER_OFF").is_some() { 1 } else { 30 };
        std::thread::sleep(Duration::from_secs(wait));
        std::process::exit(if wait == 1 { 0 } else { 3 });
    }
    if !cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows")) {
        return;
    }
    let run = |dir: &std::path::Path, off: bool| {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.env("HELPER_DATA_DIR", dir).env("EXP_FORCE_CRASH", "segv").env_remove("EXP_DIAGNOSTICS");
        if off {
            child.env("HELPER_OFF", "1");
        }
        child.status().unwrap()
    };

    let dir = tempdir("on");
    assert!(!run(&dir, false).success(), "the helper must die");
    let pending = crash_monitor::pending(&dir.join("crashes"));
    assert_eq!(pending.len(), 1, "one report in {}", dir.display());
    let crash = &pending[0].1;
    assert_eq!(crash.release, "exponential-desktop@1.2.3", "the binary's release");
    let event = crash.to_event();
    assert_eq!(event.tags.get("component").map(String::as_str), Some("speech-worker"), "{:?}", event.tags);
    let _ = std::fs::remove_dir_all(&dir);

    // The switch off: no monitor, no report.
    let dir = tempdir("off");
    std::fs::write(dir.join("diagnostics.json"), r#"{"sendDiagnostics":false}"#).unwrap();
    assert!(run(&dir, true).success(), "no monitor crashed it");
    assert!(!dir.join("crashes").exists(), "nothing watched without consent");
    let _ = std::fs::remove_dir_all(&dir);
    println!("helper_crash: ok");
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("obs-helper-crash-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
