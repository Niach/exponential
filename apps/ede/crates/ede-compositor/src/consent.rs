//! #EDE-172 (P5', design v3 3.3' and 3.4'): who may present the root token.
//!
//! eDE keeps writing the root token file: the released Exponential app and
//! CLI read it and say `hello` with it, and they must keep working with no
//! change. With the gate on (`[control] consent`, eDE's flip turns it on),
//! the token is honoured by who presents it, read from the connection's
//! peer ([`crate::peer`]):
//!
//! - eDE's own root-owned binaries (the shell, sealed, the portal), the
//!   compositor itself and the tests' stand-ins: root, no prompt.
//! - What the root-owned policy file names: its principal, no prompt.
//! - The root-owned `exponential-ctl`: the sealed consent for every ctl
//!   process (D3 a); on Allow, root for that process.
//! - An executable named `exp-desktop` or `exponential` (the app's, inside
//!   its AppImage mount, and the CLI's; both the person's own files): the
//!   sealed consent at its first `hello` per login and content hash (the
//!   path is shown, never the key: an AppImage mounts at a new path every
//!   start). On Allow, the `computer-use` principal ([`Principal`]).
//! - Anything else: refused, no prompt.
//!
//! The asking `hello` gets its reply when the person answers; later ones of
//! the same binary wait for the same answer, one prompt is open at a time.
//! A no refuses the process that asked for good and the binary for
//! [`BACKOFF`]. Nothing is written to disk: a new login asks again.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ede_protocol::Capability;
use serde::Deserialize;

use crate::peer::{Installed, Peer};

/// The policy file: root-owned, writable by root only, or it is ignored.
pub const POLICY_PATH: &str = "/etc/exponential-desktop/control.toml";

/// After a "Don't allow", the same binary is not asked again sooner.
pub const BACKOFF: Duration = Duration::from_secs(60);

/// The executables that may ask (the app's inside its AppImage, the CLI's).
pub const ASKING_NAMES: [&str; 2] = ["exp-desktop", "exponential"];

/// What a connection that said `hello` with the root token acts as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Principal {
    /// Everything (the shell, a test, eDE's helpers; every root connection
    /// while the gate is off).
    #[default]
    Root,
    /// The person allowed the app or the CLI (design v3 3.4'): it reads as
    /// root (today's app mints because its hello says `system`), and the
    /// compositor holds it to what the app does: input, capture, windows,
    /// grants no wider than `{capture, input:person, ui}` or `[space]`, and
    /// the spaces and grants of its own lineage. Never lock, logout,
    /// unlock, outputs, the config, launch tokens, bindings or events.
    ComputerUse,
}

impl Principal {
    /// The capabilities a `computer-use` grant may mint one of: either a
    /// subset of the person's run capabilities, or `[space]` alone.
    pub fn may_mint(self, capabilities: &[Capability]) -> bool {
        match self {
            Principal::Root => true,
            Principal::ComputerUse => {
                capabilities == [Capability::Space] || capabilities.iter().all(|c| matches!(c, Capability::Capture | Capability::InputPerson | Capability::Ui))
            }
        }
    }
}

/// What the compositor does with a root `hello` from a peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Grant(Principal),
    Refuse(String),
    /// The person is asked (or the answer is waited for).
    Ask(Asking),
}

/// One question to the person, and what it is keyed by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asking {
    pub key: Key,
    pub title: String,
    pub body: String,
    /// What an Allow gives.
    pub grants: Principal,
}

/// What an answer is remembered by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// The app or the CLI: by the executable's content (SHA-256), per login.
    Binary([u8; 32]),
    /// `exponential-ctl`: by the process (pid and start), for its life.
    Process(u32, u64),
}

/// The policy file's entries ([`POLICY_PATH`]).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub allow: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The executable as `/proc/<pid>/exe` names it.
    pub path: PathBuf,
    /// Its SHA-256 (hex); none = whatever is at `path`.
    #[serde(default)]
    pub sha256: Option<String>,
    /// `root` or `computer-use`.
    pub principal: String,
}

