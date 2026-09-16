//! EXP-852 — refreshing claude's OAuth credential the way the claude CLI
//! (2.1.272) does, so the daemon and the user's own CLI can share one login.
//!
//! A claude login is ONE credential document in ONE store, and both we and the
//! CLI read, rotate and write it. The refresh grant is single-use: whoever
//! POSTs it first gets a new pair and everyone else's copy is dead. So this
//! module does not invent a protocol — it reimplements the incumbent's, down
//! to the lock paths, the margin arithmetic and the write-back rules. Every
//! rule below was read out of claude 2.1.272 and is restated here because the
//! bundle is minified and unreadable at the next incident.
//!
//! ## The contract we mirror (claude 2.1.272)
//!
//! * **Config root** (`fS()`): `CLAUDE_SECURESTORAGE_CONFIG_DIR` if the
//!   variable is SET — even to the empty string, which falls to `~/.claude`
//!   without consulting anything else — else `CLAUDE_CONFIG_DIR`, else
//!   `~/.claude`. For an account PROFILE the caller passes
//!   `config_dir = Some(profile_dir)`, which IS the root (our launcher exports
//!   it as `CLAUDE_CONFIG_DIR`); `None` means the ambient login and resolves
//!   from this process's environment.
//! * **Locks** — all three are `proper-lockfile` mkdir directories, taken with
//!   [`crate::lockfile`]:
//!   1. `<root>/.oauth_refresh.lock` ([`LockOptions::oauth_refresh`]) around
//!      the whole read → POST → write.
//!   2. the LEGACY config lock, `format!("{}.lock", realpath(root))` (so
//!      `~/.claude.lock`, or `<profile-dir>.lock`), same options, taken right
//!      AFTER (1) and released BEFORE it. An `ELOCKED` there means an older
//!      CLI is mid-refresh: release (1) and report [`RefreshOutcome::Contended`].
//!      Any OTHER error on it is logged and ignored — it is the belt to (1)'s
//!      braces, not a reason to skip a refresh. SKIPPED on Windows: node's
//!      `realpath` and Rust's `canonicalize` disagree there (`\\?\` prefixes),
//!      so the path we would lock is not the path the CLI locks, and a lock
//!      nobody else takes is only a way to fail.
//!   3. `<root>/.storage-write.lock` ([`LockOptions::storage_write`]) around
//!      EVERY store write, including the canary below.
//! * **The refresh predicate** (`Y1`): `now_ms + margin_ms >= expiresAt`. The
//!   CLI's margin is 5 minutes; ours arrives from the caller (15 minutes — a
//!   daemon that polls usage every few minutes wants the token renewed well
//!   before a request can trip over it). An already-expired token satisfies
//!   the predicate and IS refreshed.
//! * **Read, lock, re-read.** The pre-lock read decides whether to bother at
//!   all; after the lock we read AGAIN, and if the stored `accessToken` moved
//!   the CLI (or another daemon) refreshed while we waited — we return its
//!   token as [`RefreshOutcome::NotNeeded`] and spend nothing.
//! * **The POST** is `https://platform.claude.com/v1/oauth/token`,
//!   `Content-Type: application/json` (no `anthropic-beta`), 30s, body
//!   `{grant_type, refresh_token, client_id, scope}` where `client_id` and
//!   `scope` come from the stored credential when it carries them.
//! * **Write-back is a compare-and-swap on the refresh token**: re-read the
//!   store under (3) and write only if it still holds a `claudeAiOauth`
//!   object whose `refreshToken` is absent/empty or still the one we POSTed.
//!   Otherwise a sibling already landed a newer pair and we ADOPT it
//!   ([`WriteOutcome::AdoptedSibling`]) rather than overwrite a live
//!   credential with our older one; and a store that VANISHED in between (a
//!   `claude logout` during the POST) stays gone ([`WriteOutcome::StoreGone`])
//!   rather than being recreated around a pair that would undo it. The rotated
//!   fields are laid OVER the previous `claudeAiOauth` object, so
//!   `subscriptionType`, `rateLimitTier`, `clientId` and every other key we do
//!   not understand survive, as do all other top-level keys of the document.
//!
//! ## Where we deliberately differ
//!
//! * **We never migrate between stores and never delete.** The CLI will move a
//!   credential from the file into the keychain (and drop the file) when it
//!   can. We write back to WHICHEVER store answered the read and to nothing
//!   else: a background keep-alive that silently relocates someone's
//!   credential is a surprise nobody asked for, and a delete we got wrong is a
//!   sign-out.
//! * **The keychain canary.** Before spending the grant we write the UNCHANGED
//!   document back through the keychain (under lock (3)). A headless daemon
//!   under launchd can hit an ACL modal that nobody will ever answer; finding
//!   that out AFTER the POST would mean a fresh token we cannot store and an
//!   old one we already burned — the exact way a shared login dies. A canary
//!   failure is [`RefreshOutcome::Denied`] with nothing spent.
//! * **After a successful POST we always try to write**, even if the refresh
//!   lock went compromised in the meantime (we log it). The grant is already
//!   spent; a CAS-guarded write is strictly better than dropping the only copy
//!   of the new token.
//!
//! Nothing here ever logs, formats or `Debug`s a token: every error string
//! names a status and an error code, and every secret-bearing type has a
//! redacted `Debug`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::lockfile::{self, LockError, LockGuard, LockOptions};

/// claude's OAuth token endpoint (2.1.272).
pub const CLAUDE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";

/// The CLI's public client id — used when the stored credential names none.
pub const CLAUDE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// The CLI's scope list — used when the stored credential carries no `scopes`.
pub const CLAUDE_OAUTH_SCOPES: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

/// Lock (1): the whole refresh, `<root>/.oauth_refresh.lock`.
pub const REFRESH_LOCK_NAME: &str = ".oauth_refresh.lock";

/// Lock (3): every store write, `<root>/.storage-write.lock`.
pub const STORAGE_WRITE_LOCK_NAME: &str = ".storage-write.lock";

/// The file store's name under the config root.
pub const CREDENTIALS_FILE: &str = ".credentials.json";

/// The CLI's own cutover: an `add-generic-password` line this long or shorter
/// goes down `security -i`'s stdin (keeping the secret out of argv, where `ps`
/// would publish it); anything longer goes as argv, because `security -i`
/// truncates its input line past roughly this point.
pub const KEYCHAIN_STDIN_LIMIT: usize = 4032;

/// The CLI's keychain write budget. Short on purpose: the thing that makes it
/// expire is an ACL modal nobody is there to answer.
#[cfg(target_os = "macos")]
const KEYCHAIN_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// The whole-request budget for the token POST.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);

/// CAS attempts against a store that keeps erroring (sleeping `100 * n` ms).
const WRITE_ATTEMPTS: u32 = 3;

/// The macOS keychain service claude names its credential after.
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// The account name used when the keychain item exists but its attributes do
/// not parse and `$USER` is unusable.
#[cfg(target_os = "macos")]
const FALLBACK_KEYCHAIN_ACCOUNT: &str = "claude-code-user";

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

/// WHERE a credential document came from — and therefore the only place it
/// may be written back to.
#[derive(Clone, PartialEq, Eq)]
pub enum CredentialSource {
    File(PathBuf),
    #[cfg(target_os = "macos")]
    Keychain { service: String, account: String },
}

impl fmt::Debug for CredentialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(f, "File({})", path.display()),
            #[cfg(target_os = "macos")]
            // A service name and a local account name: neither is a secret,
            // and without them a keychain bug is undiagnosable.
            Self::Keychain { service, account } => {
                write!(f, "Keychain({service}, {account})")
            }
        }
    }
}

/// A credential document, as read, with the store it came from.
///
/// The document is kept WHOLE (not parsed into fields) because we have to put
/// it back whole: everything we do not understand in it is something the CLI
/// does.
#[derive(Clone)]
pub struct ClaudeCredentialStore {
    pub source: CredentialSource,
    pub config_root: PathBuf,
    pub document: Value,
}

impl fmt::Debug for ClaudeCredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self
            .document
            .as_object()
            .map(|object| object.keys().map(String::as_str).collect())
            .unwrap_or_default();
        f.debug_struct("ClaudeCredentialStore")
            .field("source", &self.source)
            .field("config_root", &self.config_root.display().to_string())
            // KEY NAMES ONLY: every value in here is, or sits next to, a token.
            .field("document_keys", &keys)
            .finish()
    }
}

impl ClaudeCredentialStore {
    /// The `claudeAiOauth` object — the only branch of the document we touch.
    pub fn oauth(&self) -> Option<&Map<String, Value>> {
        self.document.get("claudeAiOauth")?.as_object()
    }

    pub fn access_token(&self) -> Option<&str> {
        self.oauth_str("accessToken")
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.oauth_str("refreshToken")
    }

    /// `expiresAt` — unix MILLIseconds, the field's own unit.
    pub fn expires_at_ms(&self) -> Option<i64> {
        let value = self.oauth()?.get("expiresAt")?;
        value.as_i64().or_else(|| value.as_f64().map(|at| at as i64))
    }

    pub fn subscription_type(&self) -> Option<&str> {
        self.oauth_str("subscriptionType")
    }

    /// The credential's own `clientId`, when it carries one.
    pub fn client_id(&self) -> Option<&str> {
        self.oauth_str("clientId")
    }

    /// `scopes` joined with spaces, as the token endpoint wants it.
    pub fn scopes_string(&self) -> Option<String> {
        let scopes: Vec<&str> = self
            .oauth()?
            .get("scopes")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (!scopes.is_empty()).then(|| scopes.join(" "))
    }

    /// The read-only credential the usage probe borrows
    /// ([`crate::agent_usage`] owns that shape; this is the one bridge).
    pub fn credential(&self) -> Option<crate::agent_usage::ClaudeOauthCredential> {
        crate::agent_usage::parse_claude_credentials(&self.document.to_string())
    }

