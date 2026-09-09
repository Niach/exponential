//! EXP-637 — the on-disk record of the runs this install launched
//! (`<data_dir>/runs.json`), so an ended run can be RESUMED: the row id
//! alone says nothing about which agent ran, where, on what branch, or with
//! which options. EXP-662 widened it from action/chat runs to ISSUE and
//! BATCH sessions, which is what made the cwd-scoped `--continue` machinery
//! redundant — every resume now relaunches an exact recorded transcript.
//!
//! Deliberately its OWN file, not an extension of
//! `coding-session-registry.json`: that one has a byte contract shared with
//! the CLI's crash reconcile (`cli/src/registry.rs` + `ui/src/
//! session_registry.rs` round-trip each other's entries), and adding fields
//! there would be silently dropped by whichever side rewrites first. This
//! file is written and read by the SAME struct on both hosts.
//!
//! gpui-free and dependency-light on purpose — the desktop app and the
//! headless CLI daemon both record into it and both resume out of it.
//!
//! Everything here is best-effort: a corrupt or missing file simply means
//! "no resumable runs", never a failed launch.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;

/// Records past this age are dropped on the next write — a resume that far
/// out would find a pruned worktree and a garbage-collected transcript
/// anyway. EXP-764: ten days — a run older than that is history, not work.
const TTL_SECS: u64 = 10 * 24 * 60 * 60;

/// Serializes every load-modify-save, exactly like the session registry:
/// the automation host records on its own threads while the cleanup path
/// removes concurrently.
static LOCK: Mutex<()> = Mutex::new(());

/// The registry's read-modify-write section, held for one whole operation.
///
/// EXP-766: the process mutex alone was never enough. The desktop app and the
/// CLI daemon share ONE data dir (REV-20), so two hosts could load the same
/// file, each append their own record and each write it back — the second
/// rename silently dropping the first's run. The advisory `flock` on a
/// sibling `runs.json.lock` makes the section machine-wide. It is a separate
/// file on purpose: `runs.json` itself is replaced by rename, so a lock taken
/// on it would guard an unlinked inode.
///
/// Non-unix keeps the in-process mutex only — there is no flock without a new
/// dependency, and the shared-data-dir deployment is unix.
struct RegistryGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    #[cfg(unix)]
    file: Option<std::fs::File>,
}

#[cfg(unix)]
impl Drop for RegistryGuard {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        if let Some(file) = self.file.as_ref() {
            // SAFETY: our own open fd; LOCK_UN cannot fail meaningfully here
            // (closing the file would release it anyway).
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

fn locked(data_dir: &Path) -> RegistryGuard {
    let process = match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    #[cfg(unix)]
    {
        let file = open_lock_file(data_dir);
        RegistryGuard {
            _process: process,
            file,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        RegistryGuard { _process: process }
    }
}

/// Open `runs.json.lock` and take it exclusively, blocking until it is ours.
/// Every failure degrades to the in-process mutex (`None`): a read-only data
/// dir must not make recording impossible.
#[cfg(unix)]
fn open_lock_file(data_dir: &Path) -> Option<std::fs::File> {
    use std::os::unix::io::AsRawFd;
    let _ = std::fs::create_dir_all(data_dir);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data_dir.join("runs.json.lock"))
        .ok()?;
    loop {
        // SAFETY: our own open fd.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Some(file);
        }
        // A signal interrupted the wait — keep waiting; anything else means
        // the filesystem cannot lock (some network mounts), so carry on
        // without it.
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return None;
        }
    }
}

/// Which program the recorded run executed — the resume path re-enters the
/// same one. The first four mirror `launcher::ActionRunKind` without its
/// payloads (a fix-conflicts resume keeps its PR context in [`RunFix`]);
/// `Issue`/`Batch` (EXP-662) are the two SESSION shapes, whose subject rides
/// in [`RunRecord::issue_id`] / [`RunRecord::issues`] instead of an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunKind {
    Team,
    Chat,
    CreateAction,
    FixConflicts,
    Issue,
    Batch,
}

impl RunKind {
    /// Whether this record is an action/chat RUN (as opposed to an issue or
    /// batch session). Runs own their branch and worktree end-to-end; issue
    /// and batch worktrees are the prune's business.
    pub fn is_action(self) -> bool {
        !matches!(self, Self::Issue | Self::Batch)
    }

    /// Whether the recorded run owns the worktree it spawned in — i.e. may
    /// have it auto-removed when it ends ([`crate::run_cleanup`]). A
    /// fix-conflicts run works in the PR branch's shared worktree, and
    /// issue/batch worktrees survive their session by design.
    pub fn owns_run_worktree(self) -> bool {
        matches!(self, Self::Team | Self::Chat)
    }
}

/// One issue a BATCH run covered — enough to name the run and re-render its
/// fallback prompt without a sync store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunIssue {
    pub issue_id: String,
    pub identifier: String,
}

/// The fix-conflicts run's PR context, kept so a resume lands back on the
/// same branch with the same merge target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFix {
    pub branch: String,
    pub default_branch: String,
    pub identifier: String,
    pub issue_id: String,
}

