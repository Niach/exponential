//! EXP-484 — the on-disk agent-usage cache (`<data_dir>/agent-usage.json`)
//! and the poll policy that decides when a window is worth re-reading.
//!
//! Two reasons this is a FILE and not process state:
//!
//! * The desktop IDE and the headless `exponential` daemon can run on the
//!   same machine against the same account. They share one token budget
//!   (the usage endpoint tolerates ~20 requests/hour), so they must share
//!   one "already fetched" fact — [`SHARED_TTL_SECS`].
//! * A restart must not cost a request, and must not lose the numbers the
//!   last run read (they stay, marked stale, until a fetch replaces them).
//!
//! Same idioms as [`crate::run_registry`]: a static mutex around every
//! load-modify-save, per-entry tolerant parsing (an entry a NEWER build
//! wrote survives an older host's rewrite verbatim), and tmp+rename writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_accounts::AgentAccount;
use crate::agent_profiles::SYSTEM_PROFILE;
use crate::agent_usage::{AgentUsage, UsageWindow};

/// The floor between two fetches for the SAME agent, shared across every
/// process on this machine: a second binary that finds numbers this fresh
/// simply reports them instead of spending a request of its own.
pub const SHARED_TTL_SECS: u64 = 180;

/// A fetch that produced NEW numbers earns the fastest cadence.
pub const MIN_POLL_SECS: u64 = 180;

/// Nothing changed: back off 100 s per unchanged run, between these bounds.
pub const UNCHANGED_BASE_SECS: u64 = 300;
pub const UNCHANGED_STEP_SECS: u64 = 100;
pub const UNCHANGED_MAX_SECS: u64 = 600;

/// A 429 must never be retried faster than this (the endpoint's own budget).
pub const RATE_LIMITED_FLOOR_SECS: u64 = 300;

/// A 401 means the credential no longer answers for usage — the fix is a
/// re-login, not a retry.
pub const UNAUTHORIZED_BACKOFF_SECS: u64 = 600;

/// A transport failure retries on the ordinary slow cadence.
pub const FAILED_BACKOFF_SECS: u64 = 300;

/// When EVERY window sits at 100 % nothing can move before the earliest
/// reset; poll just after it. EXP-817: one maxed window used to pin the
/// whole agent — a maxed per-model window froze the session and weekly
/// numbers for days on two machines.
pub const RESET_MARGIN_SECS: u64 = 60;

/// A refused/timed-out credential read (the macOS Keychain ACL prompt on a
/// headless daemon) stops the asking for an hour.
pub const CREDENTIAL_DENIED_BACKOFF_SECS: u64 = 3600;

/// EXP-849 — how often ONE codex login's keep-alive runs (`account/read`
/// with `refreshToken: true`).
///
/// Deliberately far above every poll floor: the keep-alive is a flag on a
/// request the poll already makes, so the cadence costs nothing but it does
/// spend a token ROTATION, and rotating on every poll would churn the
/// credential store several times an hour for no gain. Six hours is well
/// inside any refresh-token lifetime while staying ~1/40th of the poll rate.
pub const CODEX_REFRESH_INTERVAL_SECS: u64 = 6 * 3600;

/// EXP-852 — how long before expiry claude's keep-alive rotates the token.
/// Three times the CLI's own 5-minute predicate (`Date.now()+300000 >= expiresAt`)
/// so WE get there first and a CLI start inside the window finds `not_needed`
/// instead of contending the lock. Wide enough for one device-sync beat
/// (~30s) plus the lock backoff (≤10s) plus the POST (≤30s) plus one failed
/// attempt's [`REFRESH_FAILED_BACKOFF_SECS`]. NOT tied to the poll floors:
/// the refresh is looked at on every beat, not on a usage probe (a secondary
/// profile's probe slot can be 20+ minutes apart). ~3 rotations/day on an
/// ~8h access token.
pub const CLAUDE_REFRESH_MARGIN_SECS: u64 = 15 * 60;

/// A refresh the network (not the account) lost: retry on this cadence.
/// Ten minutes is long enough that a flapping link cannot turn the keep-alive
/// into a retry storm against the token endpoint, and short enough that a
/// laptop coming back from a tunnel still rotates well inside
/// [`CLAUDE_REFRESH_MARGIN_SECS`] — the margin budgets one of these.
pub const REFRESH_FAILED_BACKOFF_SECS: u64 = 600;

/// A store with no refresh token at all will not grow one without a relogin.
/// Nothing this process does can change that answer, so asking again on the
/// ordinary cadence would read the keychain (and on macOS risk an ACL prompt)
/// every ten minutes for nothing. The hour is a re-check, not a retry: a
/// relogin in another process rewrites the store, and this is when we notice.
pub const REFRESH_UNSUPPORTED_BACKOFF_SECS: u64 = 3600;

/// How many dead-grant markers one login remembers. A grant goes dead once,
/// and a rotation replaces the token, so the list only ever needs to cover the
/// handful of tokens in flight around a failure (the live one, the one the CLI
/// rotated past us, and the one we just buried). Capped so a pathological loop
/// cannot grow `agent-usage.json` without bound.
pub const MAX_DEAD_REFRESH_TOKENS: usize = 4;

/// EXP-792 (EXP-747 B3): entries are keyed `agent:profileId` — one poll
/// policy per LOGIN, so a 429 on one profile never backs off its siblings.
/// A pre-profile file's bare `agent` key is the ambient login's
/// (`agent:system`) and migrates on load.
pub fn entry_key(agent: &str, profile: &str) -> String {
    format!("{agent}:{profile}")
}

/// Serializes every load-modify-save (the IDE beat and a device-worker task
/// can both land here).
static LOCK: Mutex<()> = Mutex::new(());

