//! §7.7 deviceId — the stable per-install device identity, generated once
//! and never regenerated. (Formerly part of the actions trust store, which
//! EXP-268 removed — actions run without a per-device trust prompt now.)
//!
//! EXP-1102: the identity lives in a file of ITS OWN, `{data_dir}/device-id`
//! (the CLI daemon's in `cli-device-id`): one line, the uuid, written once
//! and never rewritten by anything — no preference save, no engine state
//! write, no merge of any kind touches it again. Until then it was one key
//! of the shared settings.json, and one torn write of that file by another
//! of its eight writers read as a fresh install: the id was re-minted over
//! every other key and the machine's running workflow lost its host
//! (workflow 2f353e88, 2026-09-25). A missing identity file beside a
//! settings.json that still carries the key MIGRATES that id into the file;
//! it never mints a second one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The ids this process resolved, by identity file — ONE answer per file
/// for the process lifetime. Only a read or a mint that persisted lands
/// here: a temporary id served over an unreadable file is never cached,
/// so the real id comes back the moment the file parses again.
static RESOLVED: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());

/// The stable per-install device UUID: `{data_dir}/device-id`, migrated from
/// the `deviceId` key of `{data_dir}/settings.json` when the file is not
/// there yet. Created on first call; identical forever after. If persisting
/// fails the generated id is still returned — callers keep working, the id
/// just won't be stable until the disk is writable again.
pub fn device_id(data_dir: &Path) -> String {
    stable_id(data_dir, "device-id", "deviceId")
}

/// The CLI daemon's OWN device identity (EXP-403): `{data_dir}/cli-device-id`
/// (formerly `cliDeviceId` in settings.json). Deliberately DISTINCT from
/// [`device_id`] — the relay keys presence by (user, deviceId) and evicts a
/// same-id reconnect with CLOSE_REPLACED, so a desktop app and a CLI daemon
/// sharing one machine (and one data dir) would evict each other in a loop
/// if they shared an id. Two ids ⇒ two registry rows ⇒ two independently
/// startable machines, which is also what the UI should show.
pub fn cli_device_id(data_dir: &Path) -> String {
    stable_id(data_dir, "cli-device-id", "cliDeviceId")
}

/// FEED-154: the CLI daemon's machine key — an ed25519 keypair in
/// `{data_dir}/cli-device-key` (the PKCS#8 document as one hex line, 0600),
/// minted beside `cli-device-id` and, like it, never rewritten. The daemon
/// signs every `devices.register` with it ([`DeviceKey::proof`]); the
/// server pins the first proven key per machine id and lets only its holder
/// share the machine, so an account that merely read the id from a
/// team-visible session cannot share (or squat) the box. An unreadable or
/// unparsable file is NEVER minted over (a new key would orphan the pin):
/// the daemon then registers without a proof, and the row cannot share.
pub struct DeviceKey {
    pair: ring::signature::Ed25519KeyPair,
}

/// What a register carries to prove the key: the public key, the instant it
/// signed, and the signature (all lowercase hex but `signed_at`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyProof {
    pub public_key: String,
    pub signed_at: String,
    pub signature: String,
}

impl DeviceKey {
    /// The raw 32-byte public key, hex.
    pub fn public_key_hex(&self) -> String {
        use ring::signature::KeyPair as _;
        hex(self.pair.public_key().as_ref())
    }

    /// The register proof: a signature of the server's
    /// `deviceKeyMessage` (`lib/device-keys.ts`), bound to the ACCOUNT so a
    /// proof never registers the key under another one. `signed_at` is an
    /// ISO instant (the server takes ±10 minutes).
    pub fn proof(&self, user_id: &str, device_id: &str, signed_at: &str) -> KeyProof {
        let public_key = self.public_key_hex();
        let message = key_message(user_id, device_id, &public_key, signed_at);
        KeyProof {
            signature: hex(self.pair.sign(message.as_bytes()).as_ref()),
            public_key,
            signed_at: signed_at.to_string(),
        }
    }
}

