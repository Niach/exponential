//! First-party screen consent: the Exponential app needs no "Allow
//! Exponential to use your screen?" when the compositor can tell it is the
//! genuine one.
//!
//! The app is the person's own AppImage (`exp-desktop` in a FUSE mount at a
//! new path each start), not a root-owned file: no path, unit, cgroup or
//! name tells it apart from a copy. What does is a signature. The desktop
//! build (`apps/desktop/scripts/sign-binaries.sh`) puts a list of every ELF
//! file the AppImage ships (`<sha256>  usr/bin/exp-desktop`, its bundled
//! libraries) at [`LIST`], signed with the self-updater's minisign key
//! (EXP-1430). The compositor pins that key ([`pinned_key`]). A process
//! passes ([`check`]) when all of these hold:
//!
//! - its executable's content is the list's `usr/bin/<its name>`;
//! - the list's signature verifies under the pinned key, and its trusted
//!   comment says it is a binaries list (the release feed's
//!   `SHA256SUMS.txt` carries the same key, never that comment);
//! - nothing traces it (`TracerPid` 0), and it was not started with
//!   `LD_PRELOAD` or `LD_AUDIT`;
//! - every file it maps executable is its executable, a root-owned file
//!   (the system's libraries), or a file whose content the list holds (the
//!   AppImage's own libraries). A genuine `exp-desktop` beside a library of
//!   someone else's fails here.
//!
//! Anything that fails is asked as before (#EDE-172's D2a), and so is an
//! older app without a list. What this does not stop, as for
//! [`crate::peer`]: code put into the genuine process later by its parent
//! (ptrace from the process that started it, which Yama's default allows).

use std::{
    collections::HashMap,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use crate::peer::Exe;

/// The signed list, relative to the AppImage's root (`usr/bin/..`).
pub const LIST: &str = "usr/share/exponential/binaries.sha256";
/// Its minisign signature, beside it.
pub const SIGNATURE: &str = "usr/share/exponential/binaries.sha256.minisig";
/// What the signed trusted comment starts with.
pub const COMMENT: &str = "exponential-binaries ";
/// A list larger than this is not read (the real one is a few KB).
const MAX_LIST: u64 = 1 << 20;

/// The updater's minisign public key this build pins (`EXP_UPDATE_PUBKEY`
/// at build time, as the desktop app's updater): none = every app is asked.
pub fn pinned_key() -> Option<String> {
    option_env!("EXP_UPDATE_PUBKEY").map(str::trim).filter(|k| !k.is_empty()).map(str::to_string)
}

/// A verified binaries list: each file's path and content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    entries: Vec<(String, [u8; 32])>,
}

impl List {
    /// `text` signed by `signature` under `key`, with a binaries list's
    /// trusted comment; then parsed (`<hex>  <path>` lines).
    pub fn verified(text: &[u8], signature: &str, key: &str) -> Result<List, String> {
        let key = minisign_verify::PublicKey::from_base64(key.trim()).map_err(|e| format!("the pinned key is invalid ({e})"))?;
        let signature = minisign_verify::Signature::decode(signature).map_err(|e| format!("the list's signature is malformed ({e})"))?;
        // Prehashed signatures only, as the updater.
        key.verify(text, &signature, false).map_err(|e| format!("the list's signature does not verify ({e})"))?;
        if !signature.trusted_comment().starts_with(COMMENT) {
            return Err("the signed file is not a binaries list".into());
        }
        let text = std::str::from_utf8(text).map_err(|_| "the list is not text".to_string())?;
        let mut entries = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let (hex, path) = line.split_once("  ").ok_or_else(|| format!("a list line is malformed: {line:?}"))?;
            let hash = crate::consent::hex32(hex).ok_or_else(|| format!("a list line is malformed: {line:?}"))?;
            entries.push((path.trim_start_matches('*').to_string(), hash));
        }
        Ok(List { entries })
    }

    /// The list says `path` has the content `hash`.
    pub fn names(&self, path: &str, hash: &[u8; 32]) -> bool {
        self.entries.iter().any(|(p, h)| p == path && h == hash)
    }

    /// Some file of the list has the content `hash`.
    pub fn holds(&self, hash: &[u8; 32]) -> bool {
        self.entries.iter().any(|(_, h)| h == hash)
    }
}