fn locked() -> std::sync::MutexGuard<'static, ()> {
    match LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// One agent's cached usage plus everything the poll policy keys on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentCacheEntry {
    /// The last numbers read — kept (and flagged `stale`) through every
    /// failure, so a 401 dims the bar instead of blanking it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgentUsage>,
    /// The identity the probe named, for the agents whose sign-in check
    /// cannot ([`crate::doctor`] can only see that codex IS signed in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AgentAccount>,
    /// Unix seconds of the last ATTEMPT (successful or not) — the shared
    /// TTL's key, so a failing host does not out-poll a healthy one.
    pub fetched_at_secs: u64,
    pub next_poll_at_secs: u64,
    pub unchanged_streak: u32,
    pub last_windows_hash: String,
    /// The soonest reset when EVERY window sits at 100 % — nothing can
    /// change before it. `None` while any window still has room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_reset_secs: Option<u64>,
    /// Set when the credential STORE refused; no read is attempted before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_denied_until_secs: Option<u64>,
    /// EXP-792: set on a 429 — the one floor a FORCED refresh
    /// (`agent_usage_refresh`) must still honor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limited_until_secs: Option<u64>,
    /// EXP-849 — this login's HEALTH, as the
    /// [`crate::agent_accounts::Health`] wire token, derived from the PROBE:
    /// a 401/403 is `needs_relogin`, an answer is `ok`. `None` = never
    /// probed, which the wire reports as `unknown`.
    ///
    /// Deliberately NOT touched by a transport failure: an offline laptop's
    /// login is not broken, and a dimmed bar already says the numbers are
    /// old. Only the two outcomes that are the credential's own answer move
    /// it, so the badge never flickers on a flaky network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    /// EXP-849/EXP-852 — when this login's keep-alive last ran. Unix seconds;
    /// `None` = never. BOTH agents stamp it: codex's `account/read` with
    /// `refreshToken: true`, and claude's own `grant_type=refresh_token` POST.
    /// For codex it IS the cadence ([`refresh_due`]); for claude it is
    /// diagnostic only — that cadence is expiry-driven off
    /// [`Self::claude_expires_at_ms`] ([`claude_refresh_due`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at_secs: Option<u64>,
    /// EXP-852 — the `expiresAt` the last credential read for this login saw
    /// (unix MILLIseconds, the field's own unit). The claude keep-alive's cheap
    /// gate: it makes the cadence EXPIRY-driven rather than a timer, without a
    /// keychain read on every beat. `None` = never read / no expiry in the
    /// document, which the gate treats as "look now" (a read that finds no
    /// expiry pairs it with [`REFRESH_UNSUPPORTED_BACKOFF_SECS`], so "now" is
    /// once an hour, not every beat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_expires_at_ms: Option<i64>,
    /// EXP-852 — a refresh that failed for a reason that is NOT the account's
    /// answer (transport, 5xx, a store we could not write, no refresh token at
    /// all). Health is deliberately untouched by those; this is the only backoff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_backoff_until_secs: Option<u64>,
    /// EXP-852 — `sha256(refresh_token)[..16]` of every grant the token endpoint
    /// answered `invalid_grant` for on this login. The CLI's own dead-token set
    /// is in-memory; ours survives a restart, so a dead grant is never re-spent.
    /// Newest first, capped at [`MAX_DEAD_REFRESH_TOKENS`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dead_refresh_tokens: Vec<String>,
    /// When the ENDPOINT is next owed a poll while a PARTIAL live publisher
    /// (claude's `rate_limit_event`) answers for this login. Unix seconds;
    /// `None` = owed now. Live applies stamp `fetched_at_secs` and
    /// `next_poll_at_secs` on every frame, so they cannot schedule this —
    /// without it the windows no frame carries (the model-scoped weekly)
    /// froze for the whole run. See [`live_endpoint_due`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_due_at_secs: Option<u64>,
    /// EXP-881 — when the ENDPOINT last produced a report for this login.
    /// Unix seconds; `None` = never (or an older host's row).
    ///
    /// Distinct from [`Self::fetched_at_secs`], which a LIVE apply moves too:
    /// this one only ever moves when a real fetch answered. It is what lets
    /// [`crate::agent_usage::live_probe`] tell a live frame that is NEWER than
    /// the last report (lay it over) from one that is OLDER (the report
    /// already contains it — re-laying it would drag the numbers backwards).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_fetched_at_secs: Option<u64>,
    /// Fields a newer build wrote that this one does not know — carried
    /// verbatim through every rewrite (the [`crate::run_registry`] promise).
    ///
    /// EXP-852: this flatten map EXISTS (confirmed against the struct above),
    /// which is exactly why the three keep-alive fields need no wire work of
    /// their own — a host that predates them parks them here and hands them
    /// back on its next rewrite, so a downgrade never re-spends a dead grant
    /// or forgets a backoff. None of them reach the heartbeat wire either:
    /// only `usage`/`account`/`health` are published. Locked by
    /// `a_newer_hosts_keep_alive_fields_survive_an_older_rewrite`.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Why the last fetch mattered — the input to [`next_poll_at`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// New numbers.
    Changed,
    /// The same numbers as last time.
    Unchanged,
    /// HTTP 429.
    RateLimited,
    /// HTTP 401/403.
    Unauthorized,
    /// Transport error, unparseable body, missing/expired credential, a
    /// refused app-server probe.
    Failed,
}

/// The whole file: entries this build understands plus the ones it does not
/// (kept so an older host never deletes a newer host's rows).
#[derive(Default, Debug)]
pub struct UsageCache {
    entries: BTreeMap<String, AgentCacheEntry>,
    unknown: BTreeMap<String, Value>,
}

impl UsageCache {
    pub fn get(&self, agent: &str) -> Option<&AgentCacheEntry> {
        self.entries.get(agent)
    }

    pub fn insert(&mut self, agent: String, entry: AgentCacheEntry) {
        self.unknown.remove(&agent);
        self.entries.insert(agent, entry);
    }
}

fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("agent-usage.json")
}

/// Read the cache, tolerating a missing/corrupt file (→ empty) and entries
/// this build cannot parse (→ carried verbatim).
pub fn load(data_dir: &Path) -> UsageCache {
    let _guard = locked();
    load_unlocked(data_dir)
}

fn load_unlocked(data_dir: &Path) -> UsageCache {
    let Ok(raw) = std::fs::read_to_string(cache_path(data_dir)) else {
        return UsageCache::default();
    };
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&raw) else {
        return UsageCache::default();
    };
    let mut cache = UsageCache::default();
    let mut legacy: Vec<(String, AgentCacheEntry)> = Vec::new();
    for (key, value) in object {
        match serde_json::from_value::<AgentCacheEntry>(value.clone()) {
            // EXP-792: a bare `claude` row is the ambient login's; it moves
            // under `claude:system` unless a row already sits there (the
            // row a profile-aware host wrote is the fresher one).
            Ok(entry) if !key.contains(':') => legacy.push((entry_key(&key, SYSTEM_PROFILE), entry)),
            Ok(entry) => {
                cache.entries.insert(key, entry);
            }
            Err(_) => {
                cache.unknown.insert(key, value);
            }
        }
    }
    for (key, entry) in legacy {
        cache.entries.entry(key).or_insert(entry);
    }
    cache
}

