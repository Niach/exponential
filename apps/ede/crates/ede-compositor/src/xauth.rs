//! #EDE-138: X authority. Every Xwayland of this compositor (the person's,
//! each agent space's) starts with `-auth <file>`: it admits only a client
//! that shows the cookie in that file (MIT-MAGIC-COOKIE-1), and the file
//! reaches that desk's programs as `XAUTHORITY`. Without it, any process of
//! the user could open the person's X server over its abstract socket:
//! `DISPLAY=:0 xdotool type …` from a space would type into the person's
//! X11 apps.
//!
//! The files are `$XDG_RUNTIME_DIR/xauth_<tag>` (the person's) and
//! `$XDG_RUNTIME_DIR/xauth_<tag>-space-<n>` (a space's), `<tag>` being the
//! compositor's Wayland socket name. They use those names because a snap
//! reads `XAUTHORITY` itself: `snap run` copies only a file under `/tmp`,
//! and the x11 interface's AppArmor rule lets a snap read
//! `/run/user/<uid>/xauth_*` and three names of mutter's, nothing else. A
//! file anywhere else would leave every snapped X11 app without its cookie.
//! The person's path stays the same for the compositor's life, so the
//! `XAUTHORITY` the user manager and D-Bus activation were given stays
//! right when Xwayland starts again. A space's goes when the space closes.
//! One tag per running compositor means a nested compositor never takes
//! the session's files.
//!
//! The file holds two entries with the same cookie: FamilyLocal for the
//! machine's host name (what Xlib looks up first) and FamilyWild (any host
//! name, so a host name that changes during the session breaks nothing).

use std::{
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

/// The only scheme the servers take.
pub const MIT_MAGIC_COOKIE: &[u8] = b"MIT-MAGIC-COOKIE-1";
const FAMILY_LOCAL: u16 = 256;
const FAMILY_WILD: u16 = 65535;

/// A cookie: 16 bytes from the kernel's random source.
pub fn new_cookie() -> std::io::Result<[u8; 16]> {
    let mut cookie = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut cookie)?;
    Ok(cookie)
}

/// The machine's host name, as Xlib looks it up for a local display.
fn hostname() -> Option<Vec<u8>> {
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname").ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.as_bytes().to_vec())
}

/// The file's bytes: one entry per family, for display `display` (`None`
/// before the server has one: no number, which a server ignores anyway).
fn entries(display: Option<u32>, cookie: &[u8], hostname: Option<&[u8]>) -> Vec<u8> {
    let number = display.map(|n| n.to_string()).unwrap_or_default();
    let mut out = Vec::new();
    let field = |out: &mut Vec<u8>, bytes: &[u8]| {
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(bytes);
    };
    for (family, address) in hostname.map(|host| (FAMILY_LOCAL, host)).into_iter().chain([(FAMILY_WILD, &b""[..])]) {
        out.extend_from_slice(&family.to_be_bytes());
        field(&mut out, address);
        field(&mut out, number.as_bytes());
        field(&mut out, MIT_MAGIC_COOKIE);
        field(&mut out, cookie);
    }
    out
}

/// Writes `cookie` for display `display` to `path`: readable by the user
/// alone, and whole or not at all (a reader never sees half a file).
pub fn write(path: &Path, display: Option<u32>, cookie: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    file.write_all(&entries(display, cookie, hostname().as_deref()))?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// The MIT-MAGIC-COOKIE-1 cookie the file at `path` holds for display
/// `display` (an entry with that number, or one without a number).
pub fn cookie_for(path: &Path, display: u32) -> Option<Vec<u8>> {
    fn take<'a>(bytes: &'a [u8], at: &mut usize, len: usize) -> Option<&'a [u8]> {
        let field = bytes.get(*at..*at + len)?;
        *at += len;
        Some(field)
    }
    fn counted<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
        let len = take(bytes, at, 2)?;
        take(bytes, at, u16::from_be_bytes([len[0], len[1]]) as usize)
    }
    let bytes = std::fs::read(path).ok()?;
    let number = display.to_string();
    let mut at = 0usize;
    while take(&bytes, &mut at, 2).is_some() {
        let (_address, entry_number, name, data) = (counted(&bytes, &mut at)?, counted(&bytes, &mut at)?, counted(&bytes, &mut at)?, counted(&bytes, &mut at)?);
        if name == MIT_MAGIC_COOKIE && (entry_number.is_empty() || entry_number == number.as_bytes()) {
            return Some(data.to_vec());
        }
    }
    None
}

