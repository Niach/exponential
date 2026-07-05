//! Browser-open robustness (masterplan-v3 §5.7). OAuth and
//! every "open in browser" go through here. On Linux a misconfigured
//! `xdg-open` (it opened a *text editor* on fresh Ubuntu, hard-blocking
//! login) must never block auth, so we run an explicit fallback chain:
//! `$BROWSER` → `open::that` (xdg-open et al.) → `gio open` →
//! `x-www-browser` → `sensible-browser` → `firefox` →
//! `google-chrome`/`chromium`. If **all** fail the caller surfaces the URL in
//! a copyable dialog ("Open this in your browser to sign in") — a broken
//! opener degrades to copy-paste, never to a dead end.

use std::fmt;

/// Every launcher in the chain failed. Carries the URL so the UI can render
/// the copyable-dialog fallback, plus the attempted launchers for the log.
#[derive(Debug)]
pub struct OpenError {
    /// The URL the user must open manually.
    pub url: String,
    /// Human-readable list of what was tried (for diagnostics).
    pub attempts: Vec<String>,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not open a browser (tried: {}); open this URL manually: {}",
            self.attempts.join(", "),
            self.url
        )
    }
}

impl std::error::Error for OpenError {}

/// Open `url` in the system browser. `Err` means the entire chain failed and
/// the UI must show the copyable URL — never treat it as fatal to the flow.
pub fn open_in_browser(url: &str) -> Result<(), OpenError> {
    #[cfg(target_os = "linux")]
    {
        open_linux(url)
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS `open` / Windows ShellExecute are reliable; `open::that`
        // waits for the launcher and checks its exit status.
        open::that(url).map_err(|e| OpenError {
            url: url.to_string(),
            attempts: vec![format!("system opener ({e})")],
        })
    }
}

#[cfg(target_os = "linux")]
fn open_linux(url: &str) -> Result<(), OpenError> {
    use std::process::{Command, Stdio};

    let mut attempts: Vec<String> = Vec::new();

    // Spawn detached: launchers exit fast; a directly-launched browser owns
    // its own lifetime (we must not block on it).
    let try_spawn = |program: &str, args: &[&str], attempts: &mut Vec<String>| -> bool {
        let ok = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok();
        if !ok {
            attempts.push(format!("{program} {}", args.join(" ")));
        }
        ok
    };

    // 1. $BROWSER — the user's explicit choice always wins.
    //    Convention: colon-separated candidates, each optionally with a %s
    //    URL placeholder.
    if let Ok(browser) = std::env::var("BROWSER") {
        for candidate in browser.split(':').filter(|c| !c.trim().is_empty()) {
            let mut parts = candidate.split_whitespace();
            let Some(program) = parts.next() else { continue };
            let mut args: Vec<String> = parts.map(str::to_string).collect();
            let mut replaced = false;
            for arg in &mut args {
                if arg.contains("%s") {
                    *arg = arg.replace("%s", url);
                    replaced = true;
                }
            }
            if !replaced {
                args.push(url.to_string());
            }
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            if try_spawn(program, &arg_refs, &mut attempts) {
                return Ok(());
            }
        }
    }

    // 2. The `open` crate (xdg-open and friends, with its own internal list).
    if open::that_detached(url).is_ok() {
        return Ok(());
    }
    attempts.push("open::that_detached".to_string());

    // 3. The explicit fallback chain.
    for (program, pre_args) in [
        ("xdg-open", &[][..]),
        ("gio", &["open"][..]),
        ("x-www-browser", &[][..]),
        ("sensible-browser", &[][..]),
        ("firefox", &[][..]),
        ("google-chrome", &[][..]),
        ("chromium", &[][..]),
        ("chromium-browser", &[][..]),
    ] {
        let mut args: Vec<&str> = pre_args.to_vec();
        args.push(url);
        if try_spawn(program, &args, &mut attempts) {
            return Ok(());
        }
    }

    Err(OpenError {
        url: url.to_string(),
        attempts,
    })
}
