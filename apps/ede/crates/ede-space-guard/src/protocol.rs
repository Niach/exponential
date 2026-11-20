//! What `space-enter` asks the guard, and where. One line each way:
//! `probe` | `enter <spaces>`, then `ok` | `no <reason>`.

use std::path::Path;

/// The guard's socket (`exponential-space-guard.socket`).
pub const SOCKET: &str = "/run/exponential-space-guard.sock";
/// The longest line either side sends.
pub const MAX_LINE: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Can the guard do its work on this machine (it loads its programs)?
    Probe,
    /// Guard the space slice the asking process is in. `spaces` = the
    /// compositor's directory of spaces, whose sockets stay reachable.
    Enter { spaces: String },
}

impl Request {
    pub fn line(&self) -> String {
        match self {
            Request::Probe => "probe\n".into(),
            Request::Enter { spaces } => format!("enter {spaces}\n"),
        }
    }

    pub fn parse(line: &str) -> Result<Request, String> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        match line.split_once(' ') {
            None if line == "probe" => Ok(Request::Probe),
            Some(("enter", spaces)) if !spaces.is_empty() => Ok(Request::Enter { spaces: spaces.to_string() }),
            _ => Err("not a request".into()),
        }
    }
}

/// Is `spaces` a directory of spaces of user `uid`'s compositor:
/// `/run/user/<uid>/exponential-desktop/<name>.spaces`, `<name>` a
/// Wayland socket's name? Only the shape: the guard cannot see into the
/// person's runtime directory, and a wrong one only weakens the caller's
/// own guard.
pub fn spaces_dir_of(spaces: &str, uid: u32) -> Result<(), String> {
    let prefix = format!("/run/user/{uid}/exponential-desktop/");
    let name = spaces.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix(".spaces")).ok_or_else(|| format!("{spaces:?} is not a directory of spaces of user {uid}"))?;
    let ok = !name.is_empty() && name.len() <= 64 && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if ok && Path::new(spaces).components().count() == 6 {
        Ok(())
    } else {
        Err(format!("{spaces:?} is not a directory of spaces of user {uid}"))
    }
}

/// The reply line for `result`.
pub fn reply(result: &Result<(), String>) -> String {
    match result {
        Ok(()) => "ok\n".into(),
        // One line, whatever the reason says.
        Err(reason) => format!("no {}\n", reason.replace('\n', " ").chars().take(MAX_LINE - 4).collect::<String>()),
    }
}

/// What a reply line says.
pub fn parse_reply(line: &str) -> Result<(), String> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    match line {
        "ok" => Ok(()),
        _ => Err(line.strip_prefix("no ").unwrap_or("the guard's answer is not a reply").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_replies_are_one_line_each() {
        for request in [Request::Probe, Request::Enter { spaces: "/run/user/1000/exponential-desktop/wayland-1.spaces".into() }] {
            assert_eq!(Request::parse(&request.line()), Ok(request));
        }
        for bad in ["", "enter", "enter ", "probe x", "attach /x", "PROBE"] {
            assert!(Request::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse_reply(&reply(&Ok(()))), Ok(()));
        assert_eq!(parse_reply(&reply(&Err("not\nhere".into()))), Err("not here".into()));
        assert!(parse_reply("").is_err());
    }

    #[test]
    fn only_a_compositors_directory_of_spaces_of_the_callers_own_uid() {
        assert_eq!(spaces_dir_of("/run/user/1000/exponential-desktop/wayland-1.spaces", 1000), Ok(()));
        for bad in [
            "/run/user/1000/exponential-desktop/wayland-1.spaces",
            "/run/user/1001/exponential-desktop/wayland-1.spaces/..",
            "/run/user/1001/exponential-desktop/../wayland-1.spaces",
            "/run/user/1001/exponential-desktop/.spaces",
            "/run/user/1001/exponential-desktop/a/b.spaces",
            "/run/user/1001/exponential-desktop/..spaces",
            "/run/user/1001/exponential-desktop/x y.spaces",
            "/run/user/1001",
            "/",
        ] {
            assert!(spaces_dir_of(bad, 1001).is_err(), "{bad}");
        }
    }
}