/// Where the files are: `$XDG_RUNTIME_DIR` (the person's), else the
/// temporary directory.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()).map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// A compositor's tag in the file names: its Wayland socket's name, else
/// a numbered name of this process (tests run several compositors).
pub fn tag(socket: Option<&str>) -> String {
    static SERIAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    match socket {
        Some(socket) => socket.to_string(),
        None => format!("exponential-{}-{}", std::process::id(), SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
    }
}

/// The person's file.
pub fn person_path(runtime: &Path, tag: &str) -> PathBuf {
    runtime.join(format!("xauth_{tag}"))
}

/// Space `space`'s file (`space-n`).
pub fn space_path(runtime: &Path, tag: &str, space: &str) -> PathBuf {
    runtime.join(format!("xauth_{tag}-{space}"))
}

/// Removes the spaces' files of compositor `tag`: at its end, and at its
/// start (what one that crashed on the same socket left).
pub fn remove_spaces(runtime: &Path, tag: &str) {
    let prefix = format!("xauth_{tag}-space-");
    let Ok(entries) = std::fs::read_dir(runtime) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_str().is_some_and(|name| name.starts_with(&prefix)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_holds_the_cookie_for_its_display_in_both_families_and_only_for_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wayland-1.Xauthority");
        let cookie = new_cookie().unwrap();
        assert_ne!(cookie, new_cookie().unwrap());
        write(&path, Some(3), &cookie).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(cookie_for(&path, 3).as_deref(), Some(&cookie[..]));
        assert_eq!(cookie_for(&path, 4), None, "another display's");
        // Byte for byte what Xlib reads: family, address, number, name, data.
        let bytes = entries(Some(3), &[7; 16], Some(b"mint"));
        let mut expected = vec![1, 0, 0, 4];
        expected.extend_from_slice(b"mint");
        expected.extend_from_slice(&[0, 1, b'3', 0, 18]);
        expected.extend_from_slice(MIT_MAGIC_COOKIE);
        expected.extend_from_slice(&[0, 16]);
        expected.extend_from_slice(&[7; 16]);
        expected.extend_from_slice(&[0xff, 0xff, 0, 0, 0, 1, b'3', 0, 18]);
        expected.extend_from_slice(MIT_MAGIC_COOKIE);
        expected.extend_from_slice(&[0, 16]);
        expected.extend_from_slice(&[7; 16]);
        assert_eq!(bytes, expected);
        // Before the server has a number: any display's.
        write(&path, None, &cookie).unwrap();
        assert_eq!(cookie_for(&path, 12).as_deref(), Some(&cookie[..]));
        // Names a snap's AppArmor rule reads (`/run/user/*/xauth_*`).
        let run = Path::new("/run/user/1000");
        assert_eq!(person_path(run, &tag(Some("wayland-1"))), PathBuf::from("/run/user/1000/xauth_wayland-1"));
        assert_eq!(space_path(run, "wayland-1", "space-3"), PathBuf::from("/run/user/1000/xauth_wayland-1-space-3"));
        assert_ne!(tag(None), tag(None), "one per compositor of a test process");
        // A compositor's spaces' files go, nobody else's.
        let runtime = tempfile::tempdir().unwrap();
        for name in ["xauth_wayland-1", "xauth_wayland-1-space-1", "xauth_wayland-1-space-2", "xauth_wayland-2-space-1"] {
            std::fs::write(runtime.path().join(name), b"").unwrap();
        }
        remove_spaces(runtime.path(), "wayland-1");
        let mut left: Vec<String> = std::fs::read_dir(runtime.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["xauth_wayland-1", "xauth_wayland-2-space-1"]);
    }
}
