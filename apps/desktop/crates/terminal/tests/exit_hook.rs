// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! §6.7 / §11.4 Phase-4 gate: a closing child fires the tab's one-shot
//! [`terminal::ExitHook`] with the captured exit code.
//!
//! This is the only terminal test that needs a live gpui `App` (the hook is
//! `FnOnce(TabId, &ChildExit, &mut App)` and fires from the manager's
//! `TerminalViewEvent::Exited` subscription), so it runs a real HEADLESS
//! platform — no window, no rendering. `harness = false` because the platform
//! run loop must own the process main thread; libtest runs `#[test]` fns on
//! worker threads where the macOS main-queue dispatcher never drains.
//!
//! Self-skips unless `EXP_EXIT_HOOK_E2E=1` (plain `cargo test` stays green):
//!
//! ```sh
//! EXP_EXIT_HOOK_E2E=1 cargo test -p terminal --test exit_hook
//! ```
//!
//! Pass/fail is the process exit code (a watchdog thread outside gpui decides
//! and exits — on macOS the platform run loop never returns to `main`).
//!
//! **Phase-5 deferral (§6.7):** ending the `coding_sessions` row on this edge
//! is the Start-coding launcher's wiring; this proves the seam it consumes.

use gpui::AppContext as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use terminal::{ChildExit, SpawnSpec, TabKind, TerminalManager};

fn main() {
    if std::env::var("EXP_EXIT_HOOK_E2E").as_deref() != Ok("1") {
        eprintln!(
            "exit_hook e2e: skipped (set EXP_EXIT_HOOK_E2E=1 to run — needs the process \
             main thread + a live platform run loop)"
        );
        return;
    }

    // The hook writes here; the watchdog (outside gpui) asserts + exits.
    let fired: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

    let watchdog = Arc::clone(&fired);
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(code) = *watchdog.lock().unwrap() {
                if code == 7 {
                    eprintln!("exit_hook e2e: PASS — on_exit fired once with code 7");
                    std::process::exit(0);
                }
                eprintln!("exit_hook e2e: FAIL — on_exit fired with code {code}, want 7");
                std::process::exit(2);
            }
            if Instant::now() >= deadline {
                eprintln!("exit_hook e2e: FAIL — on_exit never fired within 30s");
                std::process::exit(3);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    gpui_platform::headless().run(move |cx| {
        let manager = cx.new(|_| TerminalManager::new());
        let hook_fired = Arc::clone(&fired);
        manager.update(cx, |manager, cx| {
            manager
                .open_tab(
                    TabKind::Shell,
                    "exit-hook",
                    &SpawnSpec::new("sh").args(["-c", "exit 7"]),
                    Some(Box::new(move |_id, exit: &ChildExit, _cx: &mut gpui::App| {
                        *hook_fired.lock().unwrap() = Some(exit.code);
                    })),
                    cx,
                )
                .expect("spawn sh into a tab");
        });
        // Keep the manager entity (and its view subscription) alive for the
        // app's lifetime — the watchdog exits the process when done.
        std::mem::forget(manager);
    });
}