    fn oauth_str(&self, field: &str) -> Option<&str> {
        self.oauth()?
            .get(field)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

/// What a store read produced. `Denied` is its own answer: a keychain ACL
/// refusal must back OFF, not retry in a minute.
#[derive(Debug)]
pub enum StoreRead {
    Found(ClaudeCredentialStore),
    /// No credential in any store we look at — signed out, or signed in some
    /// way we do not read.
    Missing,
    /// The store refused or timed out.
    Denied,
}

/// The document half of a read, before a source is attached.
enum DocRead {
    Found(Value),
    Missing,
    Denied,
}

/// The config root [`read_store`] and the locks work under.
///
/// `Some(dir)` IS the root (an account profile's `CLAUDE_CONFIG_DIR`);
/// `None` resolves the ambient login from this process's environment, per the
/// module doc's `fS()`.
pub fn claude_config_root(config_dir: Option<&Path>) -> Option<PathBuf> {
    resolve_root(config_dir).map(|root| root.path)
}

/// The resolved root plus whether the CLI names its keychain item by the
/// PLAIN service (`default_location`) or the `-<hash>` suffixed one.
struct ResolvedRoot {
    path: PathBuf,
    /// The CLI's `aP()`: `true` only when NO config-dir variable selected the
    /// root. Decided by the variable's PRESENCE, not by where it points: a
    /// `CLAUDE_CONFIG_DIR=$HOME/.claude` is the default directory under the
    /// SUFFIXED service, and we must look where the CLI wrote.
    default_location: bool,
}

fn resolve_root(config_dir: Option<&Path>) -> Option<ResolvedRoot> {
    resolve_root_from(
        config_dir,
        dirs::home_dir().map(|home| home.join(".claude")),
        std::env::var("CLAUDE_SECURESTORAGE_CONFIG_DIR").ok().as_deref(),
        std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref(),
    )
}

/// The pure half of [`resolve_root`]: `fS()` for the path, `aP()` for the
/// suffix. `default` = `~/.claude`; the two variables as this process sees
/// them (`None` = unset).
fn resolve_root_from(
    config_dir: Option<&Path>,
    default: Option<PathBuf>,
    securestorage_dir: Option<&str>,
    config_dir_var: Option<&str>,
) -> Option<ResolvedRoot> {
    // A profile dir is one our launcher exports as `CLAUDE_CONFIG_DIR`, so the
    // CLI running under it hashes it, wherever it is.
    if let Some(dir) = config_dir {
        return Some(ResolvedRoot {
            path: dir.to_path_buf(),
            default_location: false,
        });
    }
    // SET, even to "", wins over CLAUDE_CONFIG_DIR — an empty value means the
    // secure-storage root is the default one (plain service), not "keep
    // looking".
    if let Some(raw) = securestorage_dir {
        let trimmed = raw.trim();
        return if trimmed.is_empty() {
            default.map(|path| ResolvedRoot {
                path,
                default_location: true,
            })
        } else {
            Some(ResolvedRoot {
                path: PathBuf::from(trimmed),
                default_location: false,
            })
        };
    }
    match config_dir_var.map(str::trim) {
        Some(raw) if !raw.is_empty() => Some(ResolvedRoot {
            path: PathBuf::from(raw),
            default_location: false,
        }),
        _ => default.map(|path| ResolvedRoot {
            path,
            default_location: true,
        }),
    }
}

/// Read claude's credential document.
///
/// Ambient (`config_dir == None`): the macOS keychain first, then
/// `<root>/.credentials.json`. A PROFILE dir: the file first, then (macOS) the
/// keychain item named after that dir. Whichever answers becomes the
/// [`CredentialSource`] every later write goes to — we never migrate.
pub fn read_store(config_dir: Option<&Path>) -> StoreRead {
    let Some(root) = resolve_root(config_dir) else {
        return StoreRead::Missing;
    };
    let file = root.path.join(CREDENTIALS_FILE);
    let ambient = config_dir.is_none();

    // Ambient: the keychain first — that is where a default install keeps it,
    // and a leftover file beside it is the older copy.
    #[cfg(target_os = "macos")]
    if ambient {
        let service = keychain_service_for(&root);
        match read_keychain(&service) {
            (DocRead::Found(document), account) => {
                return found(
                    CredentialSource::Keychain { service, account },
                    root.path,
                    document,
                )
            }
            (DocRead::Denied, _) => return StoreRead::Denied,
            (DocRead::Missing, _) => {}
        }
    }

    match read_file(&file) {
        DocRead::Found(document) => {
            return found(CredentialSource::File(file), root.path, document)
        }
        DocRead::Denied => return StoreRead::Denied,
        DocRead::Missing => {}
    }

    // A PROFILE keeps its credential beside its config; the keychain item
    // named after that dir is the fallback.
    #[cfg(target_os = "macos")]
    if !ambient {
        let service = keychain_service_for(&root);
        match read_keychain(&service) {
            (DocRead::Found(document), account) => {
                return found(
                    CredentialSource::Keychain { service, account },
                    root.path,
                    document,
                )
            }
            (DocRead::Denied, _) => return StoreRead::Denied,
            (DocRead::Missing, _) => {}
        }
    }

    let _ = ambient;
    StoreRead::Missing
}

fn found(source: CredentialSource, config_root: PathBuf, document: Value) -> StoreRead {
    StoreRead::Found(ClaudeCredentialStore {
        source,
        config_root,
        document,
    })
}

/// Re-read exactly ONE source (the CAS's "what does the store say now?").
fn read_source(source: &CredentialSource) -> DocRead {
    match source {
        CredentialSource::File(path) => read_file(path),
        #[cfg(target_os = "macos")]
        CredentialSource::Keychain { service, account } => {
            read_keychain_item(service, Some(account))
        }
    }
}

/// `<root>/.credentials.json`: absent → `Missing`, unreadable → `Denied`,
/// unparseable → `Missing` (a corrupt file is not a credential, and we must
/// not treat it as one and CAS over it blind).
fn read_file(path: &Path) -> DocRead {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(document) if document.is_object() => DocRead::Found(document),
            _ => DocRead::Missing,
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DocRead::Missing,
        Err(_) => DocRead::Denied,
    }
}

// ---------------------------------------------------------------------------
// macOS keychain
// ---------------------------------------------------------------------------

/// The keychain service for a config root: the CLI's default service, plus
/// `-<first 8 hex of sha256(path)>` whenever a config-dir variable selected
/// the root (an account profile, or an ambient `CLAUDE_CONFIG_DIR` /
/// `CLAUDE_SECURESTORAGE_CONFIG_DIR` that is set and non-empty, EVEN when it
/// names the default directory).
#[cfg(target_os = "macos")]
pub fn keychain_service(config_dir: Option<&Path>) -> String {
    match resolve_root(config_dir) {
        Some(root) => keychain_service_for(&root),
        None => KEYCHAIN_SERVICE.to_string(),
    }
}

#[cfg(target_os = "macos")]
fn keychain_service_for(root: &ResolvedRoot) -> String {
    if root.default_location {
        return KEYCHAIN_SERVICE.to_string();
    }
    let digest = Sha256::digest(nfc_path(&root.path).as_bytes());
    format!("{KEYCHAIN_SERVICE}-{}", &format!("{digest:x}")[..8])
}

/// The path string the CLI hashes.
///
/// The CLI NFC-normalises it first. This is a PASS-THROUGH: adding
/// `unicode-normalization` for one hash is a dependency (and a licence entry)
/// we are not taking, and every byte of an ASCII path is already NFC. THE GAP:
/// a config root containing decomposed non-ASCII (an `é` typed on macOS, which
/// hands out NFD, or a path off an HFS+ volume) hashes differently here than
/// in the CLI, so we would look for a keychain item under a service name
/// nobody wrote and report the account signed out. If that ever surfaces, this
/// is the function to fix.
#[cfg(target_os = "macos")]
fn nfc_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `$USER` when it is a plain name, else a fixed placeholder. The account is
/// only ever a LAST resort — see [`read_keychain`], which recovers the real
/// one from the item's own attributes.
#[cfg(target_os = "macos")]
pub fn default_keychain_account() -> String {
    keychain_account_or_default(std::env::var("USER").ok().as_deref())
}

/// The pure half of [`default_keychain_account`]: anything outside
/// `[A-Za-z0-9._-]` (a quote, a space, a newline — the characters that would
/// change the meaning of the `security -i` line we build) is refused.
#[cfg(target_os = "macos")]
fn keychain_account_or_default(user: Option<&str>) -> String {
    match user.map(str::trim) {
        Some(name)
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) =>
        {
            name.to_string()
        }
        _ => FALLBACK_KEYCHAIN_ACCOUNT.to_string(),
    }
}

/// Read the keychain item for `service`, recovering the account it was
/// actually stored under. Returns the document and the account to write back
/// to (the recovered one, or [`default_keychain_account`] if the attributes
/// did not parse).
#[cfg(target_os = "macos")]
fn read_keychain(service: &str) -> (DocRead, String) {
    match keychain_account(service) {
        // Exit 44 on the attribute probe: no item under this service at all.
        // Never write to a service that has no item — that is how a second,
        // competing credential appears.
        KeychainAccount::Missing => (DocRead::Missing, default_keychain_account()),
        KeychainAccount::Denied => (DocRead::Denied, default_keychain_account()),
        KeychainAccount::Found(account) => {
            (read_keychain_item(service, Some(&account)), account)
        }
        // The item exists but its attributes did not parse: read it without
        // `-a` and keep a sane account for the write-back.
        KeychainAccount::Unnamed => (
            read_keychain_item(service, None),
            default_keychain_account(),
        ),
    }
}

#[cfg(target_os = "macos")]
enum KeychainAccount {
    Found(String),
    /// The item exists, its `acct` attribute did not parse.
    Unnamed,
    Missing,
    Denied,
}

/// `security find-generic-password -s <service>` — ATTRIBUTES ONLY (no `-w`),
/// which is what keeps it free of an ACL prompt. The account rides a line like
/// `    "acct"<blob>="niach"`.
#[cfg(target_os = "macos")]
fn keychain_account(service: &str) -> KeychainAccount {
    let mut cmd = terminal::process::background_command("/usr/bin/security");
    cmd.args(["find-generic-password", "-s", service]);
    match crate::doctor::output_with_timeout(cmd, crate::doctor::PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            match parse_keychain_account(&String::from_utf8_lossy(&output.stdout)) {
                Some(account) => KeychainAccount::Found(account),
                None => KeychainAccount::Unnamed,
            }
        }
        Ok(output) if output.status.code() == Some(44) => KeychainAccount::Missing,
        Ok(_) | Err(_) => KeychainAccount::Denied,
    }
}