/// The signed bytes — byte-for-byte the server's `deviceKeyMessage`.
fn key_message(user_id: &str, device_id: &str, public_key: &str, signed_at: &str) -> String {
    format!("exponential-device-key:v1\n{user_id}\n{device_id}\n{public_key}\n{signed_at}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

/// The daemon's machine key (minted on first call, under the same
/// machine-wide lock as the ids). `None` = the file is there but unreadable,
/// or the disk refused the first write: register without a proof.
pub fn cli_device_key(data_dir: &Path) -> Option<DeviceKey> {
    let path = data_dir.join("cli-device-key");
    let _guard = crate::settings_lock::locked(data_dir);
    let parse = |raw: &str| {
        unhex(raw).and_then(|der| ring::signature::Ed25519KeyPair::from_pkcs8(&der).ok())
    };
    match fs::read_to_string(&path) {
        Ok(raw) => match parse(&raw) {
            Some(pair) => return Some(DeviceKey { pair }),
            None => {
                log::warn!("{} is unreadable; registering without a machine key", path.display());
                return None;
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            log::warn!("{} could not be read ({err}); registering without a machine key", path.display());
            return None;
        }
    }
    let rng = ring::rand::SystemRandom::new();
    let document = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).ok()?;
    let encoded = hex(document.as_ref());
    if let Err(err) = crate::atomic_file::write_atomic(&path, &format!("{encoded}\n")) {
        log::warn!("{} could not be written: {err}", path.display());
        return None;
    }
    parse(&encoded).map(|pair| DeviceKey { pair })
}

/// One read of a file, classified — EXP-766: "cannot read" and "not there"
/// are DIFFERENT answers, and only the second one may mint a new id.
enum Read<T> {
    /// Not there: a fresh install (or a pre-EXP-1102 one, for the identity
    /// file — the legacy key decides then).
    Fresh,
    Found(T),
    /// Present but unreadable (a torn read, a hand-edit): never permission to
    /// mint over it.
    Unreadable,
}

/// The identity file: one non-empty line.
fn read_identity_file(path: &Path) -> Read<String> {
    match fs::read_to_string(path) {
        Ok(raw) => match raw.trim() {
            "" => Read::Unreadable,
            id => Read::Found(id.to_string()),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Read::Fresh,
        Err(_) => Read::Unreadable,
    }
}

/// The legacy settings.json `key`, if the file is there and parses.
fn read_legacy_key(path: &Path, key: &str) -> Read<Option<String>> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Read::Fresh,
        Err(_) => return Read::Unreadable,
    };
    // An EMPTY file is a writer caught mid-flight, not a fresh install.
    if raw.trim().is_empty() {
        return Read::Unreadable;
    }
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) if value.is_object() => Read::Found(
            value
                .get(key)
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
        ),
        _ => Read::Unreadable,
    }
}

/// What one resolution of an identity file came back with.
enum Resolved {
    /// Read from the file, or minted AND written: the id for good.
    Stable(String),
    /// Served over a file that could not be read or written: this call's
    /// only, never remembered.
    Temporary(String),
}

