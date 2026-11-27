//! `exp-desktop` — the Exponential desktop IDE binary (masterplan-v3 §3.1):
//! the standalone app. Everything it does is [`app::run`].

// GUI subsystem on Windows — without this every launch drags a console
// window along. Costs us stdout/stderr there; logging goes to the log file.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    app::run(app::Mode::Standalone);
}
