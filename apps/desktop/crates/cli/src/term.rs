//! Local-terminal plumbing: tty probes, raw mode for the interactive PTY
//! attach, window size, and a no-echo password prompt. Unix-only (the CLI
//! ships Linux + macOS; Windows is explicitly out of scope for EXP-403).

use std::io::{BufRead, Write};

pub fn stdin_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
}

pub fn stdout_is_tty() -> bool {
    unsafe { libc::isatty(libc::STDOUT_FILENO) == 1 }
}

pub fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