fn resolved_lock() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, String>> {
    match RESOLVED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The id of `file`, answered from [`RESOLVED`] once it is known: every
/// caller in the process (the daemon's per-launch `Ctx::device_id`, the
/// desktop's direct reads) sees the ONE id, and a file that turns unreadable
/// later never hands them a fresh unpersisted uuid.
fn stable_id(data_dir: &Path, file: &str, legacy_key: &str) -> String {
    let path = data_dir.join(file);
    if let Some(id) = resolved_lock().get(&path) {
        return id.clone();
    }
    match resolve_id(data_dir, &path, legacy_key) {
        Resolved::Stable(id) => {
            resolved_lock().insert(path, id.clone());
            id
        }
        Resolved::Temporary(id) => id,
    }
}

fn resolve_id(data_dir: &Path, path: &Path, legacy_key: &str) -> Resolved {
    let legacy = data_dir.join("settings.json");
    // EXP-781: read, mint and write under ONE machine-wide section. The
    // desktop app and the CLI daemon share a data dir (REV-20), so on a first
    // run they could both read no id, both mint, and both write — one machine
    // minting two identities, which the relay keys presence by. Held for the
    // whole function: dropped before the read completes, the lock would
    // guard nothing.
    let _guard = crate::settings_lock::locked(data_dir);
    match read_identity_file(path) {
        Read::Found(id) => return Resolved::Stable(id),
        Read::Fresh => {}
        Read::Unreadable => {
            // A torn read is over in microseconds: retry once, then serve an
            // unpersisted id for this run rather than write over a file we
            // could not read.
            std::thread::sleep(std::time::Duration::from_millis(50));
            match read_identity_file(path) {
                Read::Found(id) => return Resolved::Stable(id),
                Read::Fresh => {}
                Read::Unreadable => {
                    log::warn!("{} is unreadable; using a temporary id for this run", path.display());
                    return Resolved::Temporary(uuid::Uuid::new_v4().to_string());
                }
            }
        }
    }
    // No identity file yet. A settings.json that still carries the legacy
    // key names THIS machine: migrate it, never mint beside it.
    let migrated = match read_legacy_key(&legacy, legacy_key) {
        Read::Found(existing) => existing,
        Read::Fresh => None,
        Read::Unreadable => {
            std::thread::sleep(std::time::Duration::from_millis(50));
            match read_legacy_key(&legacy, legacy_key) {
                Read::Found(existing) => existing,
                Read::Fresh => None,
                Read::Unreadable => {
                    // The id may well be in there: minting now would split the
                    // device in two the moment the file parses again.
                    log::warn!(
                        "{} is unreadable; using a temporary {legacy_key} for this run",
                        legacy.display()
                    );
                    return Resolved::Temporary(uuid::Uuid::new_v4().to_string());
                }
            }
        }
    };
    let id = migrated.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    // Written ONCE, by rename, and never again by anyone. Unwritten, the id
    // is this call's only: the next call mints and writes again.
    match crate::atomic_file::write_atomic(path, &format!("{id}\n")) {
        Ok(()) => Resolved::Stable(id),
        Err(err) => {
            log::warn!("{} could not be written: {err}", path.display());
            Resolved::Temporary(id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-device-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn cli_device_key_is_minted_once_and_signs_the_server_message() {
        let dir = TempDir::new("device-key");
        let first = cli_device_key(&dir.0).expect("minted");
        let again = cli_device_key(&dir.0).expect("read back");
        assert_eq!(first.public_key_hex(), again.public_key_hex());
        assert_eq!(first.public_key_hex().len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.0.join("cli-device-key")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let proof = first.proof("user-1", "dev-1", "2026-10-28T10:00:00.000Z");
        assert_eq!(proof.signature.len(), 128);
        let public = unhex(&proof.public_key).unwrap();
        let message = key_message("user-1", "dev-1", &proof.public_key, &proof.signed_at);
        let key = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public);
        assert!(key.verify(message.as_bytes(), &unhex(&proof.signature).unwrap()).is_ok());
        // Bound to the account: another user id does not verify.
        let other = key_message("user-2", "dev-1", &proof.public_key, &proof.signed_at);
        assert!(key.verify(other.as_bytes(), &unhex(&proof.signature).unwrap()).is_err());
    }

    #[test]
    fn an_unreadable_cli_device_key_is_never_minted_over() {
        let dir = TempDir::new("device-key-torn");
        fs::write(dir.0.join("cli-device-key"), "not hex\n").unwrap();
        assert!(cli_device_key(&dir.0).is_none());
        assert_eq!(fs::read_to_string(dir.0.join("cli-device-key")).unwrap(), "not hex\n");
    }

    #[test]
    fn device_id_is_stable_and_uuid_shaped() {
        let dir = TempDir::new("device-id");
        let first = device_id(&dir.0);
        let second = device_id(&dir.0);
        assert_eq!(first, second, "generate once, never regenerate (§7.7)");
        assert_eq!(first.len(), 36);
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        // EXP-1102: it lives in its own file, one line, and settings.json is
        // not created for it.
        assert_eq!(
            fs::read_to_string(dir.0.join("device-id")).unwrap(),
            format!("{first}\n")
        );
        assert!(!dir.0.join("settings.json").exists());
    }

    /// EXP-1102: an install that predates the identity file carries its id
    /// in settings.json. It is MIGRATED — the same id, now in the file — and
    /// settings.json is left exactly as it was (older builds still read it).
    #[test]
    fn a_legacy_settings_id_migrates_into_the_identity_file() {
        let dir = TempDir::new("device-migrate");
        let legacy = r#"{"deviceId":"pre-existing-id","claudePath":"/opt/claude"}"#;
        fs::write(dir.0.join("settings.json"), legacy).unwrap();
        assert_eq!(device_id(&dir.0), "pre-existing-id");
        assert_eq!(
            fs::read_to_string(dir.0.join("device-id")).unwrap(),
            "pre-existing-id\n"
        );
        assert_eq!(fs::read_to_string(dir.0.join("settings.json")).unwrap(), legacy);
        // The file wins from here on, whatever settings.json says later.
        fs::write(dir.0.join("settings.json"), r#"{"deviceId":"other"}"#).unwrap();
        assert_eq!(device_id(&dir.0), "pre-existing-id");
    }

    /// EXP-766/EXP-1102: an unreadable settings.json with no identity file
    /// yet must never MINT — the id may be in there. The run gets a
    /// throwaway id, nothing is written, and the id comes back once the file
    /// parses again.
    #[test]
    fn an_unparseable_legacy_file_neither_mints_nor_is_clobbered() {
        let dir = TempDir::new("device-torn");
        let path = dir.0.join("settings.json");
        let torn = r#"{"deviceId":"kept-id","claudePa"#;
        fs::write(&path, torn).unwrap();

        let temporary = device_id(&dir.0);
        assert!(uuid::Uuid::parse_str(&temporary).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), torn, "untouched");
        assert!(!dir.0.join("device-id").exists(), "nothing minted over it");

        fs::write(&path, r#"{"deviceId":"kept-id","claudePath":"/opt/claude"}"#).unwrap();
        assert_eq!(device_id(&dir.0), "kept-id");
    }

    #[test]
    fn an_empty_legacy_file_is_never_overwritten() {
        let dir = TempDir::new("device-empty");
        let path = dir.0.join("settings.json");
        fs::write(&path, "").unwrap();
        let _ = device_id(&dir.0);
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert!(!dir.0.join("device-id").exists());
    }

    /// The identity file itself, torn (empty): a temporary id, and the file
    /// is left for its writer — never re-minted over.
    #[test]
    fn an_empty_identity_file_is_never_overwritten() {
        let dir = TempDir::new("device-empty-identity");
        fs::write(dir.0.join("device-id"), "").unwrap();
        let temporary = device_id(&dir.0);
        assert!(uuid::Uuid::parse_str(&temporary).is_ok());
        assert_eq!(fs::read_to_string(dir.0.join("device-id")).unwrap(), "");
    }

    /// Preference saves never touch the identity file: it is the id's ONLY
    /// home, and nothing else in the data dir is allowed to write it.
    #[test]
    fn the_identity_file_is_not_a_settings_key() {
        let dir = TempDir::new("device-own-file");
        let id = device_id(&dir.0);
        // Whatever settings.json becomes, the id stays what the file says.
        fs::write(dir.0.join("settings.json"), "{}").unwrap();
        assert_eq!(device_id(&dir.0), id);
        fs::write(dir.0.join("settings.json"), "").unwrap();
        assert_eq!(device_id(&dir.0), id);
    }

    /// Once resolved, the id is answered from memory for the process: a
    /// file torn or gone AFTER the first read never hands a caller a fresh
    /// temporary uuid (the daemon reads its id per launch).
    #[test]
    fn a_resolved_id_is_the_same_for_the_whole_process() {
        let dir = TempDir::new("device-cached");
        let id = device_id(&dir.0);
        assert_eq!(device_id(&dir.0), id);
        fs::write(dir.0.join("device-id"), "").unwrap();
        assert_eq!(device_id(&dir.0), id, "a torn file after the fact changes nothing");
        fs::remove_file(dir.0.join("device-id")).unwrap();
        assert_eq!(device_id(&dir.0), id, "nor a missing one");
        // A temporary id was never remembered: once the file is readable
        // the real id is served (see the torn-legacy test above).
        let torn = TempDir::new("device-cached-torn");
        fs::write(torn.0.join("device-id"), "").unwrap();
        let temporary = device_id(&torn.0);
        assert_ne!(device_id(&torn.0), temporary, "a temporary id is per call");
        fs::write(torn.0.join("device-id"), "real-id\n").unwrap();
        assert_eq!(device_id(&torn.0), "real-id");
        assert_eq!(device_id(&torn.0), "real-id");
    }

    #[test]
    fn cli_device_id_is_distinct_from_the_desktop_id() {
        // One machine, one data dir, TWO relay identities — a shared id
        // would make the desktop app and the daemon evict each other's
        // control socket in a CLOSE_REPLACED loop (EXP-403 review finding).
        let dir = TempDir::new("device-cli");
        let desktop = device_id(&dir.0);
        let cli = cli_device_id(&dir.0);
        assert_ne!(desktop, cli);
        assert_eq!(cli, cli_device_id(&dir.0), "stable across calls");
        assert_eq!(desktop, device_id(&dir.0), "desktop id untouched");
        // Each in its own file; the legacy key of each migrates separately.
        assert!(dir.0.join("cli-device-id").exists());
    }

    #[test]
    fn the_cli_id_migrates_from_its_own_legacy_key() {
        let dir = TempDir::new("device-cli-migrate");
        fs::write(
            dir.0.join("settings.json"),
            r#"{"deviceId":"desk","cliDeviceId":"cli"}"#,
        )
        .unwrap();
        assert_eq!(cli_device_id(&dir.0), "cli");
        assert_eq!(device_id(&dir.0), "desk");
    }
}