/// Pull `acct` out of `security`'s attribute dump. Both renderings are
/// handled: the quoted one, and the `0x…` hex one it uses when the value is
/// not plain ASCII.
#[cfg(target_os = "macos")]
fn parse_keychain_account(attributes: &str) -> Option<String> {
    let line = attributes
        .lines()
        .find(|line| line.trim_start().starts_with("\"acct\""))?;
    let (_, value) = line.split_once('=')?;
    let value = value.trim();
    if let Some(open) = value.find('"') {
        let rest = &value[open + 1..];
        let close = rest.rfind('"')?;
        let account = &rest[..close];
        return (!account.is_empty()).then(|| account.to_string());
    }
    let hex = value.strip_prefix("0x")?;
    let hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect();
    let account = String::from_utf8(bytes).ok()?;
    (!account.is_empty()).then_some(account)
}

/// The read that actually touches the secret: `-w`. Exit 44 = no such item.
#[cfg(target_os = "macos")]
fn read_keychain_item(service: &str, account: Option<&str>) -> DocRead {
    let mut cmd = terminal::process::background_command("/usr/bin/security");
    cmd.arg("find-generic-password");
    if let Some(account) = account {
        cmd.args(["-a", account]);
    }
    cmd.args(["-s", service, "-w"]);
    match crate::doctor::output_with_timeout(cmd, crate::doctor::PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let raw = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<Value>(raw.trim()) {
                Ok(document) if document.is_object() => DocRead::Found(document),
                _ => DocRead::Missing,
            }
        }
        Ok(output) if output.status.code() == Some(44) => DocRead::Missing,
        // A refusal, a locked keychain, or the ACL modal timing out.
        Ok(_) | Err(_) => DocRead::Denied,
    }
}

/// How a keychain write is delivered — see [`KEYCHAIN_STDIN_LIMIT`].
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainWrite {
    /// The whole `security -i` line, newline included.
    Stdin(String),
    /// The same command as argv (`security <args…>`).
    Argv(Vec<String>),
}

/// Build the `add-generic-password` invocation. PURE — it runs nothing, so the
/// cutover is testable without a keychain.
///
/// `-U` updates the item in place when it already exists (never a second one),
/// `-X` takes the payload as hex so no quoting rule can corrupt it. Service
/// and account are wrapped in double quotes in the stdin form, exactly as the
/// CLI writes them; both are validated/recovered plain names, so there is
/// nothing further to escape.
#[cfg(target_os = "macos")]
pub fn keychain_write_command(service: &str, account: &str, json: &str) -> KeychainWrite {
    let hex = hex_encode(json.as_bytes());
    let line = format!("add-generic-password -U -a \"{account}\" -s \"{service}\" -X {hex}\n");
    if line.len() <= KEYCHAIN_STDIN_LIMIT {
        return KeychainWrite::Stdin(line);
    }
    KeychainWrite::Argv(vec![
        "add-generic-password".to_string(),
        "-U".to_string(),
        "-a".to_string(),
        account.to_string(),
        "-s".to_string(),
        service.to_string(),
        "-X".to_string(),
        hex,
    ])
}

#[cfg(target_os = "macos")]
fn write_keychain(service: &str, account: &str, json: &str) -> Result<(), StoreError> {
    let output = match keychain_write_command(service, account, json) {
        KeychainWrite::Stdin(line) => {
            let mut cmd = terminal::process::background_command("/usr/bin/security");
            cmd.arg("-i");
            crate::doctor::output_with_stdin_timeout(cmd, &line, KEYCHAIN_WRITE_TIMEOUT)
        }
        KeychainWrite::Argv(args) => {
            let mut cmd = terminal::process::background_command("/usr/bin/security");
            cmd.args(&args);
            crate::doctor::output_with_timeout(cmd, KEYCHAIN_WRITE_TIMEOUT)
        }
    };
    match output {
        // `security -i` reports a failed sub-command on stderr while still
        // exiting 0, so an empty stderr is part of "it worked".
        Ok(out) if out.status.success() && out.stderr.is_empty() => Ok(()),
        Ok(out) if out.status.success() => Err(StoreError::Failed(
            "security reported an error on stderr".to_string(),
        )),
        // Exit code ONLY: the invocation carries the credential as hex.
        Ok(out) => Err(StoreError::Failed(format!(
            "security exited {}",
            out.status.code().unwrap_or(-1)
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::TimedOut => Err(StoreError::Timeout),
        Err(err) => Err(StoreError::Io(err)),
    }
}

/// `-X` takes the payload as hex, which is what keeps JSON quoting out of the
/// `security` invocation entirely.
#[cfg(target_os = "macos")]
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

// ---------------------------------------------------------------------------
// The token POST
// ---------------------------------------------------------------------------

/// A rotated pair. `Debug` is redacted — this type must never be able to print
/// a token.
#[derive(Clone)]
pub struct RotatedToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
    pub refresh_token_expires_at_ms: Option<i64>,
    pub scopes: Option<Vec<String>>,
}

impl fmt::Debug for RotatedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RotatedToken")
            .field("access_token", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .field(
                "refresh_token_expires_at_ms",
                &self.refresh_token_expires_at_ms,
            )
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Why a token POST did not produce a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// The grant is dead — the account needs a re-login (or a sibling already
    /// spent this refresh token). NEVER retry it.
    InvalidGrant,
    /// Everything else: a 5xx, a transport failure, an unparseable 200. The
    /// string names a status and an error CODE, never a response body.
    Transient(String),
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGrant => write!(f, "the sign-in is no longer valid (invalid_grant)"),
            Self::Transient(detail) => write!(f, "{detail}"),
        }
    }
}

/// POST the refresh grant. `client_id`/`scopes` are the stored credential's
/// when it carries them, else the CLI's constants.
pub fn post_refresh(
    endpoint: &str,
    refresh_token: &str,
    client_id: Option<&str>,
    scopes: Option<&str>,
    now_ms: i64,
) -> Result<RotatedToken, TokenError> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": client_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(CLAUDE_OAUTH_CLIENT_ID),
        "scope": scopes
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .unwrap_or(CLAUDE_OAUTH_SCOPES),
    });
    let response = api::http::shared()
        .post(endpoint)
        // The CLI sends JSON here and NO `anthropic-beta` header; the endpoint
        // rejects a form-encoded body.
        .header("Content-Type", "application/json")
        .timeout(TOKEN_TIMEOUT)
        .body(body.to_string())
        .send()
        .map_err(|err| TokenError::Transient(scrub_transport(&err)))?;
    let status = response.status().as_u16();
    let text = response.text().unwrap_or_default();
    if (200..300).contains(&status) {
        return parse_token_response(&text, refresh_token, now_ms)
            .ok_or_else(|| TokenError::Transient(format!("HTTP {status} (unparseable response)")));
    }
    let code = oauth_error_code(&text);
    if (400..500).contains(&status) && code.as_deref() == Some("invalid_grant") {
        return Err(TokenError::InvalidGrant);
    }
    Err(TokenError::Transient(match code {
        Some(code) => format!("HTTP {status} ({code})"),
        None => format!("HTTP {status}"),
    }))
}

/// Parse a 200 body into a rotated pair. `previous_refresh_token` is kept when
/// the response rotates none (the endpoint does that for long-lived grants).
pub fn parse_token_response(
    body: &str,
    previous_refresh_token: &str,
    now_ms: i64,
) -> Option<RotatedToken> {
    let doc: Value = serde_json::from_str(body).ok()?;
    let access_token = doc
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())?
        .to_string();
    let refresh_token = doc
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .unwrap_or(previous_refresh_token)
        .to_string();
    let expires_at_ms = match seconds(doc.get("expires_in")) {
        Some(expires_in) => now_ms.saturating_add(expires_in.saturating_mul(1_000)),
        // The endpoint always sends `expires_in`; if one ever does not, an
        // hour is the CLI's own token lifetime. Stamping "now" instead would
        // put the credential permanently inside the margin and burn a
        // single-use grant on every poll — the exact failure this module is
        // here to prevent. A too-long guess costs one 401 on the usage probe.
        None => {
            log::warn!("claude_oauth: token response carried no expires_in; assuming one hour");
            now_ms.saturating_add(3_600_000)
        }
    };
    let refresh_token_expires_at_ms = seconds(doc.get("refresh_token_expires_in"))
        .map(|expires_in| now_ms.saturating_add(expires_in.saturating_mul(1_000)));
    let scopes = doc
        .get("scope")
        .and_then(Value::as_str)
        .map(|scope| {
            scope
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|scopes| !scopes.is_empty());
    Some(RotatedToken {
        access_token,
        refresh_token,
        expires_at_ms,
        refresh_token_expires_at_ms,
        scopes,
    })
}

/// A `…_in` seconds field, as a number or a numeric string.
fn seconds(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|secs| secs as i64))
        .or_else(|| value.as_str()?.trim().parse().ok())
}

/// The `error` field of an OAuth error body — and ONLY if it looks like an
/// error CODE. Anything else (a sentence, a quoted token, an HTML page) is
/// dropped rather than risk carrying a secret into a log line.
fn oauth_error_code(body: &str) -> Option<String> {
    let code = serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")?
        .as_str()?
        .trim()
        .to_string();
    (!code.is_empty()
        && code.len() <= 40
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()))
    .then_some(code)
}

/// A transport failure, named by KIND only — no URL, no body, no headers.
fn scrub_transport(err: &impl fmt::Display) -> String {
    let text = err.to_string().to_ascii_lowercase();
    if text.contains("timed out") || text.contains("timeout") {
        "the token request timed out".to_string()
    } else if text.contains("connect") || text.contains("dns") {
        "the token request could not connect".to_string()
    } else {
        "the token request failed".to_string()
    }
}

// ---------------------------------------------------------------------------
// Merge + compare-and-swap
// ---------------------------------------------------------------------------