/// Persist the cache (tmp + rename). Best-effort: a failed write only means
/// the next run re-polls.
pub fn save(data_dir: &Path, cache: &UsageCache) {
    let _guard = locked();
    let mut object = serde_json::Map::new();
    for (agent, entry) in &cache.entries {
        let Ok(value) = serde_json::to_value(entry) else {
            return;
        };
        object.insert(agent.clone(), value);
    }
    for (agent, value) in &cache.unknown {
        object.entry(agent.clone()).or_insert_with(|| value.clone());
    }
    let Ok(json) = serde_json::to_string_pretty(&Value::Object(object)) else {
        return;
    };
    let path = cache_path(data_dir);
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::create_dir_all(data_dir);
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// A login just ended on this machine for `agent`: drop its cached identity
/// and numbers so the next beat polls afresh and names the NEW account,
/// instead of the old email riding out its backoff (up to 10 min).
pub fn forget(data_dir: &Path, agent: &str) {
    forget_profile(data_dir, agent, SYSTEM_PROFILE);
}

/// EXP-792: [`forget`] for ONE account profile of `agent` — a login just
/// ended in that profile's dir; its siblings keep their numbers.
pub fn forget_profile(data_dir: &Path, agent: &str, profile: &str) {
    let key = entry_key(agent, profile);
    let mut cache = load(data_dir);
    let mut removed = cache.entries.remove(&key).is_some() | cache.unknown.remove(&key).is_some();
    if profile == SYSTEM_PROFILE {
        // The pre-profile key, should an older host have written it since.
        removed |= cache.unknown.remove(agent).is_some();
    }
    if removed {
        save(data_dir, &cache);
    }
}

/// Whether this agent may be polled right now: past its scheduled next poll,
/// past the machine-wide shared TTL, and not inside a credential-refusal
/// backoff.
///
/// EXP-881: this answers "may we SPEND a request", nothing else. Whether the
/// numbers we already hold still answer is
/// [`crate::agent_usage::live_probe`]'s question, and the two are independent:
/// a login can be due a poll and still have a current live frame, and a login
/// that is not due can hold a frame that has gone stale.
pub fn poll_due(entry: &AgentCacheEntry, now: u64) -> bool {
    if entry
        .credential_denied_until_secs
        .is_some_and(|until| now < until)
    {
        return false;
    }
    now >= entry.next_poll_at_secs
        && now.saturating_sub(entry.fetched_at_secs) >= SHARED_TTL_SECS
}

/// While a live session reports only SOME windows, how often the endpoint is
/// still read for the rest. Slower than [`MIN_POLL_SECS`]: the frames already
/// keep the session and weekly numbers moving.
///
/// EXP-881: it is ALSO the currency window of a live frame published by an
/// ATTACHED session. A live run refreshes its own numbers every turn, so a
/// frame younger than the endpoint's own cadence is worth at least as much as
/// a fetch; an older one is not, attached or not (an idle run publishes
/// nothing, and "a session is open" is not "the numbers are current").
pub const LIVE_ENDPOINT_POLL_SECS: u64 = 600;

/// Whether a login a PARTIAL live publisher answers for is owed an endpoint
/// poll for the windows the frames never carry. `endpoint_due_at_secs` is
/// stamped by [`schedule_live_endpoint`] alone, never by a live apply — see
/// [`AgentCacheEntry::endpoint_due_at_secs`].
pub fn live_endpoint_due(entry: &AgentCacheEntry, now: u64) -> bool {
    entry.endpoint_due_at_secs.is_none_or(|due| now >= due)
}

/// After an endpoint attempt: the next owed one, never before this cadence nor
/// before any backoff the attempt earned (a 401, a refused keychain, a 429,
/// every window maxed) — the live applies that follow reset those fields, this
/// stamp keeps them.
pub fn schedule_live_endpoint(entry: &mut AgentCacheEntry, now: u64) {
    let due = [
        Some(now + LIVE_ENDPOINT_POLL_SECS),
        Some(entry.next_poll_at_secs),
        entry.credential_denied_until_secs,
        entry.rate_limited_until_secs,
    ]
    .into_iter()
    .flatten()
    .max();
    entry.endpoint_due_at_secs = due;
}

/// EXP-849 — is this login's codex keep-alive due? (Never a reason to poll on
/// its own: it only ever rides a probe the poll policy already decided to
/// make.)
pub fn refresh_due(entry: &AgentCacheEntry, now: u64) -> bool {
    entry
        .refreshed_at_secs
        .is_none_or(|at| now.saturating_sub(at) >= CODEX_REFRESH_INTERVAL_SECS)
}

/// EXP-852 — is claude's keep-alive due for this login? Not inside a credential
/// denial, not inside a refresh backoff, and the cached expiry is unknown
/// (look once) or within [`CLAUDE_REFRESH_MARGIN_SECS`] — an already-expired
/// token is trivially inside the margin and IS refreshed (the CLI's predicate
/// is true for any past instant).
///
/// Unlike [`refresh_due`] this is NOT a rider on a usage probe: it is asked on
/// every beat, because the thing it protects is the token's expiry, not a
/// request budget. The two backoffs above are the whole rate limit.
pub fn claude_refresh_due(entry: &AgentCacheEntry, now: u64) -> bool {
    if entry
        .credential_denied_until_secs
        .is_some_and(|until| now < until)
    {
        return false;
    }
    if entry
        .refresh_backoff_until_secs
        .is_some_and(|until| now < until)
    {
        return false;
    }
    match entry.claude_expires_at_ms {
        // Never read, or a document with no expiry in it: look now. The read
        // itself stamps the answer that schedules every later beat, and a
        // read that finds no expiry sets the hour backoff above (the store
        // cannot be scheduled off a field it lacks, and `refresh_if_expiring`
        // never POSTs on a guess), so this arm never means "every beat".
        None => true,
        // Milliseconds on both sides. `saturating_mul` keeps a hand-edited or
        // absurd clock from wrapping the comparison into "not due".
        Some(at) => ((now + CLAUDE_REFRESH_MARGIN_SECS) as i64).saturating_mul(1000) >= at,
    }
}

/// Stamp what the last credential read saw (every read, setting on or off).
///
/// Recorded even when the keep-alive is disabled: the field is the CADENCE's
/// input, so a user switching the setting on must not owe a blind read first,
/// and a read that found no expiry writes `None` back ("look again"), which
/// the keep-alive pairs with [`note_refresh_failed`] so "again" is bounded.
pub fn note_credential_expiry(entry: &mut AgentCacheEntry, expires_at_ms: Option<i64>) {
    entry.claude_expires_at_ms = expires_at_ms;
}

/// A rotation landed: new expiry, backoff cleared, `refreshed_at_secs = now`.
///
/// Health is deliberately NOT touched here — it is the usage probe's fact
/// (EXP-849), and a rotation that works while the probe 401s says something
/// about the endpoint, not the account.
pub fn note_refresh_ok(entry: &mut AgentCacheEntry, expires_at_ms: i64, now: u64) {
    entry.claude_expires_at_ms = Some(expires_at_ms);
    entry.refresh_backoff_until_secs = None;
    entry.refreshed_at_secs = Some(now);
}

/// EXP-881 — is this login's claude token ALREADY expired (not merely inside
/// the refresh margin [`claude_refresh_due`] answers for)?
///
/// The distinction is the whole point: an expiring token is a keep-alive's
/// business and may be left alone when the user turned the keep-alive off, but
/// an EXPIRED one makes every usage read 401 and paints `Needs re-login` on a
/// perfectly good account. So the collector refreshes an expired one whatever
/// the setting says. `None` (never read, or a document with no expiry) is NOT
/// expired: we have no evidence, and guessing would mean a POST on every beat.
pub fn claude_token_expired(entry: &AgentCacheEntry, now: u64) -> bool {
    match entry.claude_expires_at_ms {
        None => false,
        Some(at) => (now as i64).saturating_mul(1000) >= at,
    }
}

/// EXP-881 — a rotation landed on a login whose numbers are dimmed: release
/// the FAILED backoff so the probe can run on THIS beat rather than waiting
/// out a wall that the new token has just taken down.
///
/// Only the failure backoff moves. The 429 floor
/// ([`AgentCacheEntry::rate_limited_until_secs`]) is the SERVER's instruction
/// and survives any rotation of ours; inside it the schedule is left exactly
/// where it stood.
pub fn note_token_rotated(entry: &mut AgentCacheEntry, now: u64) {
    if entry
        .rate_limited_until_secs
        .is_some_and(|until| now < until)
    {
        return;
    }
    entry.next_poll_at_secs = entry.next_poll_at_secs.min(now);
}

/// A refresh failed for a non-account reason: hold the keep-alive off for
/// `backoff_secs` ([`REFRESH_FAILED_BACKOFF_SECS`] for a transport/5xx loss,
/// [`REFRESH_UNSUPPORTED_BACKOFF_SECS`] for a store that holds no refresh
/// token at all). The cached expiry is left ALONE: it is still the truth about
/// the token on disk, and clearing it would make every beat retry instantly.
pub fn note_refresh_failed(entry: &mut AgentCacheEntry, now: u64, backoff_secs: u64) {
    entry.refresh_backoff_until_secs = Some(now + backoff_secs);
}

/// The endpoint said `invalid_grant`: remember the marker (newest first,
/// capped, deduped) so a restart never re-spends a grant we already know is
/// dead — re-spending one is how a refresh loop burns a working login.
pub fn note_dead_refresh_token(entry: &mut AgentCacheEntry, marker: String) {
    entry.dead_refresh_tokens.retain(|known| *known != marker);
    entry.dead_refresh_tokens.insert(0, marker);
    entry.dead_refresh_tokens.truncate(MAX_DEAD_REFRESH_TOKENS);
}

// ---------------------------------------------------------------------------
// EXP-849: the keep-alive claim (ONE refresh actor per login, machine-wide)
// ---------------------------------------------------------------------------

/// How long a keep-alive claim is honored before another process may take it
/// over. A holder that was SIGKILLed mid-probe (or a laptop that slept through
/// one) leaves its file behind, and a claim nobody releases would park the
/// login's keep-alive forever — far longer than the work itself can take.
///
/// The worst case a claim is legitimately held is EXP-852's claude step: up to
/// ~10s waiting out the CLI's `.oauth_refresh.lock` backoff, a token POST
/// bounded at 30s, and a few credential-store writes — well under a minute,
/// and so still far under these 600s. (Codex's `account/read` is shorter
/// still: [`crate::codex_app_server::PROBE_TIMEOUT`] is seconds.) The margin
/// is deliberate — the cost of waiting too long is one skipped keep-alive, the
/// cost of stealing a LIVE claim is two processes rotating one credential.
pub const REFRESH_CLAIM_STALE_SECS: u64 = 600;

/// EXP-849 — a held claim on ONE login's keep-alive refresh. Dropping it
/// releases the claim.
///
/// The poll floors live in `agent-usage.json` and are shared, so two processes
/// never spend two REQUESTS on one login. A `refreshToken: true` read is
/// different: it ROTATES the credential the store holds, so the IDE and the
/// daemon both running the keep-alive on the same profile would rotate it
/// twice, and the loser would be writing a token the winner has already
/// replaced. The claim file is the cross-process lock that keeps the rotation
/// to one actor; a process that cannot take it probes WITHOUT the keep-alive
/// (the numbers it wanted are unaffected).
#[derive(Debug)]
pub struct RefreshClaim {
    path: PathBuf,
}

impl RefreshClaim {
    /// The claim file, for tests and diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RefreshClaim {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One file per LOGIN beside the cache it guards. The id is slugged: a profile
/// id is an opaque string, and one carrying a path separator must not escape
/// the data dir.
fn refresh_claim_path(data_dir: &Path, agent: &str, profile: &str) -> PathBuf {
    let slug = |value: &str| {
        value
            .chars()
            .map(|c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                true => c,
                false => '-',
            })
            .collect::<String>()
    };
    data_dir.join(format!("{}-{}.refresh.claim", slug(agent), slug(profile)))
}

/// Whether an EXISTING claim's contents may be taken over: it is older than
/// [`REFRESH_CLAIM_STALE_SECS`], or it names no time at all (a truncated or
/// hand-edited file is nobody's claim — the alternative is a file that parks
/// the keep-alive until someone deletes it).
pub fn refresh_claim_is_stale(contents: &str, now: u64) -> bool {
    match contents
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(taken_at) => now.saturating_sub(taken_at) >= REFRESH_CLAIM_STALE_SECS,
        None => true,
    }
}

