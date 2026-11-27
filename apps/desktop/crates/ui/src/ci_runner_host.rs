//! #EXP-2174: the CI runner inside the desktop app (Linux first). The app
//! advertises `ci-runner` only where its namespace sandbox can hide its data
//! directory from a job (#EXP-2181); macOS and Windows desktops cannot yet, so
//! they never advertise it and their runner switch says why (runner-settings
//! fixture). The runner itself is the `ci-runner` crate the daemon shares.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// How long a quit or an update's relaunch waits for its CI jobs to stop (a
/// step's SIGTERM grace is 5 s) before it goes on.
pub(crate) const STOP_WAIT: Duration = Duration::from_secs(8);

/// The completion of a `ci_job` this app cannot run.
pub(crate) const UNAVAILABLE: &str = "This computer cannot hide the app's data from CI jobs.";

/// The app's runner, `None` off Linux.
pub(crate) fn runner() -> Option<Arc<ci_runner::Runner>> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    static RUNNER: OnceLock<Option<Arc<ci_runner::Runner>>> = OnceLock::new();
    RUNNER
        .get_or_init(|| {
            let helper = std::env::current_exe().ok()?;
            Some(Arc::new(ci_runner::Runner::desktop(helper)))
        })
        .clone()
}

/// Whether this app can run CI jobs here (probed once: the sandbox must work).
pub(crate) fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| match runner().map(|runner| runner.readiness()) {
        Some(Ok(())) => true,
        Some(Err(reason)) => {
            log::warn!("[ci] this app runs no CI jobs here: {reason}");
            false
        }
        None => false,
    })
}