/// Lay the rotated fields OVER the previous `claudeAiOauth` object.
///
/// `refreshTokenExpiresAt` and `scopes` are written only when the response
/// carried them: a response that omits them says nothing about them, and
/// deleting a field the CLI wrote is how a credential loses its plan.
pub fn merge_oauth(previous: Option<&Value>, rotated: &RotatedToken) -> Value {
    let mut oauth = previous
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    oauth.insert(
        "accessToken".to_string(),
        Value::String(rotated.access_token.clone()),
    );
    oauth.insert(
        "refreshToken".to_string(),
        Value::String(rotated.refresh_token.clone()),
    );
    oauth.insert("expiresAt".to_string(), Value::from(rotated.expires_at_ms));
    if let Some(at) = rotated.refresh_token_expires_at_ms {
        oauth.insert("refreshTokenExpiresAt".to_string(), Value::from(at));
    }
    if let Some(scopes) = &rotated.scopes {
        oauth.insert(
            "scopes".to_string(),
            Value::Array(scopes.iter().cloned().map(Value::String).collect()),
        );
    }
    Value::Object(oauth)
}

/// The compare-and-swap: hand back the document to write, or `None` when the
/// store moved on.
///
/// The comparison is on the REFRESH token, not the access token: the refresh
/// token is what a rotation consumes, so "the stored refresh token is still
/// the one I POSTed" is exactly "nobody has rotated since I read". A document
/// with NO `claudeAiOauth` object is a signed-out store, not a blank one: the
/// CLI's own CAS refuses it (`L.claudeAiOauth!==void 0&&...!==null`), and so
/// do we, or a `claude logout` that landed mid-refresh would be undone.
pub fn cas_document(
    document: &Value,
    posted_refresh_token: &str,
    merged_oauth: Value,
) -> Option<Value> {
    let oauth = document.get("claudeAiOauth")?.as_object()?;
    let stored = oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if !stored.is_empty() && stored != posted_refresh_token {
        return None;
    }
    let mut next = document.as_object().cloned().unwrap_or_default();
    next.insert("claudeAiOauth".to_string(), merged_oauth);
    Some(Value::Object(next))
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// What the write-back did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Saved,
    /// A sibling rotated first; its newer credential stands and ours is
    /// dropped.
    AdoptedSibling,
    /// The store (or its `claudeAiOauth` branch) was gone by the time we came
    /// to write: a `claude logout` landed between the re-read and the CAS.
    /// Nothing is written; recreating the item would undo the logout.
    StoreGone,
    /// The keychain refused CLEANLY, so the rotated credential went to
    /// `<root>/.credentials.json` (the CLI's own `plaintext_fallback_used`).
    SavedToFallbackFile,
}

/// Why a store write did not happen.
#[derive(Debug)]
pub enum StoreError {
    /// The write did not finish in time. It MAY have landed — never treat this
    /// as "the store refused" and never fall back to another store after it.
    Timeout,
    Failed(String),
    Io(std::io::Error),
    /// The storage-write lock is held by someone else.
    Contended,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout => write!(f, "the credential store timed out"),
            Self::Failed(detail) => write!(f, "{detail}"),
            Self::Io(err) => write!(f, "credential store i/o failed: {err}"),
            Self::Contended => write!(f, "the credential store is locked by another process"),
        }
    }
}

/// Write a document to the store it came from. No locking — callers go through
/// [`write_back_locked`] (or the canary), which holds `.storage-write.lock`.
pub fn write_store(source: &CredentialSource, document: &Value) -> Result<(), StoreError> {
    let json = serde_json::to_string(document)
        .map_err(|err| StoreError::Failed(format!("credential document is unwritable: {err}")))?;
    match source {
        // 0600 temp + atomic rename: a torn `.credentials.json` is a signed-out
        // CLI.
        CredentialSource::File(path) => {
            api::atomic_file::write_atomic(path, &json).map_err(StoreError::Io)
        }
        #[cfg(target_os = "macos")]
        CredentialSource::Keychain { service, account } => write_keychain(service, account, &json),
    }
}

/// The storage-write lock's options.
///
/// A function rather than a const so the tests can make contention
/// deterministic — the real options retry for ~5 seconds, which is right for a
/// daemon and wrong for a test that wants to see `Contended`.
#[cfg(not(test))]
fn storage_lock_options() -> LockOptions {
    LockOptions::storage_write()
}

#[cfg(test)]
fn storage_lock_options() -> LockOptions {
    LockOptions {
        attempts: 1,
        backoff: Duration::ZERO,
        jitter: Duration::ZERO,
        ..LockOptions::storage_write()
    }
}

/// Run `body` while holding `<root>/.storage-write.lock`.
fn with_storage_lock<T>(
    root: &Path,
    body: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    let path = root.join(STORAGE_WRITE_LOCK_NAME);
    let guard = match lockfile::acquire(&path, storage_lock_options()) {
        Ok(guard) => guard,
        Err(LockError::Contended) => return Err(StoreError::Contended),
        Err(LockError::Io(err)) => return Err(StoreError::Io(err)),
    };
    let result = body();
    drop(guard);
    result
}

/// The CAS write-back, under `.storage-write.lock`: re-read, compare the
/// stored refresh token against the one we POSTed, write only if it is ours
/// (or blank). A store that vanished is left vanished.
pub fn write_back_locked(
    store: &ClaudeCredentialStore,
    posted_refresh_token: &str,
    merged_oauth: Value,
) -> Result<WriteOutcome, StoreError> {
    with_storage_lock(&store.config_root, || {
        cas_write(store, posted_refresh_token, merged_oauth)
    })
}

fn cas_write(
    store: &ClaudeCredentialStore,
    posted_refresh_token: &str,
    merged_oauth: Value,
) -> Result<WriteOutcome, StoreError> {
    let mut last_error = None;
    let mut last_document = None;
    for attempt in 1..=WRITE_ATTEMPTS {
        let current = match read_source(&store.source) {
            DocRead::Found(document) => document,
            // The store vanished under us (a `claude logout`, a wiped
            // profile). The grant is spent, but writing our pair into an item
            // the user just deleted would sign them back in; `read_keychain`'s
            // own rule is never to write to a service with no item.
            DocRead::Missing => {
                log::warn!("claude_oauth: the credential store vanished mid-refresh; not recreating it");
                return Ok(WriteOutcome::StoreGone);
            }
            DocRead::Denied => {
                last_error = Some(StoreError::Failed(
                    "the credential store refused a read".to_string(),
                ));
                backoff(attempt);
                continue;
            }
        };
        let Some(document) = cas_document(&current, posted_refresh_token, merged_oauth.clone())
        else {
            log::info!("claude_oauth: a sibling rotated first; keeping its credential");
            return Ok(WriteOutcome::AdoptedSibling);
        };
        match write_store(&store.source, &document) {
            Ok(()) => return Ok(WriteOutcome::Saved),
            // A write that may have landed: retrying it could raise a second
            // ACL modal, and falling back to a file could leave TWO live
            // copies. Surface it and let the caller back off.
            Err(StoreError::Timeout) => return Err(StoreError::Timeout),
            Err(err) => {
                last_error = Some(err);
                last_document = Some(document);
                backoff(attempt);
            }
        }
    }

    // The keychain refused CLEANLY every time (a revoked ACL, a keychain that
    // will not unlock). The grant is already spent, so the choice is between a
    // 0600 file beside the config and a credential nobody can use — the CLI
    // makes the same one (`plaintext_fallback_used`).
    #[cfg(target_os = "macos")]
    if let CredentialSource::Keychain { .. } = &store.source {
        if let Some(document) = last_document.take() {
            let path = store.config_root.join(CREDENTIALS_FILE);
            let json = serde_json::to_string(&document).map_err(|err| {
                StoreError::Failed(format!("credential document is unwritable: {err}"))
            })?;
            return match api::atomic_file::write_atomic(&path, &json) {
                Ok(()) => {
                    log::warn!(
                        "claude_oauth: the keychain refused the rotated credential; wrote {} instead",
                        path.display()
                    );
                    Ok(WriteOutcome::SavedToFallbackFile)
                }
                Err(err) => Err(StoreError::Io(err)),
            };
        }
    }
    let _ = last_document;
    Err(last_error.unwrap_or_else(|| {
        StoreError::Failed("the credential store write failed".to_string())
    }))
}

/// `100 * n` ms between CAS attempts; nothing after the last one.
fn backoff(attempt: u32) {
    if attempt < WRITE_ATTEMPTS {
        std::thread::sleep(Duration::from_millis(100 * u64::from(attempt)));
    }
}

// ---------------------------------------------------------------------------
// The refresh
// ---------------------------------------------------------------------------

/// A stable, non-reversible name for a refresh token — the first 16 hex of its
/// sha256.
///
/// Callers remember the markers of grants the endpoint already rejected so a
/// dead credential is never POSTed twice (each attempt costs a round trip and
/// tells the endpoint nothing new). A hash, not the token, so the memo can be
/// logged, persisted and compared without ever holding a secret.
pub fn dead_marker(refresh_token: &str) -> String {
    let digest = Sha256::digest(refresh_token.as_bytes());
    format!("{digest:x}")[..16].to_string()
}

/// One refresh request. Every `now`-based decision reads `now`; only the lock
/// module and the CAS backoff touch the wall clock.
pub struct RefreshRequest<'a> {
    /// `Some(profile_dir)` for an account profile, `None` for the ambient login.
    pub config_dir: Option<&'a Path>,
    pub token_endpoint: &'a str,
    /// Unix SECONDS.
    pub now: u64,
    /// How long before `expiresAt` a token counts as expiring.
    pub margin_secs: u64,
    /// [`dead_marker`]s of grants already rejected — never POSTed again.
    pub dead_refresh_tokens: &'a [String],
    pub lock_options: crate::lockfile::LockOptions,
}

/// What a refresh did. `Debug` is redacted.
pub enum RefreshOutcome {
    Refreshed {
        access_token: String,
        expires_at_ms: i64,
        wrote: WriteOutcome,
    },
    /// The stored token is good (or someone else just refreshed it) — use it.
    NotNeeded {
        access_token: String,
        expires_at_ms: Option<i64>,
    },
    /// No credential, or one with no refresh token: the CLI is signed out.
    NoRefreshToken,
    /// The grant is dead; the account needs a re-login. `dead_marker` is the
    /// memo the caller keeps so it is never POSTed again.
    InvalidGrant { dead_marker: String },
    /// Someone else holds the refresh lock. Nothing was spent.
    Contended,
    /// A transient failure. Nothing was spent, or nothing more can be done
    /// about what was.
    Failed(String),
    /// The credential store refused (a keychain ACL). Back off for an hour —
    /// retrying every few minutes just re-raises a modal nobody answers.
    Denied,
}