/// One recorded input value (the definition-ordered run form), replayed into
/// a resume's fallback prompt when the native transcript is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInput {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// Everything a resume needs about one finished (or running) run or session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    /// The `coding_sessions` row id — the record's primary key.
    pub session_id: String,
    pub account_id: String,
    pub agent: CodingAgent,
    pub kind: RunKind,
    /// The action row id (or the builtin literal) this run executed; empty
    /// on issue/batch sessions (EXP-662), which have no action.
    #[serde(default)]
    pub action_id: String,
    #[serde(default)]
    pub action_name: String,
    #[serde(default)]
    pub team_id: String,
    /// EXP-662 — the issue this SESSION coded on, and its identifier
    /// (`EXP-42`). Both `None` on every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_identifier: Option<String>,
    /// EXP-662 — the client-minted batch id (`exp/batch-<id8>`'s suffix) and
    /// the issues the batch covered. `None`/empty on every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<RunIssue>,
    /// The spawn cwd — a run worktree, the trunk clone, or a scratch dir.
    pub cwd: PathBuf,
    /// The clone the worktree hangs off; `None` on repo-less scratch runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<PathBuf>,
    /// `owner/name` — resolves the repo for the resume's token mint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The team `repositories` row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// EXP-712: the board this run's branch is based on — replayed on the
    /// resume mint so the reinstated worktree keeps the board's base branch
    /// instead of falling back to the repo's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_id: Option<String>,
    /// The run's own branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// What the run branch was cut from (`origin/<base_branch>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// EXP-443 identity pins — the strongest resume handles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pi_session_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_originator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<RunInput>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effort: String,
    #[serde(default)]
    pub ultracode: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<RunFix>,
    /// `schedule`/`event` when an automation started the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_reason: Option<String>,
    /// The ended session THIS run resumed (a resume of a resume chains).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed_from_id: Option<String>,
    /// EXP-746: which engine ran it. EXP-773 left ONE
    /// ([`crate::launcher::ACP_TRANSPORT`]) and the field stays on the
    /// record so an older reader still parses it. A pre-773 `"pty"` value
    /// (and a missing one) means the run has no ACP session id to reopen —
    /// see [`RunRecord::is_acp`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// EXP-746: the ACP `sessionId` `session/load` takes — upserted by the
    /// engine once `session/new` answers (this file is written at PREPARE
    /// time, before any handshake).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_session_id: Option<String>,
    /// EXP-746: what the ADAPTER reports underneath (claude's stream-json
    /// session id, codex's thread id, pi's session file). The three EXP-443
    /// pins above stay the PTY-side truth; this is the ACP-side one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_native_session_id: Option<String>,
    /// EXP-758: the ACP child's pid and the pid of the host process that
    /// spawned it, written by the engine at spawn and CLEARED by its end
    /// sequence. A record still carrying both while its host is dead names
    /// an orphan (Cmd-Q with a live codex run) for
    /// [`crate::reaper::reap_recorded`] on the next start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acp_child_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_pid: Option<u32>,
    /// EXP-746 (D13): the external ACP agent this run used, when it was not
    /// one of the three builtins. `agent` above then carries the settings
    /// default and means nothing — an older host reading this record still
    /// resumes something sane instead of failing to parse.
    ///
    /// Its `env` is the ONE field that never reaches this file: an external
    /// agent's declared env is where a user puts that agent's TOKEN, and
    /// settings.json is the single place it may live. Copied here it would
    /// stay readable for the 10-day TTL after the user rotated it and deleted
    /// the agent, and a replay would respawn the binary with the stale value.
    /// So the spec is written env-less, a record that predates that rule
    /// loses its env on load (see [`load_registry`]), and
    /// [`RunRecord::resolved_external_agent`] re-attaches the LIVE env by id
    /// wherever a resume or a replay spawns the binary again.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_external_agent"
    )]
    pub external_agent: Option<crate::settings::ExternalAgentSpec>,
    /// Unix seconds — the TTL prune's key.
    pub recorded_at: u64,
    /// Every field of this entry this build does not know — the desktop app
    /// and the CLI daemon share the file and update independently, so a
    /// record a NEWER build widened must survive an older host's
    /// load-modify-save byte-for-byte instead of being re-serialized without
    /// it. Sorted so the rewritten JSON stays stable.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// EXP-746: write [`RunRecord::external_agent`] without its `env`. `serde`
