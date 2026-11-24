//! EXP-1859: the speech worker alone, without the app around it: the same
//! `etalk_engine::run_and_exit` the desktop app runs as `__speech-worker`
//! (JSON requests on stdin, events on stdout; `etalk::Request` / `Event`).
//!
//!   cargo run --release -p etalk-engine --example worker -- --data-dir <data-dir>
//!
//! For measuring what the worker costs while it listens (the wake word's
//! idle CPU with one phrase and with five) against a microphone of a
//! private audio sandbox: never run it against a real data directory or a
//! real microphone.

fn main() {
    etalk_engine::run_and_exit()
}