impl RefreshOutcome {
    /// A fixed word for logs and metrics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Refreshed { .. } => "refreshed",
            Self::NotNeeded { .. } => "not_needed",
            Self::NoRefreshToken => "no_refresh_token",
            Self::InvalidGrant { .. } => "invalid_grant",
            Self::Contended => "contended",
            Self::Failed(_) => "failed",
            Self::Denied => "denied",
        }
    }
}

impl fmt::Debug for RefreshOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refreshed {
                expires_at_ms,
                wrote,
                ..
            } => f
                .debug_struct("Refreshed")
                .field("access_token", &"<redacted>")
                .field("expires_at_ms", expires_at_ms)
                .field("wrote", wrote)
                .finish(),
            Self::NotNeeded { expires_at_ms, .. } => f
                .debug_struct("NotNeeded")
                .field("access_token", &"<redacted>")
                .field("expires_at_ms", expires_at_ms)
                .finish(),
            Self::NoRefreshToken => write!(f, "NoRefreshToken"),
            Self::InvalidGrant { dead_marker } => f
                .debug_struct("InvalidGrant")
                .field("dead_marker", dead_marker)
                .finish(),
            Self::Contended => write!(f, "Contended"),
            Self::Failed(detail) => f.debug_tuple("Failed").field(detail).finish(),
            Self::Denied => write!(f, "Denied"),
        }
    }
}

/// Refresh the credential if it is inside the margin; otherwise hand back what
/// is already stored. See the module doc for the whole contract.
pub fn refresh_if_expiring(request: RefreshRequest<'_>) -> RefreshOutcome {
    refresh_locked(request, |_| {})
}

/// [`refresh_if_expiring`] with a test seam: `after_lock` is called ONCE with
/// the config root, right after both locks are held and before the re-read —
/// the only window in which a test can play the part of a racing CLI.
pub(crate) fn refresh_locked(
    request: RefreshRequest<'_>,
    after_lock: impl FnOnce(&Path),
) -> RefreshOutcome {
    let Some(root) = claude_config_root(request.config_dir) else {
        // No home directory and no configured root: there is nowhere to read
        // and nowhere to lock. Treated as a store refusal so the caller backs
        // off rather than spinning.
        return RefreshOutcome::Denied;
    };
    let now_ms = (request.now as i64).saturating_mul(1_000);
    let margin_ms = (request.margin_secs as i64).saturating_mul(1_000);

    // 1. The pre-lock read: is this worth a lock at all?
    let store = match read_store(request.config_dir) {
        StoreRead::Found(store) => store,
        StoreRead::Missing => return RefreshOutcome::NoRefreshToken,
        StoreRead::Denied => return RefreshOutcome::Denied,
    };
    let ours_access = store.access_token().map(str::to_string);
    match precheck(&store, &request, now_ms, margin_ms) {
        Precheck::Refresh => {}
        Precheck::Answer(outcome) => return outcome,
    }

    // 2. The locks. Declared in acquisition order so the drops run in reverse:
    // the legacy lock is released BEFORE the refresh lock, as the CLI does.
    if let Err(err) = std::fs::create_dir_all(&root) {
        return RefreshOutcome::Failed(format!("could not create {}: {err}", root.display()));
    }
    let refresh_lock = match lockfile::acquire(&root.join(REFRESH_LOCK_NAME), request.lock_options)
    {
        Ok(guard) => guard,
        Err(LockError::Contended) => return RefreshOutcome::Contended,
        Err(LockError::Io(err)) => {
            return RefreshOutcome::Failed(format!("refresh lock failed: {err}"))
        }
    };
    let _legacy_lock = match acquire_legacy_lock(&root, request.lock_options) {
        LegacyLock::Held(guard) => Some(guard),
        LegacyLock::Contended => return RefreshOutcome::Contended,
        LegacyLock::Skipped => None,
    };

    after_lock(&root);

    // 3. The re-read: did anyone refresh while we queued?
    let store = match read_store(request.config_dir) {
        StoreRead::Found(store) => store,
        StoreRead::Missing => return RefreshOutcome::NoRefreshToken,
        StoreRead::Denied => return RefreshOutcome::Denied,
    };
    if let (Some(theirs), Some(ours)) = (store.access_token(), ours_access.as_deref()) {
        if theirs != ours {
            log::debug!("claude_oauth: another process refreshed while we waited for the lock");
            return RefreshOutcome::NotNeeded {
                access_token: theirs.to_string(),
                expires_at_ms: store.expires_at_ms(),
            };
        }
    }
    let refresh_token = match precheck(&store, &request, now_ms, margin_ms) {
        Precheck::Refresh => match store.refresh_token() {
            Some(token) => token.to_string(),
            None => return RefreshOutcome::NoRefreshToken,
        },
        Precheck::Answer(outcome) => return outcome,
    };
    if refresh_lock.is_compromised() {
        // Someone reclaimed our lock: they may be inside the same critical
        // section right now, and two POSTs of one grant leave one of us dead.
        return RefreshOutcome::Failed("the refresh lock was taken over".to_string());
    }

    // 4. The canary — prove we can write BEFORE we spend the grant.
    #[cfg(target_os = "macos")]
    if matches!(store.source, CredentialSource::Keychain { .. }) {
        if let Err(err) = with_storage_lock(&store.config_root, || {
            write_store(&store.source, &store.document)
        }) {
            // Every failure lands here, contention included: a storage lock we
            // cannot take is a write we could not have made either, and
            // `Denied`'s long backoff is the right answer to both an ACL modal
            // and a machine whose CLI is busy writing.
            log::warn!("claude_oauth: the credential store refused a canary write: {err}");
            return RefreshOutcome::Denied;
        }
    }

    // 5. The POST.
    let rotated = match post_refresh(
        request.token_endpoint,
        &refresh_token,
        store.client_id(),
        store.scopes_string().as_deref(),
        now_ms,
    ) {
        Ok(rotated) => rotated,
        Err(TokenError::InvalidGrant) => {
            return RefreshOutcome::InvalidGrant {
                dead_marker: dead_marker(&refresh_token),
            }
        }
        Err(TokenError::Transient(detail)) => return RefreshOutcome::Failed(detail),
    };

    // 6. The write-back. The grant is spent either way, so a compromised lock
    // is logged and the CAS-guarded write goes ahead: losing the only copy of
    // the new token is strictly worse than writing under a lock we may have
    // lost.
    if refresh_lock.is_compromised() {
        log::warn!("claude_oauth: the refresh lock was taken over mid-refresh; writing anyway");
    }
    let merged = merge_oauth(store.document.get("claudeAiOauth"), &rotated);
    match write_back_locked(&store, &refresh_token, merged) {
        Ok(wrote) => RefreshOutcome::Refreshed {
            access_token: rotated.access_token,
            expires_at_ms: rotated.expires_at_ms,
            wrote,
        },
        // The worst case: a fresh pair we could not store. Reporting it as a
        // failure (rather than a refresh) is deliberate — the next cycle
        // re-reads the store and either finds the write landed after all or
        // gets `invalid_grant` and asks for a re-login, which is the truth.
        Err(err) => {
            log::error!("claude_oauth: the rotated credential could not be stored: {err}");
            RefreshOutcome::Failed(format!("the rotated credential could not be stored: {err}"))
        }
    }
}

/// The checks that run both before and after the lock.
enum Precheck {
    Refresh,
    Answer(RefreshOutcome),
}

fn precheck(
    store: &ClaudeCredentialStore,
    request: &RefreshRequest<'_>,
    now_ms: i64,
    margin_ms: i64,
) -> Precheck {
    let Some(refresh_token) = store.refresh_token() else {
        return Precheck::Answer(RefreshOutcome::NoRefreshToken);
    };
    let marker = dead_marker(refresh_token);
    if request.dead_refresh_tokens.contains(&marker) {
        return Precheck::Answer(RefreshOutcome::InvalidGrant {
            dead_marker: marker,
        });
    }
    match store.access_token() {
        // Nothing to serve: refresh even if `expiresAt` says otherwise.
        None => Precheck::Refresh,
        Some(_) if expiring(store.expires_at_ms(), now_ms, margin_ms) => Precheck::Refresh,
        Some(access_token) => Precheck::Answer(RefreshOutcome::NotNeeded {
            access_token: access_token.to_string(),
            expires_at_ms: store.expires_at_ms(),
        }),
    }
}

/// The CLI's `Y1`: `now + margin >= expiresAt`. An already-expired token is
/// inside it. A credential with NO `expiresAt` is left alone — `agent_usage`
/// treats an unknown expiry as live, and refreshing on a guess spends a
/// single-use grant for nothing.
fn expiring(expires_at_ms: Option<i64>, now_ms: i64, margin_ms: i64) -> bool {
    match expires_at_ms {
        Some(at) => now_ms.saturating_add(margin_ms) >= at,
        None => false,
    }
}

enum LegacyLock {
    Held(LockGuard),
    Contended,
    Skipped,
}

/// The CLI's legacy `<realpath(root)>.lock` (so `~/.claude.lock`). Contention
/// is real contention; any other failure is logged and ignored.
#[cfg(not(windows))]
fn acquire_legacy_lock(root: &Path, options: LockOptions) -> LegacyLock {
    let path = match legacy_lock_path(root) {
        Ok(path) => path,
        Err(err) => {
            log::warn!(
                "claude_oauth: could not resolve the legacy config lock for {}: {err}",
                root.display()
            );
            return LegacyLock::Skipped;
        }
    };
    match lockfile::acquire(&path, options) {
        Ok(guard) => LegacyLock::Held(guard),
        Err(LockError::Contended) => LegacyLock::Contended,
        Err(LockError::Io(err)) => {
            log::warn!("claude_oauth: the legacy config lock is unusable: {err}");
            LegacyLock::Skipped
        }
    }
}

/// Windows: node's `realpath` and Rust's `canonicalize` produce different
/// strings for the same directory (`\\?\C:\…`), so the lock we would take is
/// not the lock the CLI takes — a lock nobody collides on is only a new way to
/// fail. The `.oauth_refresh.lock` inside the root is identical on both sides
/// and carries the guarantee.
#[cfg(windows)]
fn acquire_legacy_lock(_root: &Path, _options: LockOptions) -> LegacyLock {
    LegacyLock::Skipped
}

