//! Quit watchdog (EXP-300).
//!
//! ## The failure this backstops
//!
//! `cx.quit()` → gpui dispatches `[NSApp terminate:]`. AppKit then calls
//! `applicationWillTerminate:`, which is where gpui drives `App::shutdown`
//! (quit observers → `windows.clear()` → `flush_effects()`), and AppKit only
//! calls `exit(0)` **once that method returns**. All of it runs on the main
//! thread. So anything that blocks there produces exactly the reported
//! symptom: the windows vanish, the process does not — and because macOS
//! Launch Services still has the old instance registered, the next launch is
//! delivered to the survivor as `applicationShouldHandleReopen:` instead of
//! starting a new process. The app "does nothing" until you launch it twice.
//!
//! Known blocking work on that path is individually bounded — the terminal
//! join is 1s per thread, the sync stop grace is 300ms, `drain_pending_ends`
//! is 2s — but "bounded" only holds for the paths we know about. `exit(0)`'s
//! own `__cxa_finalize` pass, running driver destructors while our threads
//! are still live, is not bounded by anything we control.
//!
//! ## Why `_exit`
//!
//! `std::process::exit` runs the atexit/`__cxa_finalize` chain, which is
//! itself one of the candidate blockers. A watchdog that could deadlock on
//! the thing it exists to escape is not a watchdog, so this leaves via
//! `_exit`: no destructors, no handlers, immediate.
//!
//! That is safe *here* specifically because it only ever fires after the app
//! has committed to quitting and the shutdown observers have already had
//! their full window to flush state to disk and to the server. It is a
//! backstop for a hang, not a shutdown path.
//!
//! ## This is not a fix
//!
//! It converts an indefinite hang into a bounded one. Landing it does not
//! close EXP-300 — the root-cause blocker is still unidentified, and the
//! logging added alongside it (`app::logging`) is what should eventually
//! name it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::App;

/// How long a quit may take before we stop waiting. Comfortably above the
/// worst legitimate teardown observed (~3.4s with a live coding session, a
/// terminal tab and a full sync pipeline) with room for a slow disk, and far
/// below the point a user decides the app is broken.
const QUIT_DEADLINE: Duration = Duration::from_secs(10);

static ARMED: AtomicBool = AtomicBool::new(false);

/// Start the watchdog. Idempotent — the quit action and the `on_app_quit`
/// observer both call it, and on a normal quit both fire.
///
/// The watchdog thread is deliberately never joined and holds no app state:
/// on a healthy quit the process is gone long before the timer elapses and
/// the thread dies with it, costing one sleeping thread and nothing else.
pub fn arm_quit_watchdog() {
    if ARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("exp-quit-watchdog".to_string())
        .spawn(|| {
            std::thread::sleep(QUIT_DEADLINE);
            // Reaching here means `applicationWillTerminate:` never returned.
            log::error!(
                "quit watchdog: still alive {QUIT_DEADLINE:?} after quit was \
                 requested — the main thread is blocked in the terminate path \
                 (EXP-300). Leaving via _exit; sample the process next time to \
                 catch the blocking frame."
            );
            log::logger().flush();
            hard_exit();
        });
    if let Err(err) = spawned {
        log::warn!("quit watchdog: could not spawn ({err}); quit is unguarded");
        ARMED.store(false, Ordering::SeqCst);
    }
}

/// Leave immediately, without running destructors or atexit handlers.
#[cfg(unix)]
fn hard_exit() -> ! {
    // SAFETY: `_exit` is async-signal-safe and never returns. Nothing in this
    // process needs to observe teardown past this point — that is the point.
    unsafe { libc::_exit(0) }
}

#[cfg(not(unix))]
fn hard_exit() -> ! {
    // No `_exit` equivalent worth an extra dependency here: EXP-300 is a macOS
    // symptom, and Windows/other platforms get the still-useful bounded exit
    // even though this one does run the atexit chain.
    std::process::exit(0)
}

/// Install the `on_app_quit` arming path.
///
/// Arming from the [`crate::actions`]-level quit action alone is not enough:
/// the macOS menu bar's "Quit Exponential" and a Dock "Quit" both reach
/// AppKit directly, so the observer is what covers those. Conversely the
/// observer runs *inside* `App::shutdown`, which is already past some of the
/// blocking work — hence arming from both ends.
pub fn init(cx: &mut App) {
    cx.on_app_quit(|_cx| {
        arm_quit_watchdog();
        async {}
    })
    .detach();
}