/// Process `pid` (started at `start`, running `exe` whose content hashes
/// to `exe_hash`) is a genuine, untampered Exponential binary under `key`.
/// `Err` = why not (logged; the person is asked).
pub fn check(pid: u32, start: u64, exe: &Exe, exe_hash: &[u8; 32], key: &str) -> Result<(), String> {
    let path = exe.path.to_string_lossy();
    if path.ends_with(" (deleted)") {
        return Err("its executable was replaced while it ran".into());
    }
    let name = exe.path.file_name().and_then(|n| n.to_str()).ok_or("its executable has no name")?;
    // `<root>/usr/bin/<name>`.
    let root = exe.path.parent().filter(|d| d.ends_with("usr/bin")).and_then(Path::parent).and_then(Path::parent).ok_or("its executable is not in a usr/bin")?;
    let list = read_list(root, key)?;
    if !list.names(&format!("usr/bin/{name}"), exe_hash) {
        return Err(format!("its executable is not the signed usr/bin/{name}"));
    }
    untampered(pid, exe, &list)?;
    // Still the process the proc files were read of.
    if crate::peer::stat_of(pid).is_none_or(|(now, _)| now != start) {
        return Err("the process ended".into());
    }
    Ok(())
}

/// The verified list under `root` ([`LIST`], [`SIGNATURE`]).
pub fn read_list(root: &Path, key: &str) -> Result<List, String> {
    let mut text = Vec::new();
    std::fs::File::open(root.join(LIST))
        .map_err(|e| format!("no signed binaries list ({e})"))?
        .take(MAX_LIST)
        .read_to_end(&mut text)
        .map_err(|e| format!("the binaries list: {e}"))?;
    let signature = std::fs::read_to_string(root.join(SIGNATURE)).map_err(|e| format!("the binaries list has no signature ({e})"))?;
    List::verified(&text, &signature, key)
}

/// Nothing traces process `pid`, its start asked for no preloaded or
/// auditing library, and every executable mapping is accounted for (its
/// executable, a root-owned file, or a file the list holds).
pub fn untampered(pid: u32, exe: &Exe, list: &List) -> Result<(), String> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).map_err(|e| format!("its status: {e}"))?;
    let tracer = status.lines().find_map(|l| l.strip_prefix("TracerPid:")).map(str::trim).ok_or("its status has no TracerPid")?;
    if tracer != "0" {
        return Err(format!("it is traced (by process {tracer})"));
    }
    let environ = std::fs::read(format!("/proc/{pid}/environ")).map_err(|e| format!("its environment: {e}"))?;
    if let Some(var) = preloads(&environ) {
        return Err(format!("it was started with {var}"));
    }
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| format!("its mappings: {e}"))?;
    let mut hashed: HashMap<(PathBuf, u64), bool> = HashMap::new();
    for mapping in executable_mappings(&maps) {
        let Mapping { inode, path } = mapping;
        if inode == exe.ino && path == exe.path {
            continue;
        }
        if path.to_str().is_none_or(|p| p.starts_with("/memfd:") || p.starts_with("/SYSV")) {
            return Err(format!("it maps {}, which is nobody's file", path.display()));
        }
        if let Some(gone) = path.to_str().and_then(|p| p.strip_suffix(" (deleted)")) {
            // A system library an upgrade replaced: only where nobody but
            // root can put a file.
            if Path::new(gone).parent().is_some_and(root_owned_dir) {
                continue;
            }
            return Err(format!("it maps {} (deleted)", Path::new(gone).display()));
        }
        if crate::peer::root_owned(&path).is_some_and(|(_, ino)| ino == inode) {
            continue;
        }
        let known = *hashed.entry((path.clone(), inode)).or_insert_with(|| hash_file(&path, inode).is_ok_and(|h| list.holds(&h)));
        if !known {
            return Err(format!("it maps {}, which is neither the system's nor signed", path.display()));
        }
    }
    Ok(())
}