#[cfg(not(windows))]
fn legacy_lock_path(root: &Path) -> std::io::Result<PathBuf> {
    let real = std::fs::canonicalize(root)?;
    Ok(PathBuf::from(format!("{}.lock", real.to_string_lossy())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{canned_server_recording, temp_dir, TempDir};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    const NOW: u64 = 1_700_000_000;
    const NOW_MS: i64 = 1_700_000_000_000;
    const MARGIN: u64 = 900;

    fn fast_options() -> LockOptions {
        LockOptions {
            stale: Duration::from_secs(60),
            update: Duration::from_millis(50),
            attempts: 1,
            backoff: Duration::ZERO,
            jitter: Duration::ZERO,
        }
    }

    fn document(access: &str, refresh: &str, expires_at_ms: i64) -> Value {
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": access,
                "refreshToken": refresh,
                "expiresAt": expires_at_ms,
                "subscriptionType": "max",
                "scopes": ["user:profile", "user:inference"],
            }
        })
    }

    fn seed(dir: &TempDir, document: &Value) -> PathBuf {
        let path = dir.0.join(CREDENTIALS_FILE);
        std::fs::write(&path, document.to_string()).unwrap();
        path
    }

    fn read_back(dir: &TempDir) -> Value {
        let raw = std::fs::read_to_string(dir.0.join(CREDENTIALS_FILE)).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn request<'a>(
        dir: &'a TempDir,
        endpoint: &'a str,
        dead: &'a [String],
    ) -> RefreshRequest<'a> {
        RefreshRequest {
            config_dir: Some(&dir.0),
            token_endpoint: endpoint,
            now: NOW,
            margin_secs: MARGIN,
            dead_refresh_tokens: dead,
            lock_options: fast_options(),
        }
    }

    fn rotated_body() -> String {
        r#"{"access_token":"at-2","refresh_token":"rt-2","expires_in":3600,"scope":"user:profile user:inference"}"#
            .to_string()
    }

    fn recorded(requests: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        requests.lock().unwrap().clone()
    }

    /// No lock directory of either kind survives a finished refresh.
    fn assert_locks_released(dir: &TempDir) {
        assert!(
            !dir.0.join(REFRESH_LOCK_NAME).exists(),
            "the refresh lock is released"
        );
        assert!(
            !dir.0.join(STORAGE_WRITE_LOCK_NAME).exists(),
            "the storage-write lock is released"
        );
        #[cfg(not(windows))]
        assert!(
            !legacy_lock_path(&dir.0).unwrap().exists(),
            "the legacy config lock is released"
        );
    }

    // -- pure -------------------------------------------------------------

    #[test]
    fn merging_an_oauth_object_preserves_unknown_keys() {
        let previous = serde_json::json!({
            "accessToken": "at-1",
            "refreshToken": "rt-1",
            "expiresAt": 1,
            "subscriptionType": "max",
            "rateLimitTier": "default",
            "clientId": "client-9",
        });
        let merged = merge_oauth(
            Some(&previous),
            &RotatedToken {
                access_token: "at-2".into(),
                refresh_token: "rt-2".into(),
                expires_at_ms: 42,
                refresh_token_expires_at_ms: None,
                scopes: None,
            },
        );

        assert_eq!(merged["accessToken"], "at-2");
        assert_eq!(merged["refreshToken"], "rt-2");
        assert_eq!(merged["expiresAt"], 42);
        assert_eq!(merged["subscriptionType"], "max");
        assert_eq!(merged["rateLimitTier"], "default");
        assert_eq!(merged["clientId"], "client-9");
        assert!(
            merged.get("refreshTokenExpiresAt").is_none(),
            "a field the response did not carry is not invented"
        );
    }

    #[test]
    fn merging_preserves_sibling_top_level_keys() {
        let doc = serde_json::json!({
            "claudeAiOauth": { "refreshToken": "rt-1", "accessToken": "at-1" },
            "somethingElseEntirely": { "nested": [1, 2, 3] },
            "aTopLevelString": "keep me",
        });
        let merged = merge_oauth(
            doc.get("claudeAiOauth"),
            &RotatedToken {
                access_token: "at-2".into(),
                refresh_token: "rt-2".into(),
                expires_at_ms: 7,
                refresh_token_expires_at_ms: Some(9),
                scopes: Some(vec!["user:profile".into()]),
            },
        );
        let next = cas_document(&doc, "rt-1", merged).expect("the stored token is still ours");

        assert_eq!(next["somethingElseEntirely"]["nested"][2], 3);
        assert_eq!(next["aTopLevelString"], "keep me");
        assert_eq!(next["claudeAiOauth"]["accessToken"], "at-2");
        assert_eq!(next["claudeAiOauth"]["refreshTokenExpiresAt"], 9);
        assert_eq!(next["claudeAiOauth"]["scopes"][0], "user:profile");
    }

    #[test]
    fn cas_refuses_when_the_stored_refresh_token_moved() {
        let doc = serde_json::json!({ "claudeAiOauth": { "refreshToken": "rt-9" } });
        assert!(
            cas_document(&doc, "rt-1", serde_json::json!({ "accessToken": "at-2" })).is_none(),
            "a sibling's newer credential is never overwritten"
        );
    }

    #[test]
    fn cas_accepts_a_blank_stored_refresh_token() {
        let doc = serde_json::json!({ "claudeAiOauth": { "refreshToken": "" } });
        let next = cas_document(&doc, "rt-1", serde_json::json!({ "refreshToken": "rt-2" }))
            .expect("a blank stored token is nobody's rotation");
        assert_eq!(next["claudeAiOauth"]["refreshToken"], "rt-2");
        // An object with no `refreshToken` key at all is the same allowance.
        let doc = serde_json::json!({ "claudeAiOauth": { "accessToken": "at-1" } });
        assert!(cas_document(&doc, "rt-1", serde_json::json!({})).is_some());
    }

    /// A document whose `claudeAiOauth` branch is gone is a SIGNED-OUT store
    /// (`claude logout` deletes exactly that branch), not a blank one: the
    /// CAS refuses it, as the CLI's does, so a logout that landed during our
    /// POST is not undone by the write-back.
    #[test]
    fn cas_refuses_a_document_with_no_oauth_object() {
        let rotated = serde_json::json!({ "refreshToken": "rt-2" });
        for doc in [
            serde_json::json!({}),
            serde_json::json!({ "mcpOAuth": { "srv": {} } }),
            serde_json::json!({ "claudeAiOauth": null }),
            serde_json::json!({ "claudeAiOauth": "not an object" }),
        ] {
            assert!(
                cas_document(&doc, "rt-1", rotated.clone()).is_none(),
                "a signed-out store is never rewritten: {doc}"
            );
        }
    }

    /// The end-to-end half: the store answered the re-read, the POST rotated
    /// the pair, and by the time the storage lock is held the file is gone. The
    /// write-back reports [`WriteOutcome::StoreGone`], creates nothing, and
    /// releases the lock.
    #[test]
    fn a_vanished_store_is_not_rewritten() {
        let dir = temp_dir("claude-oauth-vanished");
        let document = document("at-1", "rt-1", NOW_MS + 60_000);
        let path = seed(&dir, &document);
        let store = ClaudeCredentialStore {
            source: CredentialSource::File(path.clone()),
            config_root: dir.0.clone(),
            document,
        };
        // The logout, between our re-read and our write.
        std::fs::remove_file(&path).unwrap();

        let merged = serde_json::json!({
            "accessToken": "at-2",
            "refreshToken": "rt-2",
            "expiresAt": NOW_MS + 3_600_000,
        });
        let outcome = write_back_locked(&store, "rt-1", merged).unwrap();

        assert_eq!(outcome, WriteOutcome::StoreGone);
        assert!(!path.exists(), "the logout stands: nothing was recreated");
        assert!(
            !dir.0.join(STORAGE_WRITE_LOCK_NAME).exists(),
            "the storage-write lock is released"
        );
    }

    #[test]
    fn parse_token_response_turns_expires_in_into_absolute_millis() {
        let rotated = parse_token_response(
            r#"{"access_token":"at-2","expires_in":3600,"refresh_token_expires_in":86400}"#,
            "rt-1",
            NOW_MS,
        )
        .unwrap();
        assert_eq!(rotated.expires_at_ms, NOW_MS + 3_600_000);
        assert_eq!(
            rotated.refresh_token_expires_at_ms,
            Some(NOW_MS + 86_400_000)
        );
    }

    #[test]
    fn parse_token_response_keeps_the_old_refresh_token_when_none_is_returned() {
        let rotated =
            parse_token_response(r#"{"access_token":"at-2","expires_in":60}"#, "rt-1", NOW_MS)
                .unwrap();
        assert_eq!(rotated.refresh_token, "rt-1");
        assert_eq!(rotated.access_token, "at-2");
        assert!(rotated.scopes.is_none());
    }

    #[test]
    fn an_invalid_grant_body_maps_to_invalid_grant() {
        let (base, _requests) =
            canned_server_recording(vec![(400, r#"{"error":"invalid_grant"}"#.to_string())]);
        let err = post_refresh(&base, "rt-1", None, None, NOW_MS).unwrap_err();
        assert_eq!(err, TokenError::InvalidGrant);
    }

    #[test]
    fn a_token_error_never_renders_a_token() {
        let (base, _requests) = canned_server_recording(vec![(
            500,
            r#"{"error":"secret-token-xyz","error_description":"secret-token-xyz leaked"}"#
                .to_string(),
        )]);
        let err = post_refresh(&base, "secret-token-xyz", None, None, NOW_MS).unwrap_err();
        assert!(!format!("{err}").contains("secret-token-xyz"), "{err}");
        assert!(!format!("{err:?}").contains("secret-token-xyz"), "{err:?}");
        assert!(format!("{err}").contains("500"), "the status still shows: {err}");
    }

    #[test]
    fn the_config_root_prefers_the_profile_dir() {
        let dir = temp_dir("claude-oauth-root");
        assert_eq!(claude_config_root(Some(&dir.0)).as_deref(), Some(dir.0.as_path()));
        // And a profile root never borrows the ambient one's file.
        assert_eq!(
            claude_config_root(Some(Path::new("/tmp/exp-profile-abc"))),
            Some(PathBuf::from("/tmp/exp-profile-abc"))
        );
    }

    /// The CLI's `aP()` picks the keychain service by the PRESENCE of the
    /// selecting variable, not by where it points: `CLAUDE_CONFIG_DIR` set to
    /// the default directory is that same directory under the SUFFIXED
    /// service, an empty `CLAUDE_SECURESTORAGE_CONFIG_DIR` is the plain
    /// default even beside a set `CLAUDE_CONFIG_DIR`, and a profile dir (which
    /// our launcher exports) is always hashed. Pure: no environment, no
    /// keychain.
    #[test]
    fn a_config_dir_variable_at_the_default_path_selects_the_suffixed_service() {
        let default = PathBuf::from("/tmp/exp-claude-home/.claude");
        let default_str = default.to_str().unwrap();
        let resolve = |config_dir: Option<&Path>, secure: Option<&str>, var: Option<&str>| {
            resolve_root_from(config_dir, Some(default.clone()), secure, var).unwrap()
        };

        let plain = resolve(None, None, None);
        assert_eq!(plain.path, default);
        assert!(plain.default_location, "nothing set: the plain service");

        let by_var = resolve(None, None, Some(default_str));
        assert_eq!(by_var.path, default, "the same directory");
        assert!(!by_var.default_location, "...under the suffixed service");

        let by_secure = resolve(None, Some(default_str), None);
        assert_eq!(by_secure.path, default);
        assert!(!by_secure.default_location);

        let empty_secure = resolve(None, Some(""), Some("/tmp/exp-elsewhere"));
        assert_eq!(empty_secure.path, default, "an empty override wins and means the default");
        assert!(empty_secure.default_location);

        let blank_var = resolve(None, None, Some("  "));
        assert_eq!(blank_var.path, default);
        assert!(blank_var.default_location, "set-but-blank is not set");

        let profile = resolve(Some(&default), None, None);
        assert!(!profile.default_location, "a profile dir is exported, so hashed");

        #[cfg(target_os = "macos")]
        {
            let digest = Sha256::digest(default_str.as_bytes());
            let suffixed = format!("Claude Code-credentials-{}", &format!("{digest:x}")[..8]);
            assert_eq!(keychain_service_for(&plain), "Claude Code-credentials");
            assert_eq!(keychain_service_for(&by_var), suffixed);
            assert_eq!(keychain_service_for(&by_secure), suffixed);
            assert_eq!(keychain_service_for(&empty_secure), "Claude Code-credentials");
            assert_eq!(keychain_service_for(&profile), suffixed);
        }
    }

    #[test]
    fn a_dead_marker_is_a_stable_sixteen_hex_prefix() {
        let marker = dead_marker("rt-1");
        assert_eq!(marker.len(), 16);
        assert!(marker.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(marker, dead_marker("rt-1"), "stable across calls");
        assert_ne!(marker, dead_marker("rt-2"));
        let digest = Sha256::digest(b"rt-1");
        assert_eq!(marker, format!("{digest:x}")[..16]);
        assert!(!marker.contains("rt-1"));
    }

    // -- macOS keychain ---------------------------------------------------

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_command_uses_stdin_under_the_limit() {
        let command = keychain_write_command("Claude Code-credentials", "niach", "{\"a\":1}");
        match command {
            KeychainWrite::Stdin(line) => {
                assert!(line.starts_with("add-generic-password -U -a \"niach\" -s \"Claude Code-credentials\" -X "));
                assert!(line.ends_with('\n'));
                assert!(line.contains(&hex_encode(b"{\"a\":1}")));
                assert!(line.len() <= KEYCHAIN_STDIN_LIMIT);
            }
            other => panic!("expected stdin delivery, got {other:?}"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_command_falls_back_to_argv_over_the_limit() {
        let json = format!("{{\"claudeAiOauth\":{{\"accessToken\":\"{}\"}}}}", "x".repeat(4_000));
        match keychain_write_command("Claude Code-credentials", "niach", &json) {
            KeychainWrite::Argv(args) => {
                assert_eq!(args[0], "add-generic-password");
                assert_eq!(args[1], "-U");
                assert_eq!(args[2..6], ["-a", "niach", "-s", "Claude Code-credentials"]);
                assert_eq!(args[6], "-X");
                assert_eq!(args[7], hex_encode(json.as_bytes()));
            }
            other => panic!("expected argv delivery, got {other:?}"),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_hostile_user_name_falls_back_to_claude_code_user() {
        assert_eq!(keychain_account_or_default(Some("niach")), "niach");
        assert_eq!(keychain_account_or_default(Some("dev.user-1_x")), "dev.user-1_x");
        for hostile in [
            "\" -s \"Claude Code-credentials",
            "niach\nadd-generic-password",
            "a user",
            "",
            "   ",
        ] {
            assert_eq!(
                keychain_account_or_default(Some(hostile)),
                FALLBACK_KEYCHAIN_ACCOUNT,
                "{hostile:?} is not a plain account name"
            );
        }
        assert_eq!(keychain_account_or_default(None), FALLBACK_KEYCHAIN_ACCOUNT);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_profile_keychain_service_matches_the_cli_hash() {
        let path = Path::new("/tmp/exp-claude-profile-fixture");
        let digest = Sha256::digest(b"/tmp/exp-claude-profile-fixture");
        let expected = format!("Claude Code-credentials-{}", &format!("{digest:x}")[..8]);
        assert_eq!(keychain_service(Some(path)), expected);

        // The ambient default carries NO suffix — but only when this process
        // is not itself running under a relocated config dir, so the
        // expectation is computed from the environment rather than assumed.
        let relocated = std::env::var("CLAUDE_CONFIG_DIR").is_ok()
            || std::env::var("CLAUDE_SECURESTORAGE_CONFIG_DIR").is_ok();
        let ambient = keychain_service(None);
        if relocated {
            assert!(ambient.starts_with("Claude Code-credentials"));
        } else {
            assert_eq!(ambient, "Claude Code-credentials");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_attributes_yield_the_account_name() {
        let dump = "keychain: \"/Users/niach/Library/Keychains/login.keychain-db\"\n\
                    attributes:\n    \"acct\"<blob>=\"niach\"\n    \"svce\"<blob>=\"Claude Code-credentials\"\n";
        assert_eq!(parse_keychain_account(dump).as_deref(), Some("niach"));
        // The hex rendering `security` uses for non-plain values.
        let hex = "    \"acct\"<blob>=0x6E69616368  \"niach\"\n";
        assert_eq!(parse_keychain_account(hex).as_deref(), Some("niach"));
        assert_eq!(parse_keychain_account("attributes:\n    \"svce\"<blob>=\"x\"\n"), None);
    }

    /// The `security -i` plumbing this module put in the doctor: a child that
    /// READS gets the bytes and sees EOF, and a child that never reads cannot
    /// wedge us on a full pipe.
    #[cfg(unix)]
    #[test]
    fn the_stdin_probe_feeds_a_child_and_never_wedges() {
        let mut cat = terminal::process::background_command("/bin/cat");
        cat.arg("-");
        let output =
            crate::doctor::output_with_stdin_timeout(cat, "hello\n", Duration::from_secs(5))
                .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");

        // 1 MiB at a child that exits without reading a byte: the write blocks
        // on a full pipe buffer, then fails when the child is gone — and the
        // call still returns.
        let deaf = terminal::process::background_command("/usr/bin/true");
        let output = crate::doctor::output_with_stdin_timeout(
            deaf,
            &"x".repeat(1024 * 1024),
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(output.status.success());
    }

    // -- routine ----------------------------------------------------------

    #[test]
    fn a_refresh_writes_the_rotated_credential_and_posts_exactly_once() {
        let dir = temp_dir("claude-oauth-refresh");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let (base, requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        match outcome {
            RefreshOutcome::Refreshed {
                access_token,
                expires_at_ms,
                wrote,
            } => {
                assert_eq!(access_token, "at-2");
                assert_eq!(expires_at_ms, NOW_MS + 3_600_000);
                assert_eq!(wrote, WriteOutcome::Saved);
            }
            other => panic!("expected a refresh, got {other:?}"),
        }

        let sent = recorded(&requests);
        assert_eq!(sent.len(), 1, "exactly one POST");
        let body = sent[0].to_ascii_lowercase();
        assert!(body.contains("content-type: application/json"), "{}", sent[0]);
        assert!(!body.contains("anthropic-beta"), "{}", sent[0]);
        assert!(sent[0].contains(r#""grant_type":"refresh_token""#), "{}", sent[0]);
        assert!(sent[0].contains(r#""refresh_token":"rt-1""#), "{}", sent[0]);
        assert!(sent[0].contains(CLAUDE_OAUTH_CLIENT_ID), "{}", sent[0]);
        assert!(sent[0].contains("user:profile user:inference"), "{}", sent[0]);

        let stored = read_back(&dir);
        assert_eq!(stored["claudeAiOauth"]["accessToken"], "at-2");
        assert_eq!(stored["claudeAiOauth"]["refreshToken"], "rt-2");
        assert_eq!(stored["claudeAiOauth"]["expiresAt"], NOW_MS + 3_600_000);
        assert_eq!(stored["claudeAiOauth"]["subscriptionType"], "max");
        assert_eq!(stored["claudeAiOauth"]["scopes"][1], "user:inference");
        assert_locks_released(&dir);
    }

    #[test]
    fn a_credential_document_keeps_its_siblings_through_a_rotation() {
        let dir = temp_dir("claude-oauth-siblings");
        let mut doc = document("at-1", "rt-1", NOW_MS + 60_000);
        doc["someOtherLogin"] = serde_json::json!({ "token": "leave-me-alone" });
        doc["numberOfThings"] = Value::from(3);
        seed(&dir, &doc);
        let (base, _requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));
        assert!(matches!(outcome, RefreshOutcome::Refreshed { .. }), "{outcome:?}");

        let stored = read_back(&dir);
        assert_eq!(stored["someOtherLogin"]["token"], "leave-me-alone");
        assert_eq!(stored["numberOfThings"], 3);
        assert_eq!(stored["claudeAiOauth"]["accessToken"], "at-2");
    }

    #[test]
    fn a_token_that_moved_under_the_lock_posts_nothing() {
        let dir = temp_dir("claude-oauth-moved");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let (base, requests) = canned_server_recording(vec![(200, rotated_body())]);

        // The seam plays the racing CLI: it rotates the file while we hold the
        // locks, between the pre-lock read and the re-read.
        let outcome = refresh_locked(request(&dir, &base, &[]), |root| {
            let path = root.join(CREDENTIALS_FILE);
            let raw = std::fs::read_to_string(&path).unwrap();
            let mut doc: Value = serde_json::from_str(&raw).unwrap();
            doc["claudeAiOauth"]["accessToken"] = Value::from("at-9");
            doc["claudeAiOauth"]["refreshToken"] = Value::from("rt-9");
            std::fs::write(&path, doc.to_string()).unwrap();
        });

        match outcome {
            RefreshOutcome::NotNeeded { access_token, .. } => assert_eq!(access_token, "at-9"),
            other => panic!("expected the sibling's token, got {other:?}"),
        }
        assert!(recorded(&requests).is_empty(), "the grant was not spent");
        assert_locks_released(&dir);
    }

    #[test]
    fn a_fresh_foreign_refresh_lock_posts_nothing() {
        let dir = temp_dir("claude-oauth-contended");
        let doc = document("at-1", "rt-1", NOW_MS + 60_000);
        seed(&dir, &doc);
        let foreign = dir.0.join(REFRESH_LOCK_NAME);
        std::fs::create_dir(&foreign).unwrap();
        lockfile::set_dir_mtime(&foreign, lockfile::whole_second(SystemTime::now())).unwrap();
        let (base, requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        assert!(matches!(outcome, RefreshOutcome::Contended), "{outcome:?}");
        assert!(recorded(&requests).is_empty());
        assert_eq!(read_back(&dir), doc, "the credential is untouched");
        assert!(foreign.is_dir(), "a live holder's lock is left alone");
    }

    #[test]
    fn a_stale_refresh_lock_is_reclaimed_and_the_refresh_posts_once() {
        let dir = temp_dir("claude-oauth-stale");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let abandoned = dir.0.join(REFRESH_LOCK_NAME);
        std::fs::create_dir(&abandoned).unwrap();
        lockfile::set_dir_mtime(
            &abandoned,
            lockfile::whole_second(SystemTime::now() - Duration::from_secs(120)),
        )
        .unwrap();
        let (base, requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        assert!(matches!(outcome, RefreshOutcome::Refreshed { .. }), "{outcome:?}");
        assert_eq!(recorded(&requests).len(), 1);
        assert_eq!(read_back(&dir)["claudeAiOauth"]["accessToken"], "at-2");
        assert_locks_released(&dir);
    }

    #[test]
    fn an_invalid_grant_writes_nothing_and_names_the_dead_token() {
        let dir = temp_dir("claude-oauth-dead");
        let doc = document("at-1", "rt-1", NOW_MS + 60_000);
        seed(&dir, &doc);
        let before = std::fs::read(dir.0.join(CREDENTIALS_FILE)).unwrap();
        let (base, requests) =
            canned_server_recording(vec![(400, r#"{"error":"invalid_grant"}"#.to_string())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        match outcome {
            RefreshOutcome::InvalidGrant { dead_marker: marker } => {
                assert_eq!(marker, dead_marker("rt-1"));
            }
            other => panic!("expected invalid_grant, got {other:?}"),
        }
        assert_eq!(recorded(&requests).len(), 1);
        assert_eq!(
            std::fs::read(dir.0.join(CREDENTIALS_FILE)).unwrap(),
            before,
            "a dead grant never rewrites the store"
        );
        assert_locks_released(&dir);
    }

    #[test]
    fn a_known_dead_refresh_token_posts_nothing() {
        let dir = temp_dir("claude-oauth-known-dead");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let (base, requests) = canned_server_recording(vec![]);
        let dead = vec![dead_marker("rt-1")];

        let outcome = refresh_if_expiring(request(&dir, &base, &dead));

        match outcome {
            RefreshOutcome::InvalidGrant { dead_marker: marker } => {
                assert_eq!(marker, dead_marker("rt-1"))
            }
            other => panic!("expected invalid_grant, got {other:?}"),
        }
        assert!(recorded(&requests).is_empty(), "a known-dead grant is never retried");
        assert!(!dir.0.join(REFRESH_LOCK_NAME).exists(), "not even locked");
    }

    #[test]
    fn a_token_outside_the_margin_posts_nothing() {
        let dir = temp_dir("claude-oauth-fresh");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 10 * 3_600_000));
        let (base, requests) = canned_server_recording(vec![]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        match outcome {
            RefreshOutcome::NotNeeded {
                access_token,
                expires_at_ms,
            } => {
                assert_eq!(access_token, "at-1");
                assert_eq!(expires_at_ms, Some(NOW_MS + 10 * 3_600_000));
            }
            other => panic!("expected not-needed, got {other:?}"),
        }
        assert!(recorded(&requests).is_empty());
        assert!(!dir.0.join(REFRESH_LOCK_NAME).exists(), "not even locked");
    }

    #[test]
    fn an_expired_token_with_a_refresh_token_is_refreshed() {
        let dir = temp_dir("claude-oauth-expired");
        seed(&dir, &document("at-1", "rt-1", NOW_MS - 5_000));
        let (base, requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));

        assert!(matches!(outcome, RefreshOutcome::Refreshed { .. }), "{outcome:?}");
        assert_eq!(recorded(&requests).len(), 1);
        assert_eq!(read_back(&dir)["claudeAiOauth"]["accessToken"], "at-2");
    }

    #[test]
    fn the_storage_write_lock_is_taken_and_released_around_the_write() {
        let dir = temp_dir("claude-oauth-storage-lock");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let (base, _requests) = canned_server_recording(vec![(200, rotated_body())]);

        let outcome = refresh_if_expiring(request(&dir, &base, &[]));
        assert!(matches!(outcome, RefreshOutcome::Refreshed { .. }), "{outcome:?}");
        assert_locks_released(&dir);

        // And a write-back cannot proceed while someone else holds it.
        let held = dir.0.join(STORAGE_WRITE_LOCK_NAME);
        std::fs::create_dir(&held).unwrap();
        lockfile::set_dir_mtime(&held, lockfile::whole_second(SystemTime::now())).unwrap();
        let store = match read_store(Some(&dir.0)) {
            StoreRead::Found(store) => store,
            other => panic!("expected the seeded store, got {other:?}"),
        };
        let blocked = write_back_locked(
            &store,
            "rt-2",
            serde_json::json!({ "accessToken": "at-3", "refreshToken": "rt-3" }),
        )
        .unwrap_err();
        assert!(matches!(blocked, StoreError::Contended), "{blocked:?}");
        assert_eq!(
            read_back(&dir)["claudeAiOauth"]["accessToken"],
            "at-2",
            "a contended lock writes nothing"
        );
        let _ = std::fs::remove_dir(&held);
    }

    #[cfg(not(windows))]
    #[test]
    fn the_legacy_config_lock_is_taken_and_released() {
        let dir = temp_dir("claude-oauth-legacy-lock");
        seed(&dir, &document("at-1", "rt-1", NOW_MS + 60_000));
        let (base, _requests) = canned_server_recording(vec![(200, rotated_body())]);
        let legacy = legacy_lock_path(&dir.0).unwrap();
        let seen = std::cell::Cell::new(false);

        let outcome = refresh_locked(request(&dir, &base, &[]), |_| {
            seen.set(legacy.is_dir());
        });

        assert!(matches!(outcome, RefreshOutcome::Refreshed { .. }), "{outcome:?}");
        assert!(seen.get(), "the legacy config lock is held during the refresh");
        assert!(!legacy.exists(), "and released afterwards");
    }

    #[test]
    fn a_refresh_survives_a_transport_failure_without_writing() {
        let dir = temp_dir("claude-oauth-transport");
        let doc = document("at-1", "rt-1", NOW_MS + 60_000);
        seed(&dir, &doc);
        let before = std::fs::read(dir.0.join(CREDENTIALS_FILE)).unwrap();

        // Port 1 on loopback: nothing listens, so the connect fails fast.
        let outcome = refresh_if_expiring(request(&dir, "http://127.0.0.1:1/v1/oauth/token", &[]));

        match &outcome {
            RefreshOutcome::Failed(detail) => {
                assert!(!detail.contains("rt-1"), "{detail}");
            }
            other => panic!("expected a transient failure, got {other:?}"),
        }
        assert_eq!(
            std::fs::read(dir.0.join(CREDENTIALS_FILE)).unwrap(),
            before,
            "a failed POST rewrites nothing"
        );
        assert_locks_released(&dir);
    }

    #[test]
    fn both_locks_are_released_after_every_outcome() {
        // Refreshed, invalid_grant, transient, and not-needed in turn: no
        // outcome may leave a lock behind, or the next refresh deadlocks for a
        // whole stale window.
        struct Case {
            tag: &'static str,
            responses: Vec<(u16, String)>,
            expires_at: i64,
        }
        let cases = vec![
            Case {
                tag: "ok",
                responses: vec![(200, rotated_body())],
                expires_at: NOW_MS + 60_000,
            },
            Case {
                tag: "dead",
                responses: vec![(400, r#"{"error":"invalid_grant"}"#.to_string())],
                expires_at: NOW_MS + 60_000,
            },
            Case {
                tag: "boom",
                responses: vec![(500, r#"{"error":"server_error"}"#.to_string())],
                expires_at: NOW_MS + 60_000,
            },
            Case {
                tag: "fresh",
                responses: vec![],
                expires_at: NOW_MS + 10 * 3_600_000,
            },
        ];
        for Case {
            tag,
            responses,
            expires_at,
        } in cases
        {
            let dir = temp_dir(&format!("claude-oauth-locks-{tag}"));
            seed(&dir, &document("at-1", "rt-1", expires_at));
            let (base, _requests) = canned_server_recording(responses);
            let outcome = refresh_if_expiring(request(&dir, &base, &[]));
            assert!(
                !matches!(outcome, RefreshOutcome::Contended),
                "{tag}: {outcome:?}"
            );
            assert_locks_released(&dir);
        }
    }
}