impl Policy {
    /// The file at `path`, when root owns it and the directories above it
    /// and nobody else can write any of them; else none (and why, for the
    /// log).
    pub fn load(path: &Path) -> Result<Option<Policy>, String> {
        if !path.exists() {
            return Ok(None);
        }
        if crate::peer::root_owned(path).is_none() {
            return Err(format!("{} is ignored: it must belong to root and be writable by root only", path.display()));
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Policy::parse(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Policy, String> {
        let policy: Policy = toml::from_str(text).map_err(|e| e.to_string())?;
        for entry in &policy.allow {
            if !matches!(entry.principal.as_str(), "root" | "computer-use") {
                return Err(format!("{}: principal must be root or computer-use", entry.path.display()));
            }
            if entry.sha256.as_ref().is_some_and(|h| hex32(h).is_none()) {
                return Err(format!("{}: sha256 must be 64 hex", entry.path.display()));
            }
        }
        Ok(policy)
    }

    /// The principal for `peer` (whose executable hashes to `hash`, when
    /// known), if an entry names it. An entry with a hash needs the hash.
    pub fn principal_for(&self, peer: &Peer, hash: Option<&[u8; 32]>) -> Option<Principal> {
        let exe = peer.exe_path().filter(|_| peer.same_userns)?;
        self.allow.iter().find(|e| e.path == exe && e.sha256.as_deref().is_none_or(|h| hex32(h).as_ref() == hash)).map(|e| match e.principal.as_str() {
            "root" => Principal::Root,
            _ => Principal::ComputerUse,
        })
    }

    /// Does an entry for `peer`'s path need its hash to be decided?
    pub fn wants_hash(&self, peer: &Peer) -> bool {
        let Some(exe) = peer.exe_path() else { return false };
        self.allow.iter().any(|e| e.path == exe && e.sha256.is_some())
    }
}

fn hex32(text: &str) -> Option<[u8; 32]> {
    let text = text.trim();
    if text.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// The SHA-256 of what process `pid` runs, read through `/proc/<pid>/exe`
/// and only if that is still the file `(dev, ino)` it connected with. Off
/// the event loop: an AppImage's binary is read through its FUSE mount.
pub fn hash_exe(pid: u32, dev: u64, ino: u64) -> Result<[u8; 32], String> {
    use sha2::Digest;
    let mut file = std::fs::File::open(format!("/proc/{pid}/exe")).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if (meta.dev(), meta.ino()) != (dev, ino) {
        return Err("the process runs another executable than it connected with".into());
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

/// What a file is for the hash cache.
pub fn file_key(peer: &Peer) -> Option<(u64, u64, u64, i64)> {
    let meta = std::fs::metadata(format!("/proc/{}/exe", peer.pid)).ok()?;
    let exe = peer.exe.as_ref()?;
    ((meta.dev(), meta.ino()) == (exe.dev, exe.ino)).then(|| (meta.dev(), meta.ino(), meta.size(), meta.mtime()))
}

/// A `hello` waiting for the person (or for its executable's hash).
#[derive(Debug)]
pub struct Waiter {
    pub conn: u64,
    pub call: u64,
    pub pid: u32,
    pub start: u64,
    pub asking: Asking,
}

/// What the tests answer for the person: each question goes to `asks`;
/// `answer` there is what the person said.
pub struct StandIn {
    pub asking: Asking,
    pub answer: smithay::reexports::calloop::channel::Sender<bool>,
}

/// The gate's state, for the compositor's lifetime (one login).
#[derive(Default)]
pub struct Gate {
    /// `[control] consent`, or a test's switch.
    pub on: bool,
    /// Where the policy file is ([`POLICY_PATH`]; a test's own).
    pub policy_path: Option<PathBuf>,
    /// Answers given this login.
    allowed: HashSet<Key>,
    /// Processes the person refused: refused for their life.
    refused: HashSet<(u32, u64)>,
    /// A binary refused lately is not asked again before this.
    backoff: HashMap<[u8; 32], Instant>,
    /// Executables hashed already, by `(dev, ino, size, mtime)`.
    hashes: HashMap<(u64, u64, u64, i64), [u8; 32]>,
    /// The hellos waiting for an answer; the first one's question is open.
    pub waiting: VecDeque<Waiter>,
    /// The open question's key and, when the compositor spawned the
    /// prompt, the prompt's pid (its surface is sealed).
    pub open: Option<(Key, Option<u32>)>,
    /// Tests: questions go here instead of to a prompt.
    pub stand_in: Option<std::sync::mpsc::Sender<StandIn>>,
    /// `--consent-answer` (a headless or nested test compositor): the
    /// person's answer to every question, without a prompt.
    pub answer_with: Option<bool>,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gate").field("on", &self.on).field("allowed", &self.allowed.len()).field("waiting", &self.waiting.len()).field("open", &self.open).finish()
    }
}

impl Gate {
    /// The policy in force now (read at every root `hello`: an entry added
    /// with sudo counts at once); a file that does not qualify is logged
    /// and ignored.
    pub fn policy(&self) -> Policy {
        let path = self.policy_path.clone().unwrap_or_else(|| PathBuf::from(POLICY_PATH));
        match Policy::load(&path) {
            Ok(policy) => policy.unwrap_or_default(),
            Err(why) => {
                tracing::warn!("{why}");
                Policy::default()
            }
        }
    }

    pub fn cached_hash(&self, peer: &Peer) -> Option<[u8; 32]> {
        file_key(peer).and_then(|key| self.hashes.get(&key).copied())
    }

    pub fn remember_hash(&mut self, key: (u64, u64, u64, i64), hash: [u8; 32]) {
        self.hashes.insert(key, hash);
    }

    /// The verdict for a root `hello` from `peer`, given what is known now.
    /// `hash` = the executable's, when it was needed and is known; `None`
    /// with [`Gate::needs_hash`] true = hash it first.
    pub fn verdict(&self, peer: Option<&Peer>, installed: &Installed, own: &[u32], policy: &Policy, hash: Option<&[u8; 32]>) -> Verdict {
        let Some(peer) = peer else { return Verdict::Refuse("the root token is honoured only from a process the compositor knows".into()) };
        if own.contains(&peer.pid) || [crate::peer::SHELL, crate::peer::SEALED, crate::peer::PORTAL].iter().any(|name| installed.is(peer, name)) {
            return Verdict::Grant(Principal::Root);
        }
        if let Some(principal) = policy.principal_for(peer, hash) {
            return Verdict::Grant(principal);
        }
        if self.refused.contains(&(peer.pid, peer.start)) {
            return Verdict::Refuse("the person did not allow this process".into());
        }
        let path = peer.exe_path().unwrap_or_default();
        let parent = peer.parent.as_ref().and_then(|(_, p)| p.clone()).map(|p| p.display().to_string()).unwrap_or_else(|| "an unknown program".into());
        if installed.is(peer, crate::peer::CTL) {
            let key = Key::Process(peer.pid, peer.start);
            if self.allowed.contains(&key) {
                return Verdict::Grant(Principal::Root);
            }
            return Verdict::Ask(Asking {
                key,
                title: "Allow exponential-ctl to control your desktop?".into(),
                body: format!("exponential-ctl (process {}, started by {parent}) wants to see your screen and control your windows and your input.", peer.pid),
                grants: Principal::Root,
            });
        }
        let named = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| ASKING_NAMES.contains(&n));
        if !named || !peer.same_userns {
            return Verdict::Refuse(format!("the root token is not honoured from {}", path.display()));
        }
        let Some(hash) = hash else { return Verdict::Refuse("its executable could not be read".into()) };
        let key = Key::Binary(*hash);
        if self.allowed.contains(&key) {
            return Verdict::Grant(Principal::ComputerUse);
        }
        if self.backoff.get(hash).is_some_and(|until| Instant::now() < *until) {
            return Verdict::Refuse("the person did not allow this program a moment ago".into());
        }
        Verdict::Ask(Asking {
            key,
            title: "Allow Exponential to use your screen?".into(),
            body: format!("{} (process {}, started by {parent}) wants to see your screen and control it when an agent works on your screen.", path.display(), peer.pid),
            grants: Principal::ComputerUse,
        })
    }

    /// Whether the verdict for `peer` needs its executable's hash first.
    pub fn needs_hash(&self, peer: &Peer, installed: &Installed, policy: &Policy) -> bool {
        if [crate::peer::SHELL, crate::peer::SEALED, crate::peer::PORTAL, crate::peer::CTL].iter().any(|name| installed.is(peer, name)) {
            return false;
        }
        let named = peer.exe_path().and_then(|p| p.file_name().and_then(|n| n.to_str()).map(|n| ASKING_NAMES.contains(&n))).unwrap_or(false);
        (named && peer.same_userns) || policy.wants_hash(peer)
    }

    /// The person answered `key`: remembered (yes) or the asker refused
    /// and the binary backed off (no).
    pub fn answered(&mut self, key: &Key, allowed: bool, askers: &[(u32, u64)]) {
        if allowed {
            self.allowed.insert(key.clone());
            return;
        }
        self.refused.extend(askers.iter().copied());
        if let Key::Binary(hash) = key {
            self.backoff.insert(*hash, Instant::now() + BACKOFF);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_parses_and_refuses_what_it_does_not_know() {
        assert!(Policy::parse("[[allow]]\npath = \"/x\"\n").is_err(), "the principal is said");
        let ok = Policy::parse(&format!("[[allow]]\npath = \"/usr/bin/exponential-ctl\"\nprincipal = \"root\"\n[[allow]]\npath = \"/home/u/.local/bin/exponential\"\nsha256 = \"{}\"\nprincipal = \"computer-use\"\n", "ab".repeat(32))).unwrap();
        assert_eq!(ok.allow.len(), 2);
        assert_eq!(ok.allow[1].sha256.as_deref().and_then(hex32), Some([0xab; 32]));
        assert!(Policy::parse("[[allow]]\npath = \"/x\"\nprincipal = \"god\"\n").is_err());
        assert!(Policy::parse("[[allow]]\npath = \"/x\"\nprincipal = \"root\"\nsha256 = \"12\"\n").is_err());
        assert!(Policy::parse("[[allow]]\npath = \"/x\"\nprincipal = \"root\"\nmode = 1\n").is_err(), "no unknown keys");
        assert_eq!(Policy::parse("").unwrap(), Policy::default());
    }

    #[test]
    fn computer_use_mints_what_the_app_mints_and_nothing_wider() {
        let cu = Principal::ComputerUse;
        assert!(cu.may_mint(&[Capability::Capture, Capability::InputPerson, Capability::Ui]));
        assert!(cu.may_mint(&[Capability::Capture]));
        assert!(cu.may_mint(&[Capability::Space]));
        assert!(!cu.may_mint(&[Capability::System]));
        assert!(!cu.may_mint(&[Capability::Capture, Capability::Space]));
        assert!(!cu.may_mint(&[Capability::Lend]));
        assert!(Principal::Root.may_mint(&[Capability::System]));
    }
}