/// Take the keep-alive claim for `(agent, profile)`, machine-wide. `None` =
/// another live process holds it, so this pass must probe without the
/// keep-alive. The claim is released when the returned guard drops.
pub fn claim_refresh(
    data_dir: &Path,
    agent: &str,
    profile: &str,
    now: u64,
) -> Option<RefreshClaim> {
    let path = refresh_claim_path(data_dir, agent, profile);
    // Serialized against the cache's own load-modify-save, so two threads of
    // THIS process cannot both win the stale takeover below.
    let _guard = locked();
    if let Some(claim) = create_claim(&path, now) {
        return Some(claim);
    }
    let stale = match std::fs::read_to_string(&path) {
        Ok(contents) => refresh_claim_is_stale(&contents, now),
        // Unreadable, or released between the two calls: either way nobody is
        // provably holding it.
        Err(_) => true,
    };
    if !stale {
        return None;
    }
    let _ = std::fs::remove_file(&path);
    create_claim(&path, now)
}

/// `create_new` is the atomic step: exactly one process can create the file.
fn create_claim(path: &Path, now: u64) -> Option<RefreshClaim> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .ok()?;
    // pid + when, so a human reading the file can tell who parked it.
    let _ = write!(file, "{} {now}", std::process::id());
    Some(RefreshClaim {
        path: path.to_path_buf(),
    })
}

/// When this agent may be polled again, given what the last attempt did.
/// Every window sitting at 100 % pins the answer to just past the earliest
/// reset — the numbers physically cannot move before then. One maxed window
/// among others does NOT (EXP-817): the others keep moving.
pub fn next_poll_at(entry: &AgentCacheEntry, outcome: PollOutcome, now: u64) -> u64 {
    let delay = match outcome {
        PollOutcome::Changed => MIN_POLL_SECS,
        PollOutcome::Unchanged => (UNCHANGED_BASE_SECS
            + UNCHANGED_STEP_SECS * u64::from(entry.unchanged_streak))
        .clamp(UNCHANGED_BASE_SECS, UNCHANGED_MAX_SECS),
        PollOutcome::RateLimited => RATE_LIMITED_FLOOR_SECS,
        PollOutcome::Unauthorized => UNAUTHORIZED_BACKOFF_SECS,
        PollOutcome::Failed => FAILED_BACKOFF_SECS,
    };
    let scheduled = now + delay;
    match entry.earliest_reset_secs {
        Some(reset) => scheduled.max(reset + RESET_MARGIN_SECS),
        None => scheduled,
    }
}

/// Fold one attempt's result into the entry: keep or replace the numbers,
/// move the unchanged streak, and schedule the next poll.
pub fn apply_outcome(
    entry: &mut AgentCacheEntry,
    outcome: PollOutcome,
    windows: Option<Vec<UsageWindow>>,
    now: u64,
    stamp: &str,
) {
    entry.fetched_at_secs = now;
    let outcome = match (outcome, windows) {
        (PollOutcome::Changed | PollOutcome::Unchanged, Some(windows)) => {
            let hash = windows_hash(&windows);
            let unchanged = entry.usage.is_some() && hash == entry.last_windows_hash;
            entry.unchanged_streak = if unchanged {
                entry.unchanged_streak.saturating_add(1)
            } else {
                0
            };
            entry.last_windows_hash = hash;
            entry.earliest_reset_secs = earliest_maxed_reset(&windows);
            entry.usage = Some(AgentUsage {
                fetched_at: stamp.to_string(),
                stale: false,
                windows,
            });
            // A successful read proves the credential store answers again.
            entry.credential_denied_until_secs = None;
            entry.rate_limited_until_secs = None;
            // EXP-849: …and proves the credential itself still works, which
            // is exactly what `health: ok` claims.
            entry.health = Some(crate::agent_accounts::Health::Ok.as_str().to_string());
            if unchanged {
                PollOutcome::Unchanged
            } else {
                PollOutcome::Changed
            }
        }
        (outcome, _) => {
            // Keep the old numbers — dimmed, never blanked, never invented.
            if let Some(usage) = &mut entry.usage {
                usage.stale = true;
            }
            if outcome == PollOutcome::RateLimited {
                entry.rate_limited_until_secs = Some(now + RATE_LIMITED_FLOOR_SECS);
            }
            // EXP-849: a 401/403 is the provider saying this credential is no
            // longer good for anything — the one failure whose fix is a
            // login. Every other failure (transport, an unparseable body, a
            // 429, a refused keychain) leaves the badge where it was: those
            // are this machine's problems, not the account's.
            if outcome == PollOutcome::Unauthorized {
                entry.health =
                    Some(crate::agent_accounts::Health::NeedsRelogin.as_str().to_string());
            }
            outcome
        }
    };
    entry.next_poll_at_secs = next_poll_at(entry, outcome, now);
}

/// EXP-792: make `entry` due for one FORCED poll — past the scheduled next
/// poll and the machine-wide shared TTL, but never past the 429 floor.
/// `Err(until)` = still rate-limited until that unix second.
pub fn force_due(entry: &mut AgentCacheEntry, now: u64) -> Result<(), u64> {
    if let Some(until) = entry.rate_limited_until_secs.filter(|until| now < *until) {
        return Err(until);
    }
    entry.next_poll_at_secs = 0;
    entry.fetched_at_secs = 0;
    entry.endpoint_due_at_secs = None;
    entry.credential_denied_until_secs = None;
    Ok(())
}

