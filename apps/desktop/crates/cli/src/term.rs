//! Local-terminal plumbing: tty probes, raw mode for the interactive PTY
//! attach, window size, and a no-echo password prompt. Unix-only (the CLI
//! ships Linux + macOS; Windows is explicitly out of scope for EXP-403).

use std::io::{BufRead, Write};
use std::mem::MaybeUninit;

pub fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
}

pub fn stdout_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) == 1 }
}

/// The local terminal's (cols, rows), or `None` when stdout is not a tty.
pub fn window_size() -> Option<(u16, u16)> {
    let mut size = MaybeUninit::<libc::winsize>::uninit();
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) };
    if ok != 0 {
        return None;
    }
    let size = unsafe { size.assume_init() };
    (size.ws_col > 0 && size.ws_row > 0).then_some((size.ws_col, size.ws_row))
}

/// Puts the local terminal into raw mode; restores the saved termios on
/// drop (incl. panics unwinding through the guard).
pub struct RawMode {
    saved: libc::termios,
}

impl RawMode {
    pub fn enter() -> Option<Self> {
        if !stdin_is_tty() {
            return None;
        }
        let mut saved = MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, saved.as_mut_ptr()) } != 0 {
            return None;
        }
        let saved = unsafe { saved.assume_init() };
        let mut raw = saved;
        unsafe { libc::cfmakeraw(&mut raw) };
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) } != 0 {
            return None;
        }
        Some(Self { saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}

pub fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