/// `LD_PRELOAD` or `LD_AUDIT` in a `/proc/<pid>/environ` (NUL-separated).
pub fn preloads(environ: &[u8]) -> Option<&'static str> {
    environ.split(|b| *b == 0).find_map(|var| {
        ["LD_PRELOAD", "LD_AUDIT"].into_iter().find(|name| var.strip_prefix(name.as_bytes()).is_some_and(|rest| rest.first() == Some(&b'=') && rest.len() > 1))
    })
}

/// One file-backed executable mapping of `/proc/<pid>/maps`.
#[derive(Debug, PartialEq, Eq)]
pub struct Mapping {
    pub inode: u64,
    pub path: PathBuf,
}

/// The executable mappings of a `maps` text that a file backs (the vDSO
/// and anonymous code have inode 0 and are skipped; a `memfd:` one is
/// kept, and fails the check as it is nobody's file).
pub fn executable_mappings(maps: &str) -> Vec<Mapping> {
    maps.lines()
        .filter_map(|line| {
            // `address perms offset dev inode path` (the path may hold spaces).
            let mut fields = line.splitn(6, ' ');
            let (_, perms, _, _, inode) = (fields.next()?, fields.next()?, fields.next()?, fields.next()?, fields.next()?);
            let path = fields.next().unwrap_or("").trim_start();
            let inode: u64 = inode.parse().ok()?;
            (perms.as_bytes().get(2) == Some(&b'x') && inode != 0).then(|| Mapping { inode, path: PathBuf::from(path) })
        })
        .collect()
}

/// `dir` and every directory above it belong to root, and only root can
/// write them.
fn root_owned_dir(dir: &Path) -> bool {
    let mut at = Some(dir);
    while let Some(d) = at {
        match std::fs::metadata(d) {
            Ok(m) if m.is_dir() && m.uid() == 0 && m.mode() & 0o022 == 0 => at = d.parent(),
            _ => return false,
        }
    }
    true
}

/// The SHA-256 of the file at `path`, if it is still inode `inode`.
fn hash_file(path: &Path, inode: u64) -> Result<[u8; 32], String> {
    use sha2::Digest;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.ino() != inode {
        return Err("another file is there now".into());
    }
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_file_mappings_only() {
        let maps = "55d0a000-55d0b000 r--p 00000000 fd:01 1234 /usr/bin/x\n\
                    55d0b000-55d0c000 r-xp 00001000 fd:01 1234 /usr/bin/x\n\
                    7f00-7f01 r-xp 00000000 00:5d 77 /tmp/.mount_Ab cd/usr/lib/libz.so.1\n\
                    7f02-7f03 r-xp 00000000 00:00 0 \n\
                    7f04-7f05 r-xp 00000000 00:01 99 /memfd:jit (deleted)\n\
                    7ffd-7ffe r-xp 00000000 00:00 0                          [vdso]\n";
        assert_eq!(
            executable_mappings(maps),
            vec![
                Mapping { inode: 1234, path: "/usr/bin/x".into() },
                Mapping { inode: 77, path: "/tmp/.mount_Ab cd/usr/lib/libz.so.1".into() },
                Mapping { inode: 99, path: "/memfd:jit (deleted)".into() },
            ]
        );
    }

    #[test]
    fn preloads_are_told_by_their_names() {
        assert_eq!(preloads(b"HOME=/h\0LD_PRELOAD=/x.so\0"), Some("LD_PRELOAD"));
        assert_eq!(preloads(b"LD_AUDIT=/a.so\0"), Some("LD_AUDIT"));
        assert_eq!(preloads(b"LD_PRELOAD=\0LD_PRELOADX=1\0LD_LIBRARY_PATH=/l\0"), None, "empty, another name, the library path (the mappings decide)");
    }
}