/// A stable digest of the rendered windows — the change detector. SHA-256
/// (not `DefaultHasher`, which is explicitly unstable across releases) so a
/// compiler bump cannot make every cached entry look changed.
pub fn windows_hash(windows: &[UsageWindow]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for window in windows {
        hasher.update(window.key.as_bytes());
        hasher.update([0]);
        hasher.update(window.percent.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(window.resets_at.as_deref().unwrap_or_default().as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())[..16].to_string()
}

/// The soonest reset (unix seconds) when EVERY window is at 100 % — `None`
/// as soon as one window has room, because that one can move (EXP-817).
fn earliest_maxed_reset(windows: &[UsageWindow]) -> Option<u64> {
    if windows.is_empty() || windows.iter().any(|window| window.percent < 100) {
        return None;
    }
    windows
        .iter()
        .filter_map(|window| {
            let stamp = window.resets_at.as_deref()?;
            chrono::DateTime::parse_from_rfc3339(stamp)
                .ok()
                .map(|at| at.timestamp())
                .filter(|secs| *secs > 0)
                .map(|secs| secs as u64)
        })
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_drops_one_agent_and_makes_its_poll_due() {
        let dir = std::env::temp_dir().join(format!("exp-usage-cache-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cache = UsageCache::default();
        let mut entry = AgentCacheEntry::default();
        entry.next_poll_at_secs = 10_000;
        cache.insert(entry_key("codex", "system"), entry.clone());
        cache.insert(entry_key("claude", "system"), entry.clone());
        cache.insert(entry_key("claude", "0a1b2c3d"), entry);
        save(&dir, &cache);
        forget(&dir, "codex");
        let reloaded = load(&dir);
        assert!(reloaded.get(&entry_key("codex", "system")).is_none());
        assert!(reloaded.get(&entry_key("claude", "system")).is_some());
        assert!(poll_due(&AgentCacheEntry::default(), 5_000));
        // A profile's forget leaves the ambient login's row alone.
        forget_profile(&dir, "claude", "0a1b2c3d");
        let reloaded = load(&dir);
        assert!(reloaded.get(&entry_key("claude", "0a1b2c3d")).is_none());
        assert!(reloaded.get(&entry_key("claude", "system")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-792: a pre-profile file keys rows by bare agent id; they load as
    /// the ambient login's (`agent:system`) — and a row already under the
    /// new key wins over the legacy one.
    #[test]
    fn load_migrates_legacy_agent_keys_to_the_system_profile() {
        let dir = temp_dir("migrate");
        let legacy = AgentCacheEntry { unchanged_streak: 7, ..AgentCacheEntry::default() };
        let native = AgentCacheEntry { unchanged_streak: 1, ..AgentCacheEntry::default() };
        let mut object = serde_json::Map::new();
        object.insert("claude".into(), serde_json::to_value(&legacy).unwrap());
        object.insert("codex".into(), serde_json::to_value(&legacy).unwrap());
        object.insert("codex:system".into(), serde_json::to_value(&native).unwrap());
        std::fs::write(cache_path(&dir), serde_json::to_string(&object).unwrap()).unwrap();
        let loaded = load(&dir);
        assert_eq!(loaded.get("claude"), None, "bare keys are gone");
        assert_eq!(loaded.get(&entry_key("claude", "system")), Some(&legacy));
        assert_eq!(loaded.get(&entry_key("codex", "system")), Some(&native), "the native row wins");
        // Saving writes the new keys only.
        save(&dir, &loaded);
        let raw = std::fs::read_to_string(cache_path(&dir)).unwrap();
        assert!(!raw.contains("\"claude\":"), "{raw}");
        assert!(raw.contains("\"claude:system\""), "{raw}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-792: one profile's 429 parks THAT profile at the floor; its
    /// sibling on the same agent stays due (a separate login, a separate
    /// budget).
    #[test]
    fn a_profiles_rate_limit_never_backs_off_its_sibling() {
        let now = 1_000_000;
        let mut cache = UsageCache::default();
        let mut limited = AgentCacheEntry { fetched_at_secs: now - 400, ..AgentCacheEntry::default() };
        apply_outcome(&mut limited, PollOutcome::RateLimited, None, now, "T");
        cache.insert(entry_key("claude", "0a1b2c3d"), limited);
        let sibling = AgentCacheEntry { fetched_at_secs: now - 400, ..AgentCacheEntry::default() };
        cache.insert(entry_key("claude", "system"), sibling);

        let limited = cache.get(&entry_key("claude", "0a1b2c3d")).unwrap();
        assert!(!poll_due(limited, now + 10));
        assert_eq!(force_due(&mut limited.clone(), now + 10), Err(now + RATE_LIMITED_FLOOR_SECS));
        let sibling = cache.get(&entry_key("claude", "system")).unwrap();
        assert!(poll_due(sibling, now + 10), "the sibling profile is untouched");
        assert_eq!(force_due(&mut sibling.clone(), now + 10), Ok(()));
    }

    /// EXP-849: the keep-alive claim is the cross-process lock that keeps ONE
    /// refresh actor per login — a second claimant is refused and probes
    /// without the keep-alive, and releasing the claim hands it on.
    #[test]
    fn only_one_process_holds_a_logins_refresh_claim() {
        let dir = temp_dir("refresh-claim");
        let now = 1_700_000_000;
        let first = claim_refresh(&dir, "codex", "0a1b2c3d", now)
            .expect("the first claimant takes it");
        assert!(first.path().exists());
        // The sibling process (same login) is refused…
        assert!(claim_refresh(&dir, "codex", "0a1b2c3d", now).is_none());
        // …but ANOTHER login's keep-alive is independent.
        let sibling = claim_refresh(&dir, "codex", "system", now)
            .expect("one claim per login, not per agent");
        assert_ne!(first.path(), sibling.path());
        // Releasing hands it on, and leaves no file behind.
        let path = first.path().to_path_buf();
        drop(first);
        assert!(!path.exists());
        let again = claim_refresh(&dir, "codex", "0a1b2c3d", now).expect("released");
        assert_eq!(again.path(), path);
        drop(again);
        drop(sibling);

        // A holder that died mid-probe leaves its file behind: the claim is
        // taken over once it goes stale, never before.
        std::fs::write(&path, format!("4242 {}", now - 10)).unwrap();
        assert!(claim_refresh(&dir, "codex", "0a1b2c3d", now).is_none());
        let reclaimed = claim_refresh(&dir, "codex", "0a1b2c3d", now + REFRESH_CLAIM_STALE_SECS)
            .expect("a stale claim is taken over");
        assert_eq!(reclaimed.path(), path);
        drop(reclaimed);

        // A truncated/hand-edited file names nobody.
        std::fs::write(&path, "garbage").unwrap();
        assert!(claim_refresh(&dir, "codex", "0a1b2c3d", now).is_some());

        // A profile id can never escape the data dir.
        let nasty = claim_refresh(&dir, "codex", "../../etc/passwd", now).unwrap();
        assert_eq!(nasty.path().parent(), Some(dir.as_path()));
    }

    #[test]
    fn a_claim_is_stale_when_it_is_old_or_names_no_time() {
        let now = 1_700_000_000;
        assert!(!refresh_claim_is_stale(&format!("17 {now}"), now));
        assert!(!refresh_claim_is_stale(
            &format!("17 {}", now - REFRESH_CLAIM_STALE_SECS + 1),
            now
        ));
        assert!(refresh_claim_is_stale(
            &format!("17 {}", now - REFRESH_CLAIM_STALE_SECS),
            now
        ));
        assert!(refresh_claim_is_stale("", now));
        assert!(refresh_claim_is_stale("17", now));
        assert!(refresh_claim_is_stale("17 later", now));
        // Clock skew (a stamp from the future) is not stale.
        assert!(!refresh_claim_is_stale(&format!("17 {}", now + 90), now));
    }

    fn window(key: &str, percent: u8, resets_at: Option<&str>) -> UsageWindow {
        UsageWindow {
            key: key.to_string(),
            label: key.to_string(),
            percent,
            resets_at: resets_at.map(str::to_string),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-usage-cache-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole policy truth table in one place — every arm of the
    /// cadence, the streak's clamp, and the maxed-window floor.
    #[test]
    fn poll_policy_schedules_every_outcome() {
        let now = 1_000_000;
        let fresh = AgentCacheEntry::default();
        assert_eq!(next_poll_at(&fresh, PollOutcome::Changed, now), now + 180);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Unchanged, now), now + 300);
        assert_eq!(next_poll_at(&fresh, PollOutcome::RateLimited, now), now + 300);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Unauthorized, now), now + 600);
        assert_eq!(next_poll_at(&fresh, PollOutcome::Failed, now), now + 300);

        // The unchanged streak backs off 100 s a run and clamps at 600.
        for (streak, expected) in [(0, 300), (1, 400), (3, 600), (9, 600)] {
            let entry = AgentCacheEntry {
                unchanged_streak: streak,
                ..AgentCacheEntry::default()
            };
            assert_eq!(
                next_poll_at(&entry, PollOutcome::Unchanged, now),
                now + expected,
                "streak {streak}"
            );
        }

        // A window at 100 % pins every outcome past its reset + 60.
        let maxed = AgentCacheEntry {
            earliest_reset_secs: Some(now + 4000),
            ..AgentCacheEntry::default()
        };
        assert_eq!(
            next_poll_at(&maxed, PollOutcome::Changed, now),
            now + 4000 + 60
        );
        // A reset already past never PULLS the schedule forward.
        let stale_reset = AgentCacheEntry {
            earliest_reset_secs: Some(now - 5000),
            ..AgentCacheEntry::default()
        };
        assert_eq!(
            next_poll_at(&stale_reset, PollOutcome::Changed, now),
            now + 180
        );
    }

    #[test]
    fn poll_due_respects_the_schedule_the_shared_ttl_and_a_refusal() {
        let now = 1_000_000;
        // A never-polled entry is due immediately.
        assert!(poll_due(&AgentCacheEntry::default(), now));

        let entry = AgentCacheEntry {
            fetched_at_secs: now - 200,
            next_poll_at_secs: now - 10,
            ..AgentCacheEntry::default()
        };
        assert!(poll_due(&entry, now));

        // Scheduled, but ANOTHER process fetched 60 s ago: the shared TTL
        // holds this one back (one token budget per machine).
        let shared = AgentCacheEntry {
            fetched_at_secs: now - 60,
            next_poll_at_secs: now - 10,
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&shared, now));

        // Not yet scheduled.
        let early = AgentCacheEntry {
            fetched_at_secs: now - 400,
            next_poll_at_secs: now + 10,
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&early, now));

        // A refused credential store parks the agent for an hour.
        let denied = AgentCacheEntry {
            credential_denied_until_secs: Some(now + 10),
            ..AgentCacheEntry::default()
        };
        assert!(!poll_due(&denied, now));
        assert!(poll_due(&denied, now + 10));
    }

    /// EXP-849 — health tracks the CREDENTIAL's own answer and nothing else:
    /// a read proves it works, a 401 proves it does not, and a flaky network
    /// (or a 429, or a refused keychain) leaves the badge where it was.
    #[test]
    fn health_follows_the_probe_and_the_keep_alive_has_its_own_cadence() {
        use crate::agent_accounts::Health;
        let now = 1_000_000;
        let mut entry = AgentCacheEntry::default();
        assert_eq!(entry.health, None, "never probed");

        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window("session", 10, None)]),
            now,
            "T0",
        );
        assert_eq!(entry.health.as_deref(), Some(Health::Ok.as_str()));

        // Transport failures say nothing about the account.
        for outcome in [
            PollOutcome::Failed,
            PollOutcome::RateLimited,
            PollOutcome::Unchanged,
        ] {
            apply_outcome(&mut entry, outcome, None, now + 1, "T1");
            assert_eq!(
                entry.health.as_deref(),
                Some(Health::Ok.as_str()),
                "{outcome:?} must not flip the badge"
            );
        }

        // A 401 does — and a later good read clears it again.
        apply_outcome(&mut entry, PollOutcome::Unauthorized, None, now + 2, "T2");
        assert_eq!(entry.health.as_deref(), Some(Health::NeedsRelogin.as_str()));
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window("session", 20, None)]),
            now + 3,
            "T3",
        );
        assert_eq!(entry.health.as_deref(), Some(Health::Ok.as_str()));

        // The keep-alive's cadence is its own — never probed is due, and one
        // run parks it for six hours.
        assert!(refresh_due(&entry, now));
        entry.refreshed_at_secs = Some(now);
        assert!(!refresh_due(&entry, now + CODEX_REFRESH_INTERVAL_SECS - 1));
        assert!(refresh_due(&entry, now + CODEX_REFRESH_INTERVAL_SECS));
    }

    /// EXP-852 helper: an entry whose only interesting fact is its cached
    /// claude expiry.
    fn expiring_at(expires_at_ms: i64) -> AgentCacheEntry {
        AgentCacheEntry {
            claude_expires_at_ms: Some(expires_at_ms),
            ..AgentCacheEntry::default()
        }
    }

    /// Unix milliseconds `secs` seconds after `now`.
    fn ms_after(now: u64, secs: i64) -> i64 {
        (now as i64 + secs) * 1000
    }

    /// EXP-852 — the claude keep-alive's cadence is the token's EXPIRY, not a
    /// timer: nothing happens until the margin opens, and everything inside it
    /// is due. The margin is three times the CLI's own 5-minute predicate so
    /// we rotate first and a CLI start inside the window finds `not_needed`.
    #[test]
    fn claude_refresh_is_due_only_inside_the_margin() {
        let now = 1_700_000_000;

        // An 8-hour token, minutes old: nowhere near due.
        let fresh = expiring_at(ms_after(now, 8 * 3600));
        assert!(!claude_refresh_due(&fresh, now));

        // …and it becomes due exactly when the clock reaches the margin.
        let opens = now + 8 * 3600 - CLAUDE_REFRESH_MARGIN_SECS;
        assert!(!claude_refresh_due(&fresh, opens - 1));
        assert!(claude_refresh_due(&fresh, opens));

        // The same edge read from the expiry side: one millisecond past the
        // margin is not due, the margin itself is.
        let edge = ms_after(now, CLAUDE_REFRESH_MARGIN_SECS as i64);
        assert!(claude_refresh_due(&expiring_at(edge), now));
        assert!(!claude_refresh_due(&expiring_at(edge + 1), now));

        // A hand-edited absurd expiry must not wrap the comparison into
        // "not due" — it simply stays far away.
        assert!(!claude_refresh_due(&expiring_at(i64::MAX), now));

        // EXP-881 — the two predicates are NOT the same question. Inside the
        // margin the token is due a refresh but still WORKS, so nothing is
        // forced: only an already-dead one overrides the keep-alive setting.
        assert!(claude_refresh_due(&expiring_at(edge), now));
        assert!(!claude_token_expired(&expiring_at(edge), now));
        assert!(!claude_token_expired(&fresh, opens));
    }

    /// An access token that already expired is trivially inside the margin, so
    /// it IS refreshed (the CLI's predicate is true for any past instant) —
    /// the laptop that slept through its own expiry rotates on the first beat
    /// instead of waiting for a 401.
    #[test]
    fn an_expired_claude_token_is_due() {
        let now = 1_700_000_000;
        assert!(claude_refresh_due(&expiring_at(ms_after(now, -60)), now));
        assert!(claude_refresh_due(&expiring_at(ms_after(now, -90 * 86_400)), now));
        // A zero/absent-looking stamp is "expired in 1970", i.e. due.
        assert!(claude_refresh_due(&expiring_at(0), now));

        // EXP-881: all three are also EXPIRED, the stronger fact that makes
        // the collector rotate whatever `claudeKeepAlive` says. An entry that
        // was never read is not expired — we have no evidence either way.
        assert!(claude_token_expired(&expiring_at(ms_after(now, -60)), now));
        assert!(claude_token_expired(&expiring_at(0), now));
        assert!(!claude_token_expired(&AgentCacheEntry::default(), now));
    }

    /// `None` = never read (or a document with no expiry): look ONCE, then let
    /// what the read saw schedule every later beat. Without this a fresh
    /// install would never take its first look.
    #[test]
    fn an_unread_login_is_due_once() {
        let now = 1_700_000_000;
        let mut entry = AgentCacheEntry::default();
        assert_eq!(entry.claude_expires_at_ms, None, "never read");
        assert!(claude_refresh_due(&entry, now));

        note_credential_expiry(&mut entry, Some(ms_after(now, 8 * 3600)));
        assert!(!claude_refresh_due(&entry, now), "the read answered the gate");

        // A later read that finds no expiry puts it back to "look now"; the
        // keep-alive pairs that with the hour backoff, so "now" is once an
        // hour rather than a keychain read on every beat.
        note_credential_expiry(&mut entry, None);
        assert!(claude_refresh_due(&entry, now));
        note_refresh_failed(&mut entry, now, REFRESH_UNSUPPORTED_BACKOFF_SECS);
        assert_eq!(entry.claude_expires_at_ms, None, "the read's answer stands");
        assert!(!claude_refresh_due(&entry, now));
        assert!(!claude_refresh_due(&entry, now + REFRESH_UNSUPPORTED_BACKOFF_SECS - 1));
        assert!(claude_refresh_due(&entry, now + REFRESH_UNSUPPORTED_BACKOFF_SECS));
    }

    /// A refresh the NETWORK lost backs the keep-alive off — the only rate
    /// limit this path has — and a landed rotation clears it along with
    /// stamping the new expiry. Health is untouched throughout: a flaky link
    /// is not the account's answer.
    #[test]
    fn a_refresh_backoff_holds_the_keep_alive_off() {
        let now = 1_700_000_000;
        let mut entry = AgentCacheEntry::default();
        assert!(claude_refresh_due(&entry, now));

        note_refresh_failed(&mut entry, now, REFRESH_FAILED_BACKOFF_SECS);
        assert!(!claude_refresh_due(&entry, now));
        assert!(!claude_refresh_due(&entry, now + REFRESH_FAILED_BACKOFF_SECS - 1));
        assert!(claude_refresh_due(&entry, now + REFRESH_FAILED_BACKOFF_SECS));
        assert_eq!(entry.health, None, "a lost request says nothing about the account");

        // A store with no refresh token at all waits an hour, not ten minutes.
        note_refresh_failed(&mut entry, now, REFRESH_UNSUPPORTED_BACKOFF_SECS);
        assert!(!claude_refresh_due(&entry, now + REFRESH_FAILED_BACKOFF_SECS));
        assert!(claude_refresh_due(&entry, now + REFRESH_UNSUPPORTED_BACKOFF_SECS));

        // A rotation lands: backoff gone, expiry recorded, stamp moved.
        let expires = ms_after(now, 8 * 3600);
        note_refresh_ok(&mut entry, expires, now + 10);
        assert_eq!(entry.refresh_backoff_until_secs, None);
        assert_eq!(entry.claude_expires_at_ms, Some(expires));
        assert_eq!(entry.refreshed_at_secs, Some(now + 10));
        assert!(!claude_refresh_due(&entry, now + 10));
    }

    /// A credential store that REFUSED (the macOS Keychain ACL prompt on a
    /// headless daemon) parks the keep-alive exactly like it parks the poll:
    /// there is nothing to refresh if we cannot read the token.
    #[test]
    fn a_credential_denial_holds_the_keep_alive_off() {
        let now = 1_700_000_000;
        let entry = AgentCacheEntry {
            credential_denied_until_secs: Some(now + CREDENTIAL_DENIED_BACKOFF_SECS),
            ..AgentCacheEntry::default()
        };
        // Due on the expiry test alone (never read), held by the denial.
        assert!(entry.claude_expires_at_ms.is_none());
        assert!(!claude_refresh_due(&entry, now));
        assert!(!claude_refresh_due(&entry, now + CREDENTIAL_DENIED_BACKOFF_SECS - 1));
        assert!(claude_refresh_due(&entry, now + CREDENTIAL_DENIED_BACKOFF_SECS));

        // An EXPIRED token does not punch through the denial either.
        let denied_and_expired = AgentCacheEntry {
            claude_expires_at_ms: Some(ms_after(now, -60)),
            ..entry
        };
        assert!(!claude_refresh_due(&denied_and_expired, now));
    }

    /// EXP-852 — the dead-grant set: newest first, capped, and re-noting a
    /// marker MOVES it instead of duplicating (a grant the endpoint keeps
    /// refusing must not evict the other markers by repeating itself).
    #[test]
    fn the_dead_refresh_token_list_is_capped_newest_first() {
        let mut entry = AgentCacheEntry::default();
        for n in 0..MAX_DEAD_REFRESH_TOKENS + 2 {
            note_dead_refresh_token(&mut entry, format!("marker{n}"));
        }
        assert_eq!(entry.dead_refresh_tokens.len(), MAX_DEAD_REFRESH_TOKENS);
        assert_eq!(
            entry.dead_refresh_tokens[0],
            format!("marker{}", MAX_DEAD_REFRESH_TOKENS + 1),
            "newest first"
        );
        assert!(
            !entry.dead_refresh_tokens.contains(&"marker0".to_string()),
            "the oldest markers fall off the end"
        );

        // Re-noting the oldest survivor moves it to the front, once.
        let oldest = entry.dead_refresh_tokens.last().unwrap().clone();
        note_dead_refresh_token(&mut entry, oldest.clone());
        assert_eq!(entry.dead_refresh_tokens[0], oldest);
        assert_eq!(entry.dead_refresh_tokens.len(), MAX_DEAD_REFRESH_TOKENS);
        assert_eq!(
            entry
                .dead_refresh_tokens
                .iter()
                .filter(|marker| **marker == oldest)
                .count(),
            1,
            "moved, not duplicated"
        );
    }

    /// EXP-852 — the keep-alive fields need no wire work of their own: the
    /// struct's `#[serde(flatten)] extra` map means a host that PREDATES them
    /// loads the row, rewrites it, and hands every value back untouched. A
    /// downgrade therefore never re-spends a dead grant nor forgets a backoff.
    /// Simulated with an OLD-shaped entry: the new fields absent, the flatten
    /// map present, exactly as this struct looked before EXP-852.
    #[test]
    fn a_newer_hosts_keep_alive_fields_survive_an_older_rewrite() {
        #[derive(Default, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", default)]
        struct OldEntry {
            fetched_at_secs: u64,
            next_poll_at_secs: u64,
            unchanged_streak: u32,
            last_windows_hash: String,
            #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
            extra: BTreeMap<String, Value>,
        }

        let now = 1_700_000_000;
        let mut entry = AgentCacheEntry {
            fetched_at_secs: now,
            next_poll_at_secs: now + 300,
            claude_expires_at_ms: Some(ms_after(now, 8 * 3600)),
            refresh_backoff_until_secs: Some(now + REFRESH_FAILED_BACKOFF_SECS),
            dead_refresh_tokens: vec!["deadbeefdeadbeef".into(), "0011223344556677".into()],
            refreshed_at_secs: Some(now - 5),
            endpoint_fetched_at_secs: Some(now - 30),
            ..AgentCacheEntry::default()
        };
        // …plus a field from a build NEWER than either of them.
        entry
            .extra
            .insert("futureField".into(), Value::String("keep me".into()));

        let written = serde_json::to_string(&entry).unwrap();
        for key in [
            "claudeExpiresAtMs",
            "refreshBackoffUntilSecs",
            "deadRefreshTokens",
            // EXP-881: the endpoint's own stamp rides the same flatten map.
            "endpointFetchedAtSecs",
        ] {
            assert!(written.contains(key), "{key} must be persisted: {written}");
        }

        // The older host parses what it knows and buffers the rest…
        let old: OldEntry = serde_json::from_str(&written).unwrap();
        assert_eq!(old.fetched_at_secs, now, "the shared fields still parse");
        assert!(old.extra.contains_key("claudeExpiresAtMs"));
        assert!(old.extra.contains_key("endpointFetchedAtSecs"));
        assert!(old.extra.contains_key("futureField"));

        // …and its rewrite hands every one of them back verbatim.
        let rewritten = serde_json::to_string(&old).unwrap();
        let round_tripped: AgentCacheEntry = serde_json::from_str(&rewritten).unwrap();
        assert_eq!(round_tripped, entry);
        assert_eq!(
            round_tripped.extra["futureField"],
            Value::String("keep me".into()),
            "an unknown key survives the same way"
        );

        // A default entry writes NONE of the keep-alive keys (skip_serializing_if).
        let bare = serde_json::to_string(&AgentCacheEntry::default()).unwrap();
        assert!(!bare.contains("claudeExpiresAtMs"), "{bare}");
        assert!(!bare.contains("deadRefreshTokens"), "{bare}");
        assert!(!bare.contains("endpointFetchedAtSecs"), "{bare}");
    }

    /// EXP-881 — the two stamps answer different questions. `fetched_at_secs`
    /// says "when did anything last land" and a LIVE apply moves it;
    /// `endpoint_fetched_at_secs` says "when did the ENDPOINT last answer",
    /// which is the only stamp a live frame may be compared against.
    #[test]
    fn an_endpoint_read_is_stamped_apart_from_a_live_apply() {
        let now = 1_700_000_000;
        let mut entry = AgentCacheEntry::default();
        assert_eq!(entry.endpoint_fetched_at_secs, None, "never polled");

        // A real endpoint read: the collector stamps both.
        entry.endpoint_fetched_at_secs = Some(now);
        apply_outcome(&mut entry, PollOutcome::Changed, Some(Vec::new()), now, "poll");
        assert_eq!(entry.fetched_at_secs, now);
        assert_eq!(entry.endpoint_fetched_at_secs, Some(now));

        // A live frame, a minute later: `apply_outcome` moves the shared
        // stamp and leaves the endpoint's own where the poll left it.
        apply_outcome(&mut entry, PollOutcome::Changed, Some(Vec::new()), now + 60, "live");
        assert_eq!(entry.fetched_at_secs, now + 60);
        assert_eq!(
            entry.endpoint_fetched_at_secs,
            Some(now),
            "a live apply is not an endpoint read"
        );
    }

    /// EXP-881 — a rotation takes down the wall the FAILED refresh put up, so
    /// the dimmed numbers can be re-read on this beat. The 429 floor is the
    /// server's instruction and outlives any token of ours.
    #[test]
    fn a_rotation_releases_only_the_failed_backoff() {
        let now = 1_700_000_000;

        let mut failed = AgentCacheEntry::default();
        apply_outcome(&mut failed, PollOutcome::Failed, None, now, "s");
        assert!(failed.next_poll_at_secs > now, "the failure backed off");
        note_token_rotated(&mut failed, now);
        assert_eq!(failed.next_poll_at_secs, now, "the new token may be tried now");
        assert!(poll_due(&failed, now + SHARED_TTL_SECS));

        let mut limited = AgentCacheEntry {
            rate_limited_until_secs: Some(now + 600),
            next_poll_at_secs: now + 600,
            ..AgentCacheEntry::default()
        };
        note_token_rotated(&mut limited, now);
        assert_eq!(
            limited.next_poll_at_secs,
            now + 600,
            "a 429 floor is not ours to lift"
        );
        // Past the floor a rotation releases the schedule like any other.
        note_token_rotated(&mut limited, now + 600);
        assert_eq!(limited.next_poll_at_secs, now + 600);
    }

    #[test]
    fn apply_outcome_keeps_old_numbers_stale_on_failure() {
        let now = 1_000_000;
        let mut entry = AgentCacheEntry::default();
        let windows = vec![window("session", 40, None)];
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now,
            "T0",
        );
        assert_eq!(entry.usage.as_ref().unwrap().fetched_at, "T0");
        assert!(!entry.usage.as_ref().unwrap().stale);
        assert_eq!(entry.unchanged_streak, 0);
        assert_eq!(entry.next_poll_at_secs, now + 180);

        // The same numbers again: unchanged, slower cadence, same values.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(windows.clone()),
            now + 200,
            "T1",
        );
        assert_eq!(entry.unchanged_streak, 1);
        // The streak the run just earned is the one that schedules it.
        assert_eq!(entry.next_poll_at_secs, now + 200 + 400);
        assert_eq!(entry.usage.as_ref().unwrap().fetched_at, "T1");

        // A 401: the numbers STAY, flagged stale, and back off 600 s.
        apply_outcome(&mut entry, PollOutcome::Unauthorized, None, now + 600, "T2");
        let usage = entry.usage.as_ref().unwrap();
        assert!(usage.stale);
        assert_eq!(usage.fetched_at, "T1", "the numbers keep their own age");
        assert_eq!(usage.windows, windows);
        assert_eq!(entry.next_poll_at_secs, now + 600 + 600);

        // New numbers clear the flag and reset the streak.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window("session", 55, None)]),
            now + 1300,
            "T3",
        );
        assert!(!entry.usage.as_ref().unwrap().stale);
        assert_eq!(entry.unchanged_streak, 0);
    }

    /// EXP-817: the reset pin applies only when EVERY window is maxed. One
    /// window at 100 % beside one with room used to freeze the whole agent
    /// past that reset (a maxed per-model window froze the session and
    /// weekly numbers for days), so a mixed report keeps the ordinary
    /// cadence and only an all-maxed one waits for the earliest reset.
    #[test]
    fn apply_outcome_pins_the_next_poll_only_when_every_window_is_maxed() {
        let now = 1_756_000_000;
        let mut entry = AgentCacheEntry::default();
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![
                window("session", 100, Some("2025-08-24T03:26:40Z")),
                window("weekly", 12, Some("2025-08-30T00:00:00Z")),
            ]),
            now,
            "T0",
        );
        // The weekly window can still move: no pin, the fast cadence.
        assert_eq!(entry.earliest_reset_secs, None);
        assert_eq!(entry.next_poll_at_secs, now + MIN_POLL_SECS);

        // Everything maxed: nothing moves before the EARLIEST reset.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![
                window("session", 100, Some("2025-08-24T03:26:40Z")),
                window("weekly", 100, Some("2025-08-30T00:00:00Z")),
                window("model:fable", 100, None),
            ]),
            now,
            "T1",
        );
        assert_eq!(entry.earliest_reset_secs, Some(1_756_006_000));
        assert_eq!(entry.next_poll_at_secs, 1_756_006_000 + 60);

        // A single idle session window at 0 % is room too.
        apply_outcome(
            &mut entry,
            PollOutcome::Changed,
            Some(vec![window("session", 0, None)]),
            now,
            "T2",
        );
        assert_eq!(entry.earliest_reset_secs, None);
    }

    #[test]
    fn windows_hash_ignores_labels_and_tracks_values() {
        let a = vec![window("session", 40, Some("R"))];
        let mut relabelled = a.clone();
        relabelled[0].label = "Five hours".into();
        assert_eq!(windows_hash(&a), windows_hash(&relabelled));
        assert_ne!(windows_hash(&a), windows_hash(&[window("session", 41, Some("R"))]));
        assert_ne!(windows_hash(&a), windows_hash(&[]));
    }

    #[test]
    fn cache_round_trips_and_keeps_unknown_fields_and_entries() {
        let dir = temp_dir("roundtrip");
        let mut cache = UsageCache::default();
        let mut entry = AgentCacheEntry {
            fetched_at_secs: 100,
            next_poll_at_secs: 280,
            unchanged_streak: 2,
            last_windows_hash: "abc".into(),
            ..AgentCacheEntry::default()
        };
        entry
            .extra
            .insert("futureField".into(), Value::String("keep me".into()));
        // EXP-792: entries are keyed per login now; the bare `claude` form
        // is what a pre-profile build wrote and is covered by the migration
        // test below.
        cache.insert(entry_key("claude", SYSTEM_PROFILE), entry.clone());
        save(&dir, &cache);

        // A row a NEWER build wrote, of a shape this one cannot parse.
        let path = cache_path(&dir);
        let mut object: serde_json::Map<String, Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        object.insert("qwen".into(), Value::String("not an entry".into()));
        std::fs::write(&path, serde_json::to_string(&object).unwrap()).unwrap();

        let reloaded = load(&dir);
        assert_eq!(reloaded.get(&entry_key("claude", SYSTEM_PROFILE)), Some(&entry));
        assert_eq!(
            reloaded.get(&entry_key("claude", SYSTEM_PROFILE)).unwrap().extra["futureField"],
            Value::String("keep me".into())
        );
        // Rewriting must not drop the foreign row.
        save(&dir, &reloaded);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("qwen"), "{raw}");
        assert!(raw.contains("futureField"), "{raw}");

        // A corrupt file reads as empty, never as a panic.
        std::fs::write(&path, "{{{").unwrap();
        assert!(load(&dir).get("claude").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