/// hands a field-level `serialize_with` the whole `Option`, and the `None`
/// arm is unreachable behind the field's `skip_serializing_if`.
fn serialize_external_agent<S>(
    spec: &Option<crate::settings::ExternalAgentSpec>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match spec {
        Some(spec) => crate::settings::ExternalAgentSpec {
            env: BTreeMap::new(),
            ..spec.clone()
        }
        .serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// EXP-792: the [`RunRecord::extra`] key carrying the launch's team MCP
/// server picks (`mcp_servers` row ids, pick order) — a resume re-resolves
/// them against the CURRENT secret store, so a rotated token is picked up
/// and a server that lost its credential refuses the resume by name. Rides
/// `extra` rather than a declared field so an older host round-trips it
/// untouched.
pub const MCP_SERVER_IDS_KEY: &str = "mcpServerIds";

impl RunRecord {
    /// EXP-792: the recorded team MCP server ids (empty when none, or when
    /// the entry is not the string array this build writes).
    pub fn mcp_server_ids(&self) -> Vec<String> {
        self.extra
            .get(MCP_SERVER_IDS_KEY)
            .and_then(|value| value.as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str())
                    .filter(|id| !id.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// EXP-792: record the pick. An empty pick REMOVES the key, so a run
    /// without servers serializes exactly as before.
    pub fn set_mcp_server_ids(&mut self, ids: &[String]) {
        self.extra.remove(MCP_SERVER_IDS_KEY);
        self.extra.extend(mcp_server_ids_extra(ids));
    }
}

/// EXP-792: the [`RunRecord::extra`] entries a pick writes — empty for an
/// empty pick. The ONE writer of [`MCP_SERVER_IDS_KEY`]; the launcher seeds a
/// fresh record's `extra` from it.
pub fn mcp_server_ids_extra(ids: &[String]) -> BTreeMap<String, serde_json::Value> {
    let mut extra = BTreeMap::new();
    if !ids.is_empty() {
        extra.insert(
            MCP_SERVER_IDS_KEY.to_string(),
            serde_json::Value::Array(
                ids.iter().map(|id| serde_json::Value::String(id.clone())).collect(),
            ),
        );
    }
    extra
}

/// EXP-792 (EXP-747 B7): the [`RunRecord::extra`] key carrying the run's
/// agent ACCOUNT PROFILE id — a resume must reopen the SAME config dir
/// (credentials, trust flags, codex rollouts). Absent for the ambient
/// login, so a profile-less run serializes exactly as before.
pub const ACCOUNT_KEY: &str = "account";

impl RunRecord {
    /// EXP-792: the recorded account profile; `None` = the ambient login.
    pub fn account(&self) -> Option<String> {
        self.extra
            .get(ACCOUNT_KEY)
            .and_then(|value| value.as_str())
            .filter(|id| !crate::agent_profiles::is_system(Some(id)))
            .map(str::to_string)
    }

    /// EXP-792: record (or, for `None`/`system`, clear) the profile.
    pub fn set_account(&mut self, account: Option<&str>) {
        self.extra.remove(ACCOUNT_KEY);
        self.extra.extend(account_extra(account));
    }
}

/// EXP-792: the `extra` entry an account pick writes — empty for the
/// ambient login.
pub fn account_extra(account: Option<&str>) -> BTreeMap<String, serde_json::Value> {
    let mut extra = BTreeMap::new();
    if !crate::agent_profiles::is_system(account) {
        if let Some(id) = account {
            extra.insert(
                ACCOUNT_KEY.to_string(),
                serde_json::Value::String(id.trim().to_string()),
            );
        }
    }
    extra
}

/// EXP-792: everything a fresh record's `extra` carries off the launch
/// options — the team server pick and the account profile.
pub fn launch_extra(ids: &[String], account: Option<&str>) -> BTreeMap<String, serde_json::Value> {
    let mut extra = mcp_server_ids_extra(ids);
    extra.extend(account_extra(account));
    extra
}

impl RunRecord {

    /// EXP-746: the external agent this run ran under, with its `env` taken
    /// from the CURRENT settings (`configured`) instead of from disk — the
    /// record carries none. `None` for every builtin run.
    ///
    /// An agent the user has since deleted (or whose id they changed) still
    /// resumes on its recorded `command`/`args`, just without a declared env:
    /// a rotated secret is never replayed out of a stale copy, and the run
    /// fails loudly on the agent's own auth instead.
    pub fn resolved_external_agent(
        &self,
        configured: &[crate::settings::ExternalAgentSpec],
    ) -> Option<crate::settings::ExternalAgentSpec> {
        let recorded = self.external_agent.as_ref()?;
        let env = configured
            .iter()
            .find(|entry| entry.id == recorded.id)
            .map(|entry| entry.env.clone())
            .unwrap_or_default();
        Some(crate::settings::ExternalAgentSpec {
            env,
            ..recorded.clone()
        })
    }

    /// EXP-773: whether this run was recorded on the ACP engine, i.e.
    /// whether its `acpSessionId` can be re-entered with `session/load`. A
    /// missing value, the retired `"pty"` and an id this build does not know
    /// all answer `false` — the resume falls back to the agent's OWN handle
    /// ([`Self::agent_native_session_id`]) instead.
    pub fn is_acp(&self) -> bool {
        self.transport.as_deref() == Some(crate::launcher::ACP_TRANSPORT)
    }

    /// Whether the recorded workspace can still be resumed INTO. A
    /// repo-backed run needs its worktree, `.git` link included (a removed
    /// worktree leaves the dir gone, or gutted). EXP-764: a repo-less run's
    /// scratch dir is purged with the whole run the moment it ends
    /// ([`crate::scratch::purge`]), so it is resumable only while that dir
    /// still stands — in practice, while the run is live.
    pub fn resumable(&self) -> bool {
        if self.clone.is_none() {
            return self.cwd.is_dir();
        }
        self.cwd.is_dir() && self.cwd.join(".git").exists()
    }

    /// What this record is CALLED wherever a resume is offered or narrated
    /// (tab titles, the fallback prompt, the ended strip): the issue's
    /// identifier, the batch's `EXP-42 +2` shape, or the action's name.
    pub fn display_name(&self) -> String {
        match self.kind {
            RunKind::Issue => self
                .issue_identifier
                .clone()
                .unwrap_or_else(|| self.action_name.clone()),
            RunKind::Batch => {
                let first = self
                    .issues
                    .first()
                    .map(|issue| issue.identifier.as_str())
                    .unwrap_or("batch");
                format!("{first} +{}", self.issues.len().saturating_sub(1))
            }
            _ => self.action_name.clone(),
        }
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("runs.json")
}

/// The file split into what THIS build understands and what it does not.
/// Parsing is PER ENTRY: the desktop app and the CLI daemon share one data
/// dir and update independently, so a newer build widening [`RunKind`] (or
/// the record shape) writes entries this one cannot deserialize — as a
/// whole-file parse they would take every sibling record down with them, and
/// the next `record()` would rewrite the file with its single entry. Unknown
/// entries are instead carried verbatim in [`Registry::unknown`] through every
/// load-modify-save, so an older host never deletes a newer host's records.
///
/// The other half of that promise is [`RunRecord::extra`]: an entry this build
/// CAN parse but which carries fields it has never heard of keeps them too, so
/// a rewrite is not a silent downgrade of the newer host's record.
#[derive(Default)]
struct Registry {
    records: Vec<RunRecord>,
    unknown: Vec<serde_json::Value>,
}

/// Record keys this build DELETED (never keys it merely predates) — dropped
/// on load so [`RunRecord::extra`]'s forward-compat catch-all does not
/// resurrect them on every rewrite. `skipPermissions` went with EXP-690's
/// toggle; its `#[serde(skip_serializing)]` tombstone went with EXP-693.
const DEAD_KEYS: &[&str] = &["skipPermissions"];

fn load_registry(data_dir: &Path) -> Registry {
    let Ok(raw) = std::fs::read_to_string(registry_path(data_dir)) else {
        return Registry::default();
    };
    let entries: Vec<serde_json::Value> = match serde_json::from_str(&raw) {
        Ok(entries) => entries,
        Err(err) => {
            log::warn!("run registry unreadable ({err}); starting empty");
            return Registry::default();
        }
    };
    let mut registry = Registry::default();
    for entry in entries {
        match serde_json::from_value::<RunRecord>(entry.clone()) {
            Ok(mut record) => {
                // EXP-693: keys this build RETIRED are not keys it never heard
                // of — [`RunRecord::extra`] would otherwise carry a pre-0.15
                // `skipPermissions` through every rewrite forever.
                for dead in DEAD_KEYS {
                    record.extra.remove(*dead);
                }
                // EXP-746: an external agent's env is settings.json's alone.
                // A record written before that rule still carries it, so it
                // is dropped on the way in — nothing downstream can spawn on
                // a stale secret, and the next write purges it from the file.
                if let Some(external) = record.external_agent.as_mut() {
                    external.env.clear();
                }
                registry.records.push(record);
            }
            Err(err) => {
                log::debug!("run registry: keeping an entry this build cannot read ({err})");
                registry.unknown.push(entry);
            }
        }
    }
    registry
}

fn load(data_dir: &Path) -> Vec<RunRecord> {
    load_registry(data_dir).records
}

/// A field off an unknown entry — the only two this build reads out of one
/// (the upsert key and the TTL key); everything else stays opaque.
fn entry_session_id(entry: &serde_json::Value) -> Option<&str> {
    entry.get("sessionId")?.as_str()
}

fn entry_recorded_at(entry: &serde_json::Value) -> Option<u64> {
    entry.get("recordedAt")?.as_u64()
}

/// EXP-757: the third key read off an unknown entry — the scratch sweep's
/// keep set must cover a live run a NEWER build recorded.
fn entry_cwd(entry: &serde_json::Value) -> Option<&str> {
    entry.get("cwd")?.as_str()
}

fn save(data_dir: &Path, registry: &Registry) {
    let path = registry_path(data_dir);
    let mut entries: Vec<serde_json::Value> = Vec::with_capacity(registry.records.len());
    for record in &registry.records {
        let Ok(value) = serde_json::to_value(record) else {
            return;
        };
        entries.push(value);
    }
    entries.extend(registry.unknown.iter().cloned());
    let Ok(json) = serde_json::to_string_pretty(&entries) else {
        return;
    };
    // EXP-766: a UNIQUE temp per write (pid + nanos, inside `write_atomic`).
    // The old fixed `runs.json.tmp` was one file two hosts wrote at once, so
    // one could rename the other's half-written bytes into place.
    let _ = api::atomic_file::write_atomic(&path, &json);
}

/// Upsert by `session_id` (a re-record of the same run replaces it) and
/// drop everything past the TTL in the same pass.
pub fn record(data_dir: &Path, record: RunRecord) {
    let _guard = locked(data_dir);
    let mut registry = load_registry(data_dir);
    let cutoff = now_secs().saturating_sub(TTL_SECS);
    registry
        .records
        .retain(|old| old.session_id != record.session_id && old.recorded_at >= cutoff);
    // Unknown entries take the same upsert and TTL rules where they expose
    // the two keys they ride on, and are kept untouched where they don't.
    registry.unknown.retain(|entry| {
        entry_session_id(entry) != Some(record.session_id.as_str())
            && entry_recorded_at(entry).is_none_or(|at| at >= cutoff)
    });
    registry.records.push(record);
    save(data_dir, &registry);
}

/// Mutate ONE record in place, under a single take of the section.
///
/// EXP-781: the read-modify-write a caller would otherwise spell as
/// [`get`] + [`record`] takes the lock TWICE, and anything can happen in the
/// gap — a concurrent [`remove_many`] (the scratch sweep's purge) drops the
/// record, and the `record` that follows puts it straight back, resurrecting
/// a run whose worktree is already gone.
///
/// `mutate` returns whether it changed anything; `false` writes nothing, and
/// so does a `session_id` with no record. Returns whether the file was
/// rewritten.
///
/// The TTL sweep [`record`] performs deliberately does NOT run here: this is
/// an update of an existing entry, not a new run arriving, and expiring
/// neighbours under a caller that asked to touch one record would make the
/// purge race worse rather than better.
pub fn update(
    data_dir: &Path,
    session_id: &str,
    mutate: impl FnOnce(&mut RunRecord) -> bool,
) -> bool {
    let _guard = locked(data_dir);
    let mut registry = load_registry(data_dir);
    let Some(record) = registry
        .records
        .iter_mut()
        .find(|record| record.session_id == session_id)
    else {
        return false;
    };
    if !mutate(record) {
        return false;
    }
    save(data_dir, &registry);
    true
}

/// EXP-758: every record this build can read, oldest first. The orphan reaper
/// ([`crate::reaper::reap_recorded`]) is the caller — it has to look at ALL of
/// them, not one by one, and unknown entries are none of its business (a
/// newer build's record names pids only that build knows how to judge).
pub fn all(data_dir: &Path) -> Vec<RunRecord> {
    let _guard = locked(data_dir);
    load(data_dir)
}

pub fn get(data_dir: &Path, session_id: &str) -> Option<RunRecord> {
    let _guard = locked(data_dir);
    load(data_dir)
        .into_iter()
        .find(|record| record.session_id == session_id)
}

/// EXP-662 — the newest still-resumable ISSUE record for `issue_id` on this
/// account. The Start-coding dialog's Resume offer and every remote
/// `resume: true` start resolve through here: a hit becomes a
/// `PrepareRequest::ResumeRun`, a miss a fresh launch. `record()` appends the
/// newest last, so a resume-of-a-resume chain resolves to its tail even when
/// two records share a second.
pub fn latest_for_issue(
    data_dir: &Path,
    account_id: &str,
    issue_id: &str,
) -> Option<RunRecord> {
    let _guard = locked(data_dir);
    load(data_dir)
        .into_iter()
        .filter(|record| {
            record.kind == RunKind::Issue
                && record.account_id == account_id
                && record.issue_id.as_deref() == Some(issue_id)
                && record.resumable()
        })
        .max_by_key(|record| record.recorded_at)
}

/// EXP-757: the recorded cwds of `session_ids` — the scratch sweep's keep
/// set. Reads unknown entries too: an older host must never sweep the live
/// scratch dir of a run a newer build recorded.
pub fn cwds_for(data_dir: &Path, session_ids: &[String]) -> Vec<PathBuf> {
    let _guard = locked(data_dir);
    let registry = load_registry(data_dir);
    let wanted = |id: &str| session_ids.iter().any(|wanted| wanted == id);
    let mut cwds: Vec<PathBuf> = registry
        .records
        .iter()
        .filter(|record| wanted(&record.session_id))
        .map(|record| record.cwd.clone())
        .collect();
    cwds.extend(
        registry
            .unknown
            .iter()
            .filter(|entry| entry_session_id(entry).is_some_and(wanted))
            .filter_map(|entry| entry_cwd(entry).map(PathBuf::from)),
    );
    cwds
}

/// EXP-766: the cwds of runs whose HOST process is still alive — the prune's
/// cross-process busy set. `busy_paths` only ever knew this process's tabs,
/// so a worktree driven by the sibling on the shared data dir (the CLI
/// daemon, a second desktop; REV-20) looked idle and could be removed under
/// a running agent. Unknown entries count too: a newer build's live run is
/// still live work.
///
/// A recycled pid can only make this set too BIG, which parks a worktree for
/// one pass — the safe direction, and [`crate::reaper`] clears the pids of
/// dead hosts at every start.
pub fn live_host_cwds(data_dir: &Path) -> Vec<PathBuf> {
    let _guard = locked(data_dir);
    let registry = load_registry(data_dir);
    let mut cwds: Vec<PathBuf> = registry
        .records
        .iter()
        .filter(|record| record.host_pid.is_some_and(crate::process::is_alive))
        .map(|record| record.cwd.clone())
        .collect();
    cwds.extend(
        registry
            .unknown
            .iter()
            .filter(|entry| {
                entry
                    .get("hostPid")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|pid| u32::try_from(pid).is_ok_and(crate::process::is_alive))
            })
            .filter_map(|entry| entry_cwd(entry).map(PathBuf::from)),
    );
    cwds
}

pub fn remove(data_dir: &Path, session_id: &str) {
    remove_many(data_dir, std::slice::from_ref(&session_id.to_string()));
}

/// [`remove`] for a batch of ids in ONE rewrite — the scratch sweep's
/// purge list (EXP-764). Known and unknown entries alike.
pub fn remove_many(data_dir: &Path, session_ids: &[String]) {
    if session_ids.is_empty() {
        return;
    }
    let _guard = locked(data_dir);
    let mut registry = load_registry(data_dir);
    let before = registry.records.len() + registry.unknown.len();
    registry
        .records
        .retain(|record| !session_ids.iter().any(|id| *id == record.session_id));
    registry.unknown.retain(|entry| {
        !entry_session_id(entry).is_some_and(|id| session_ids.iter().any(|wanted| wanted == id))
    });
    if registry.records.len() + registry.unknown.len() != before {
        save(data_dir, &registry);
    }
}

/// Every recorded run branch on `clone` — the prune's nomination list
/// (EXP-637: run worktrees are ours to reclaim, but git still has to confirm
/// the branch landed before anything is removed). EXP-662: ACTION kinds only
/// — issue and batch worktrees stay governed by the prune's own prefix/keep
/// policy, which a nomination would bypass.
pub fn branches_for_clone(data_dir: &Path, clone: &Path) -> Vec<String> {
    let _guard = locked(data_dir);
    let mut branches: Vec<String> = load(data_dir)
        .into_iter()
        .filter(|record| record.clone.as_deref() == Some(clone))
        .filter(|record| record.kind.is_action())
        .filter_map(|record| record.branch)
        .collect();
    branches.sort();
    branches.dedup();
    branches
}

/// A fully populated repo-backed record for tests in this crate (the scratch
/// sweep's tests need real records without restating the struct).
#[cfg(test)]
pub(crate) fn sample_record(session_id: &str) -> RunRecord {
    RunRecord {
            session_id: session_id.to_string(),
            board_id: None,
            account_id: "acc-1".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Team,
            action_id: "act-1".to_string(),
            action_name: "Code review".to_string(),
            team_id: "ws-1".to_string(),
            issue_id: None,
            issue_identifier: None,
            batch_id: None,
            issues: Vec::new(),
            cwd: PathBuf::from("/repos/owner/name.worktrees/code-review-1a2b3c4d"),
            clone: Some(PathBuf::from("/repos/owner/name")),
            repo: Some("owner/name".to_string()),
            repository_id: Some("repo-1".to_string()),
            branch: Some("exp/code-review-1a2b3c4d".to_string()),
            base_branch: Some("main".to_string()),
            claude_session_id: Some("cs-1".to_string()),
            pi_session_file: None,
            codex_originator: None,
            inputs: vec![RunInput {
                key: "scope".to_string(),
                value: "everything".to_string(),
                display: None,
            }],
            model: "fable".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            fix: None,
            started_reason: None,
            resumed_from_id: None,
            recorded_at: now_secs(),
            extra: BTreeMap::new(),
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            external_agent: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-run-registry-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(session_id: &str) -> RunRecord {
        sample_record(session_id)
    }

    #[test]
    fn round_trips_a_record() {
        let dir = temp_dir("round-trip");
        // EXP-781: ONE sample, compared against itself. `sample_record`
        // stamps `recorded_at: now_secs()`, so two calls that straddle a
        // second boundary are not equal — and a loaded workspace run puts
        // exactly that gap between the write and the assertion.
        let written = sample("sess-1");
        record(&dir, written.clone());
        assert_eq!(get(&dir, "sess-1").unwrap(), written);
        assert_eq!(get(&dir, "sess-nope"), None);
        remove(&dir, "sess-1");
        assert_eq!(get(&dir, "sess-1"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-781: `update` is the whole read-modify-write under ONE take of the
    /// section. A record that is gone is not recreated (which `get` + `record`
    /// would do, resurrecting a run a concurrent purge just removed), and a
    /// mutation that changed nothing writes nothing.
    #[test]
    fn update_mutates_in_place_and_never_resurrects() {
        let dir = temp_dir("update");
        record(&dir, sample("sess-1"));

        assert!(update(&dir, "sess-1", |record| {
            record.acp_session_id = Some("acp-42".to_string());
            true
        }));
        assert_eq!(
            get(&dir, "sess-1").unwrap().acp_session_id.as_deref(),
            Some("acp-42")
        );
        // The rest of the record is untouched.
        assert_eq!(get(&dir, "sess-1").unwrap().account_id, "acc-1");

        // A mutation that reports no change is a no-op.
        assert!(!update(&dir, "sess-1", |_| false));

        // The purge race: the record is gone by the time the update runs, and
        // the update must leave it gone.
        remove(&dir, "sess-1");
        assert!(!update(&dir, "sess-1", |record| {
            record.host_pid = Some(1);
            true
        }));
        assert_eq!(get(&dir, "sess-1"), None);

        // A sibling record is never touched by another's update.
        record(&dir, sample("sess-a"));
        let sibling = sample("sess-b");
        record(&dir, sibling.clone());
        assert!(update(&dir, "sess-a", |record| {
            record.host_pid = Some(7);
            true
        }));
        assert_eq!(get(&dir, "sess-b").unwrap(), sibling);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-746: an ACP run round-trips its transport and both session ids,
    /// and a record without them still reads as a terminal run — the four
    /// fields are skip-none, so a PTY record's JSON is byte-unchanged.
    #[test]
    fn an_acp_record_round_trips() {
        let dir = temp_dir("acp-record");
        let mut acp = sample("sess-acp");
        acp.transport = Some("acp".to_string());
        acp.acp_session_id = Some("acp-42".to_string());
        acp.agent_native_session_id = Some("claude-99".to_string());
        acp.external_agent = Some(crate::settings::ExternalAgentSpec {
            id: "acme".to_string(),
            label: "Acme ACP".to_string(),
            command: "acme".to_string(),
            args: vec!["--acp".to_string()],
            ..Default::default()
        });
        record(&dir, acp.clone());
        let loaded = get(&dir, "sess-acp").expect("record");
        assert_eq!(loaded, acp);
        assert!(loaded.is_acp());

        // A pre-773 record carries none of the four keys at all.
        record(&dir, sample("sess-pty"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        let pty = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-pty")
            .expect("the pty record");
        for key in ["transport", "acpSessionId", "agentNativeSessionId", "externalAgent"] {
            assert_eq!(pty.get(key), None, "{key} must not be serialized");
        }
        assert!(!get(&dir, "sess-pty").unwrap().is_acp());
        // An id this build does not know is not the ACP engine either.
        let mut future = sample("sess-future");
        future.transport = Some("quantum".to_string());
        assert!(!future.is_acp());
        // ... and neither is the retired PTY value.
        future.transport = Some("pty".to_string());
        assert!(!future.is_acp());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-746: an external agent's `env` is settings.json's alone — it is
    /// never written here, and a resume re-resolves the LIVE one by id.
    #[test]
    fn an_external_agent_env_never_reaches_the_registry() {
        let dir = temp_dir("external-env");
        let spec = crate::settings::ExternalAgentSpec {
            id: "acme".to_string(),
            label: "Acme ACP".to_string(),
            command: "acme".to_string(),
            args: vec!["--acp".to_string()],
            env: BTreeMap::from([("ACME_TOKEN".to_string(), "sk-live-1".to_string())]),
        };
        let mut run = sample("sess-ext");
        run.external_agent = Some(spec.clone());
        record(&dir, run);

        let raw = std::fs::read_to_string(registry_path(&dir)).unwrap();
        assert!(!raw.contains("sk-live-1"), "the secret was written: {raw}");
        let entries: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        let written = &entries[0]["externalAgent"];
        assert_eq!(written["command"], "acme");
        assert_eq!(written["args"], serde_json::json!(["--acp"]));
        assert_eq!(written["env"], serde_json::json!({}));

        // The command and args are pinned; the env comes back from the live
        // settings entry ...
        let loaded = get(&dir, "sess-ext").expect("record");
        assert!(loaded.external_agent.as_ref().unwrap().env.is_empty());
        assert_eq!(
            loaded.resolved_external_agent(std::slice::from_ref(&spec)),
            Some(spec.clone())
        );
        // ... an agent the user has since deleted resumes env-less rather
        // than on a rotated token, and so does one whose id no longer
        // matches.
        let mut renamed = spec.clone();
        renamed.id = "acme-2".to_string();
        for configured in [Vec::new(), vec![renamed]] {
            let resolved = loaded
                .resolved_external_agent(&configured)
                .expect("the recorded spec");
            assert_eq!(resolved.command, "acme");
            assert!(resolved.env.is_empty());
        }
        // A builtin run resolves to no external agent at all.
        assert_eq!(
            sample("sess-builtin").resolved_external_agent(std::slice::from_ref(&spec)),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A record a build before that rule wrote still carries the secret on
    /// disk: it is dropped on the way in, and a neighbour's write purges it
    /// from the file for good.
    #[test]
    fn a_previously_recorded_external_agent_env_is_purged() {
        let dir = temp_dir("external-env-legacy");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d","recordedAt":{now},
                "transport":"acp","externalAgent":{{"id":"acme","label":"Acme ACP",
                "command":"acme","args":["--acp"],"env":{{"ACME_TOKEN":"sk-live-1"}}}}
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        let loaded = get(&dir, "sess-1").expect("record");
        let external = loaded.external_agent.as_ref().expect("the spec");
        assert_eq!(external.command, "acme");
        assert!(external.env.is_empty(), "the stale secret is dropped");

        record(&dir, sample("sess-2"));
        let raw = std::fs::read_to_string(registry_path(&dir)).unwrap();
        assert!(!raw.contains("ACME_TOKEN"), "{raw}");
        let entries: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        let kept = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-1")
            .expect("the record survives");
        assert_eq!(kept["externalAgent"]["command"], "acme");
        assert_eq!(kept["externalAgent"]["env"], serde_json::json!({}));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-746: the four new fields ride the SAME forward-compat contract as
    /// the rest — an older host that rewrites this file must hand a newer
    /// host's ACP record back untouched.
    #[test]
    fn a_record_written_by_a_newer_host_survives_a_load_modify_save() {
        let dir = temp_dir("acp-forward-compat");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"issue",
                "cwd":"/repos/owner/name.worktrees/exp-EXP-42","recordedAt":{now},
                "transport":"acp","acpSessionId":"acp-42",
                "agentNativeSessionId":"claude-99","acpProtocolVersion":3
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        let loaded = get(&dir, "sess-1").expect("record");
        assert!(loaded.is_acp());
        assert_eq!(loaded.acp_session_id.as_deref(), Some("acp-42"));
        assert_eq!(loaded.agent_native_session_id.as_deref(), Some("claude-99"));
        // A field this build has never heard of rides `extra`, as ever.
        assert_eq!(
            loaded.extra.get("acpProtocolVersion"),
            Some(&serde_json::json!(3))
        );

        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        let kept = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-1")
            .expect("the record survives");
        assert_eq!(kept["transport"], "acp");
        assert_eq!(kept["acpSessionId"], "acp-42");
        assert_eq!(kept["agentNativeSessionId"], "claude-99");
        assert_eq!(kept["acpProtocolVersion"], 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_upserts_by_session_id() {
        let dir = temp_dir("upsert");
        record(&dir, sample("sess-1"));
        let mut updated = sample("sess-1");
        updated.branch = Some("exp/chat-deadbeef".to_string());
        record(&dir, updated);
        assert_eq!(
            get(&dir, "sess-1").unwrap().branch.as_deref(),
            Some("exp/chat-deadbeef")
        );
        assert_eq!(load(&dir).len(), 1, "an upsert must not duplicate");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ancient_records_are_pruned_on_write() {
        let dir = temp_dir("ttl");
        let mut old = sample("sess-old");
        old.recorded_at = now_secs().saturating_sub(TTL_SECS + 60);
        record(&dir, old);
        record(&dir, sample("sess-new"));
        assert_eq!(get(&dir, "sess-old"), None);
        assert!(get(&dir, "sess-new").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn branches_for_clone_are_sorted_and_scoped() {
        let dir = temp_dir("branches");
        record(&dir, sample("sess-1"));
        let mut second = sample("sess-2");
        second.branch = Some("exp/chat-00000001".to_string());
        record(&dir, second);
        let mut foreign = sample("sess-3");
        foreign.clone = Some(PathBuf::from("/repos/other/repo"));
        foreign.branch = Some("exp/elsewhere-1".to_string());
        record(&dir, foreign);
        let mut repo_less = sample("sess-4");
        repo_less.clone = None;
        repo_less.branch = None;
        record(&dir, repo_less);

        assert_eq!(
            branches_for_clone(&dir, Path::new("/repos/owner/name")),
            vec![
                "exp/chat-00000001".to_string(),
                "exp/code-review-1a2b3c4d".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-662: an issue record made from `sample`'s shape — a real worktree
    /// so `resumable()` passes.
    fn issue_sample(dir: &Path, session_id: &str, issue_id: &str) -> RunRecord {
        let worktree = dir.join(format!("wt-{session_id}"));
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere").unwrap();
        RunRecord {
            kind: RunKind::Issue,
            action_id: String::new(),
            action_name: String::new(),
            issue_id: Some(issue_id.to_string()),
            issue_identifier: Some("EXP-42".to_string()),
            cwd: worktree,
            branch: Some("exp/EXP-42".to_string()),
            ..sample(session_id)
        }
    }

    #[test]
    fn latest_for_issue_prefers_the_newest_resumable_record() {
        let dir = temp_dir("latest-for-issue");
        let mut older = issue_sample(&dir, "sess-1", "issue-1");
        older.recorded_at = now_secs().saturating_sub(60);
        record(&dir, older);
        record(&dir, issue_sample(&dir, "sess-2", "issue-1"));
        // Another issue, another account, and an ACTION record on the same
        // account are all invisible to this lookup.
        record(&dir, issue_sample(&dir, "sess-3", "issue-2"));
        let mut foreign = issue_sample(&dir, "sess-4", "issue-1");
        foreign.account_id = "acc-2".to_string();
        record(&dir, foreign);
        record(&dir, sample("sess-5"));

        assert_eq!(
            latest_for_issue(&dir, "acc-1", "issue-1")
                .map(|record| record.session_id)
                .as_deref(),
            Some("sess-2")
        );
        // A record whose worktree is gone is not resumable — it must not
        // shadow the older one that still is.
        let mut gone = issue_sample(&dir, "sess-6", "issue-1");
        gone.cwd = dir.join("vanished");
        record(&dir, gone);
        assert_eq!(
            latest_for_issue(&dir, "acc-1", "issue-1")
                .map(|record| record.session_id)
                .as_deref(),
            Some("sess-2")
        );
        assert_eq!(latest_for_issue(&dir, "acc-1", "issue-nope"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn issue_and_batch_branches_are_not_prune_nominations() {
        // EXP-662: session worktrees stay governed by the prune's own
        // prefix/keep policy — nominating them would hand the prune a
        // worktree the user still expects to find.
        let dir = temp_dir("prune-nominations");
        record(&dir, sample("sess-1"));
        record(&dir, issue_sample(&dir, "sess-2", "issue-1"));
        let mut batch = issue_sample(&dir, "sess-3", "issue-1");
        batch.kind = RunKind::Batch;
        batch.issue_id = None;
        batch.issue_identifier = None;
        batch.batch_id = Some("a1b2c3d4".to_string());
        batch.branch = Some("exp/batch-a1b2c3d4".to_string());
        record(&dir, batch);

        assert_eq!(
            branches_for_clone(&dir, Path::new("/repos/owner/name")),
            vec!["exp/code-review-1a2b3c4d".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_name_names_each_kind() {
        let dir = temp_dir("display-name");
        assert_eq!(sample("sess-1").display_name(), "Code review");
        assert_eq!(issue_sample(&dir, "sess-2", "issue-1").display_name(), "EXP-42");
        let mut batch = sample("sess-3");
        batch.kind = RunKind::Batch;
        batch.issues = vec![
            RunIssue {
                issue_id: "issue-1".to_string(),
                identifier: "EXP-42".to_string(),
            },
            RunIssue {
                issue_id: "issue-2".to_string(),
                identifier: "EXP-43".to_string(),
            },
        ];
        assert_eq!(batch.display_name(), "EXP-42 +1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pre_exp662_action_record_still_parses() {
        // The EXP-637 wire, byte-for-byte: no issue/batch fields at all. A
        // record written by the previous build must keep resuming.
        let json = r#"[{
            "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
            "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
            "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
            "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
            "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
            "claudeSessionId":"cs-1","model":"fable","effort":"high",
            "ultracode":false,"skipPermissions":true,"recordedAt":1
        }]"#;
        let records: Vec<RunRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, RunKind::Team);
        assert_eq!(records[0].issue_id, None);
        assert!(records[0].issues.is_empty());
        assert_eq!(records[0].display_name(), "Code review");
    }

    #[test]
    fn a_retired_skip_permissions_key_neither_breaks_a_load_nor_survives_a_write() {
        // EXP-693 removed the `skip_permissions` tombstone field. Records the
        // pre-0.15 builds wrote still carry the key: it must parse (no
        // `deny_unknown_fields` anywhere on `RunRecord`) and must NOT ride
        // `extra` back out — `extra` is for fields a NEWER build added, never
        // for ones this build deleted.
        let dir = temp_dir("dead-key");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now}
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        let loaded = get(&dir, "sess-1").expect("a pre-0.15 record still parses");
        assert_eq!(loaded.action_name, "Code review");
        assert!(loaded.extra.get("skipPermissions").is_none());

        // And the rewrite drops it for good.
        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert!(entries
            .iter()
            .all(|entry| entry.get("skipPermissions").is_none()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_this_build_cannot_read_survives_a_write() {
        // The desktop app and the CLI daemon share this file and update
        // independently: an entry a NEWER build wrote (here a widened
        // `RunKind`) must cost nothing but itself — its siblings still load,
        // and the next write carries it through verbatim instead of the older
        // host silently wiping every record it could not parse.
        let dir = temp_dir("unknown-entry");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
                "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now}
            }},{{
                "sessionId":"sess-future","accountId":"acc-1","agent":"claude",
                "kind":"someFutureKind","cwd":"/repos/owner/name",
                "somethingNew":{{"deep":[1,2]}},"recordedAt":{now}
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        // The readable record loads; the other one is invisible to every read.
        assert_eq!(load(&dir).len(), 1);
        assert!(get(&dir, "sess-1").is_some());
        assert_eq!(get(&dir, "sess-future"), None);

        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 3, "the write kept both siblings");
        let future = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-future")
            .expect("the unreadable entry survives a neighbour's write");
        assert_eq!(future["kind"], "someFutureKind");
        assert_eq!(future["somethingNew"]["deep"], serde_json::json!([1, 2]));
        assert_eq!(load(&dir).len(), 2);

        // It is still addressable by session id, so a removal reaches it.
        remove(&dir, "sess-future");
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_known_entry_keeps_the_fields_this_build_never_heard_of() {
        // The half [`RunRecord::extra`] covers: a NEWER build's record whose
        // `kind` this one already knows parses fine, so the lenient per-entry
        // load cannot save it — without the flattened catch-all the next
        // `record()` would rewrite it stripped of `worktreeMode`, silently
        // downgrading the other host's record.
        let dir = temp_dir("unknown-field");
        let now = now_secs();
        let json = format!(
            r#"[{{
                "sessionId":"sess-1","accountId":"acc-1","agent":"claude","kind":"team",
                "actionId":"act-1","actionName":"Code review","teamId":"ws-1",
                "cwd":"/repos/owner/name.worktrees/code-review-1a2b3c4d",
                "clone":"/repos/owner/name","repo":"owner/name","repositoryId":"repo-1",
                "branch":"exp/code-review-1a2b3c4d","baseBranch":"main",
                "claudeSessionId":"cs-1","model":"fable","effort":"high",
                "ultracode":false,"skipPermissions":true,"recordedAt":{now},
                "worktreeMode":"scratch"
            }}]"#
        );
        std::fs::write(registry_path(&dir), json).unwrap();

        // It loads as an ordinary record, unknown field and all.
        let loaded = get(&dir, "sess-1").expect("a known kind still parses");
        assert_eq!(loaded.action_name, "Code review");
        assert_eq!(
            loaded.extra.get("worktreeMode"),
            Some(&serde_json::json!("scratch"))
        );

        // ... and a neighbour's write carries it back out verbatim.
        record(&dir, sample("sess-2"));
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        assert_eq!(entries.len(), 2);
        let kept = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-1")
            .expect("the record survives");
        assert_eq!(kept["worktreeMode"], "scratch");
        assert_eq!(kept["kind"], "team");
        // A record with nothing extra serializes exactly as before — no
        // empty object, no stray key.
        let plain = entries
            .iter()
            .find(|entry| entry["sessionId"] == "sess-2")
            .expect("the neighbour");
        assert_eq!(plain.get("extra"), None);
        assert_eq!(plain.get("worktreeMode"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-792 (EXP-747 B7): the account profile rides `extra["account"]`;
    /// the ambient login writes nothing.
    #[test]
    fn account_round_trips_through_extra_and_system_writes_nothing() {
        let dir = temp_dir("account-extra");
        let mut picked = sample("sess-1");
        picked.set_account(Some("0a1b2c3d"));
        let mut plain = sample("sess-2");
        plain.set_account(Some("system"));
        record(&dir, picked.clone());
        record(&dir, plain.clone());
        assert_eq!(get(&dir, "sess-1").unwrap().account().as_deref(), Some("0a1b2c3d"));
        assert_eq!(get(&dir, "sess-2").unwrap().account(), None);
        assert!(!get(&dir, "sess-2").unwrap().extra.contains_key(ACCOUNT_KEY));
        let extra = launch_extra(&["srv-1".to_string()], Some("0a1b2c3d"));
        assert_eq!(extra.len(), 2);
        assert!(launch_extra(&[], None).is_empty());
        assert!(launch_extra(&[], Some("system")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-792: the MCP server pick rides `extra["mcpServerIds"]` through a
    /// write+load, an empty pick leaves no key, and a foreign shape reads as
    /// none instead of failing the record.
    #[test]
    fn mcp_server_ids_round_trip_through_extra() {
        let dir = temp_dir("mcp-ids");
        let mut picked = sample("sess-1");
        picked.set_mcp_server_ids(&["srv-1".to_string(), "srv-2".to_string()]);
        let mut plain = sample("sess-2");
        plain.set_mcp_server_ids(&[]);
        record(&dir, picked);
        record(&dir, plain);

        let loaded = get(&dir, "sess-1").expect("recorded");
        assert_eq!(loaded.mcp_server_ids(), vec!["srv-1", "srv-2"]);
        assert_eq!(get(&dir, "sess-2").unwrap().mcp_server_ids(), Vec::<String>::new());
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(registry_path(&dir)).unwrap()).unwrap();
        let on_disk = entries.iter().find(|entry| entry["sessionId"] == "sess-1").unwrap();
        assert_eq!(on_disk[MCP_SERVER_IDS_KEY], serde_json::json!(["srv-1", "srv-2"]));
        let bare = entries.iter().find(|entry| entry["sessionId"] == "sess-2").unwrap();
        assert!(bare.get(MCP_SERVER_IDS_KEY).is_none());

        // Clearing the pick removes the key again.
        let mut cleared = loaded.clone();
        cleared.set_mcp_server_ids(&[]);
        assert!(cleared.extra.get(MCP_SERVER_IDS_KEY).is_none());
        // A shape this build never wrote (an object) is not a pick.
        let mut foreign = sample("sess-3");
        foreign.extra.insert(MCP_SERVER_IDS_KEY.to_string(), serde_json::json!({ "a": 1 }));
        assert!(foreign.mcp_server_ids().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_file_reads_as_empty() {
        let dir = temp_dir("corrupt");
        std::fs::write(registry_path(&dir), "{not json").unwrap();
        assert!(load(&dir).is_empty());
        assert_eq!(get(&dir, "sess-1"), None);
        // ... and a write heals it.
        record(&dir, sample("sess-1"));
        assert!(get(&dir, "sess-1").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resumable_needs_the_workspace_to_still_exist() {
        let dir = temp_dir("resumable");
        let mut record = sample("sess-1");
        record.cwd = dir.join("gone");
        record.clone = Some(dir.clone());
        assert!(!record.resumable(), "a removed worktree is not resumable");

        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        record.cwd = worktree.clone();
        assert!(!record.resumable(), "a gutted worktree has no .git");
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere").unwrap();
        assert!(record.resumable());

        // EXP-764: a repo-less scratch run is resumable only while its
        // directory stands — the purge takes it with the run.
        record.clone = None;
        record.cwd = dir.join("scratch-gone");
        assert!(!record.resumable(), "a purged scratch dir is not resumable");
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        record.cwd = scratch;
        assert!(record.resumable(), "a standing scratch dir (a live run) is");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cwds_for_reads_known_and_unknown_entries() {
        let dir = temp_dir("cwds-for");
        let mut known = sample("sess-known");
        known.cwd = PathBuf::from("/data/actions/act-1/aaaaaaaa");
        record(&dir, known);
        record(&dir, sample("sess-other"));
        // An entry this build cannot parse (a newer kind) still names its cwd.
        let mut entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(dir.join("runs.json")).unwrap())
                .unwrap();
        entries.push(serde_json::json!({
            "sessionId": "sess-future",
            "kind": "hologram",
            "cwd": "/data/actions/act-2/bbbbbbbb",
            "recordedAt": now_secs()
        }));
        std::fs::write(dir.join("runs.json"), serde_json::to_string(&entries).unwrap()).unwrap();

        let mut cwds = cwds_for(
            &dir,
            &[
                "sess-known".to_string(),
                "sess-future".to_string(),
                "sess-missing".to_string(),
            ],
        );
        cwds.sort();
        assert_eq!(
            cwds,
            vec![
                PathBuf::from("/data/actions/act-1/aaaaaaaa"),
                PathBuf::from("/data/actions/act-2/bbbbbbbb"),
            ]
        );
        assert!(cwds_for(&dir, &[]).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-766: only records whose host process is alive, unknown entries
    /// included (a newer build's live run is still live work).
    #[test]
    fn live_host_cwds_reads_known_and_unknown_entries() {
        let dir = temp_dir("live-hosts");
        let mut live = sample("sess-live");
        live.cwd = PathBuf::from("/repos/acme/web.worktrees/live");
        live.host_pid = Some(std::process::id());
        record(&dir, live);
        let mut dead = sample("sess-dead");
        dead.cwd = PathBuf::from("/repos/acme/web.worktrees/dead");
        dead.host_pid = Some(u32::MAX - 1);
        record(&dir, dead);
        // No pid at all (a pre-EXP-758 record) never parks anything.
        record(&dir, sample("sess-pidless"));

        let mut entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(dir.join("runs.json")).unwrap())
                .unwrap();
        entries.push(serde_json::json!({
            "sessionId": "sess-future",
            "kind": "hologram",
            "cwd": "/repos/acme/web.worktrees/future",
            "hostPid": std::process::id(),
            "recordedAt": now_secs()
        }));
        std::fs::write(dir.join("runs.json"), serde_json::to_string(&entries).unwrap()).unwrap();

        let mut cwds = live_host_cwds(&dir);
        cwds.sort();
        assert_eq!(
            cwds,
            vec![
                PathBuf::from("/repos/acme/web.worktrees/future"),
                PathBuf::from("/repos/acme/web.worktrees/live"),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-766: concurrent recorders never drop each other's runs. Threads
    /// first (the automation host records while a cleanup removes) …
    #[test]
    fn concurrent_threads_lose_no_record() {
        let dir = temp_dir("concurrent-threads");
        let writers: Vec<_> = (0..6)
            .map(|writer| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    for index in 0..8 {
                        record(&dir, sample(&format!("sess-{writer}-{index}")));
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let ids: Vec<String> = all(&dir).into_iter().map(|r| r.session_id).collect();
        assert_eq!(ids.len(), 48, "{ids:?}");
        for writer in 0..6 {
            for index in 0..8 {
                let wanted = format!("sess-{writer}-{index}");
                assert!(ids.contains(&wanted), "{wanted} was dropped");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// … and PROCESSES: the desktop app and the CLI daemon share one data dir
    /// (REV-20), which the in-process mutex could never cover. The child is
    /// this same test binary, re-invoked on the ignored writer below.
    #[cfg(unix)]
    #[test]
    fn a_second_process_loses_no_record() {
        let dir = temp_dir("concurrent-procs");
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let Ok(mut child) = std::process::Command::new(exe)
            .args([
                "--exact",
                "--ignored",
                "--nocapture",
                "run_registry::tests::cross_process_child_writer",
            ])
            .env("EXP_RUN_REGISTRY_CHILD_DIR", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return; // no way to fork a helper here; the thread test stands
        };
        for index in 0..30 {
            record(&dir, sample(&format!("parent-{index}")));
        }
        assert!(child.wait().unwrap().success(), "the child writer failed");

        let ids: Vec<String> = all(&dir).into_iter().map(|r| r.session_id).collect();
        for index in 0..30 {
            assert!(ids.contains(&format!("parent-{index}")), "parent record lost");
            assert!(ids.contains(&format!("child-{index}")), "child record lost");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The child half of [`a_second_process_loses_no_record`] — ignored, so
    /// it only runs when that test names it explicitly.
    #[test]
    #[ignore]
    fn cross_process_child_writer() {
        let Ok(dir) = std::env::var("EXP_RUN_REGISTRY_CHILD_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        for index in 0..30 {
            record(&dir, sample(&format!("child-{index}")));
        }
    }
}
