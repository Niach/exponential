//! EXP-1005 / EXP-1067 — the ACCOUNT ROTATION decision, pure and headless.
//!
//! Every run on this device (person-started, automation, workflow node,
//! reviewer) spends ONE signed-in account profile of its agent. Two moments
//! decide which:
//!
//! * **At start** ([`pick_start_account`], applied by
//!   [`apply_start_pick`] inside `coding::prepare`, so every launch path on
//!   the device — desktop, daemon, relay, engine — goes through it once): the
//!   signed-in, healthy profile with the MOST headroom, read off the usage
//!   cache ([`crate::agent_usage::profile_usage_snapshot`], no probe: the
//!   cache is fresh enough to avoid an obviously walled login).
//! * **At a wall** ([`pick_rotation_target`], driven by a host's
//!   [`RotationTracker`] beat): a run whose `blocked.kind = rate_limit` is
//!   moved to another profile with headroom on the window it hit — after a
//!   FORCED usage read of every profile
//!   ([`crate::agent_usage::collect_now`]), never off cached numbers — as a
//!   RESUME naming the target (the same switch a person makes; the server
//!   inherits `started_reason`, the parent and the workflow membership from
//!   the predecessor, EXP-1082 §1).
//!
//! Guards, each enforced ONCE here: claude only (codex keeps one login per
//! session by contract and waits for its reset); repo-backed runs only (a
//! scratch run's dir is purged when the switch ends it, so its resume has
//! nowhere to go); between turns only (the turn slot, EXP-848, is the
//! authority — the host reads it and passes `idle`); a per-run cooldown and
//! a cap on rotations, both persisted ([`RotationTracker::load`]) so a
//! restart does not reset them; never into a profile that hit the SAME
//! window inside its own reset; the device toggle
//! `Settings.auto_rotate_accounts` (default ON, Danny 2026-09-25) turns the
//! whole thing off. The hop is SAID in the run ([`switch_prompt`]) and, for
//! a workflow run, in the workflow's event trail ([`switch_event_message`],
//! [`waiting_event_message`]) — the trail is a team shape, so it names the
//! profile's LABEL, never the login's email.
//!
//! A START pick only: a resume (merge-upstream, review findings, a refused
//! land, a conflict relaunch) keeps its RECORDED account (EXP-906) and never
//! re-picks; moving a run that hit a wall is the mid-run switch above.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent_accounts::Health;
use crate::CodingAgent;

/// One usage window: `percent` used (0–100) and when it resets (unix ms).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub percent: u8,
    pub resets_at: Option<i64>,
}

impl Window {
    /// The percent that still COUNTS at `now_ms`: a window whose reset has
    /// passed since it was read is open again, whatever number it quoted.
    fn effective_percent(&self, now_ms: i64) -> u8 {
        if self.resets_at.is_some_and(|reset| reset <= now_ms) {
            0
        } else {
            self.percent
        }
    }

    /// Spent = at 100 % with a reset still in the future (or none known).
    fn spent(&self, now_ms: i64) -> bool {
        self.effective_percent(now_ms) >= 100
    }
}

/// The windows the pickers weigh: the 5-hour session, the weekly, and the
/// model-scoped weeklies keyed by model alias (lowercased display name, the
/// `model:<alias>` key without its prefix).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsageWindows {
    pub session: Option<Window>,
    pub weekly: Option<Window>,
    pub model: BTreeMap<String, Window>,
}

impl UsageWindows {
    /// The model window a run on `model` would spend, when one is reported:
    /// exact alias first, then a window whose alias contains the model
    /// (`fable` ↔ `fable 5.1`), else none.
    fn model_window(&self, model: Option<&str>) -> Option<&Window> {
        let model = model.map(str::trim).filter(|m| !m.is_empty())?.to_ascii_lowercase();
        self.model.get(&model).or_else(|| {
            self.model
                .iter()
                .find(|(alias, _)| alias.contains(&model) || model.contains(alias.as_str()))
                .map(|(_, window)| window)
        })
    }

    /// The window `key` names in the wall vocabulary (contract
    /// `codingSessionBlocked.windows`): `session`, `weekly`, or `model` — the
    /// latter being the WORST model window, since the wall names no alias.
    fn by_key(&self, key: &str, now_ms: i64) -> Option<Window> {
        match key {
            "session" => self.session.clone(),
            "weekly" => self.weekly.clone(),
            "model" => self
                .model
                .values()
                .max_by_key(|window| window.effective_percent(now_ms))
                .cloned(),
            _ => None,
        }
    }
}

/// One account profile of one agent on this device, with its usage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileUsage {
    /// A device-local profile id (`system` = the ambient login).
    pub profile_id: String,
    pub agent: CodingAgent,
    pub signed_in: bool,
    pub health: Health,
    pub windows: UsageWindows,
    /// The profile's own label (`Default`, `Work`) — what the devices row
    /// shows. Rides every message that leaves the device (the workflow
    /// event trail, a team shape); never a decision input.
    pub label: String,
    /// The login's email when the probe named one. Local only: the run's
    /// own switch note and this device's log lines.
    pub email: Option<String>,
}

impl ProfileUsage {
    fn eligible(&self, agent: CodingAgent) -> bool {
        self.agent == agent && self.signed_in && self.health == Health::Ok
    }

    /// What a person on THIS device calls the login: the email when known,
    /// else the label.
    fn local_name(&self) -> &str {
        self.email.as_deref().unwrap_or(&self.label)
    }

    /// The ordering key — lowest 5h percent, then weekly, then the model
    /// window the run needs. Windows a profile does not report count as
    /// empty.
    fn headroom_key(&self, model: Option<&str>, now_ms: i64) -> (u8, u8, u8) {
        let pct = |window: Option<&Window>| {
            window.map(|w| w.effective_percent(now_ms)).unwrap_or(0)
        };
        (
            pct(self.windows.session.as_ref()),
            pct(self.windows.weekly.as_ref()),
            pct(self.windows.model_window(model)),
        )
    }

    /// Any window a run on `model` would spend sits at 100 % with its
    /// reset still ahead.
    fn spent_for(&self, model: Option<&str>, now_ms: i64) -> bool {
        self.windows.session.as_ref().is_some_and(|w| w.spent(now_ms))
            || self.windows.weekly.as_ref().is_some_and(|w| w.spent(now_ms))
            || self.windows.model_window(model).is_some_and(|w| w.spent(now_ms))
    }
}

/// The brake on a rotating run: no rotation before `cooldown_until` (unix
/// ms), and at most `cap` rotations per wall.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RotationGuard {
    pub cooldown_until: Option<i64>,
    pub rotations_this_wall: u32,
    pub cap: u32,
}

/// How long after a rotation the SAME run chain is left alone, whatever its
/// new account reports: a switch replays the transcript into the new login,
/// and a second hop inside this window would only be that replay's own cost
/// walling the next account too.
pub const ROTATION_COOLDOWN_MS: i64 = 10 * 60 * 1000;

/// Rotations one run chain may take inside [`ROTATION_WINDOW_MS`] before it
/// waits for a reset instead — the thrash cap.
pub const ROTATIONS_PER_WALL: u32 = 3;

/// The rolling window the cap counts in (the 5h window the wall usually is).
pub const ROTATION_WINDOW_MS: i64 = 5 * 60 * 60 * 1000;

/// After a probe found NO target, how long the run is left to wait before
/// the host spends another forced collection on it — unless the wall's own
/// reset comes first.
pub const NO_TARGET_RETRY_MS: i64 = 15 * 60 * 1000;

/// How long a chain's state outlives its last LIVE sighting. A switch ends
/// the old row and registers the resumed one only after `coding::prepare`
/// (a git fetch, a token mint: seconds), so a beat landing in that gap sees
/// no session on the worktree — dropping the state there would void the
/// cooldown and the cap on every desktop switch.
pub const CHAIN_GRACE_MS: i64 = 10 * 60 * 1000;

/// Whether `windows` on `agent` leave the pick to the picker at all: only
/// claude rotates (codex keeps one login per session — interface E).
pub fn rotates(agent: CodingAgent) -> bool {
    agent == CodingAgent::Claude
}

/// EXP-1005 — whether THIS device handles a wall on `agent` itself (rotates
/// the run between turns), which is what tells the server to send the owner
/// no rate-limit notification (`setBlocked { handled }`). True only when
/// rotation is on, the agent rotates AND `profiles` hold at least two
/// eligible logins (signed in, healthy): a one-login machine has nowhere to
/// move the run, so its owner hears about the wall (EXP-980), as codex's
/// always does — throttled server-side to one per profile per hour.
pub fn wall_handled_by(
    agent: CodingAgent,
    settings: &crate::settings::Settings,
    profiles: &[ProfileUsage],
) -> bool {
    settings.auto_rotate_accounts
        && rotates(agent)
        && profiles.iter().filter(|profile| profile.eligible(agent)).count() >= 2
}

/// [`wall_handled_by`] off the usage cache under `data_dir`.
pub fn wall_handled_here(
    agent: CodingAgent,
    settings: &crate::settings::Settings,
    data_dir: &Path,
) -> bool {
    wall_handled_by(
        agent,
        settings,
        &crate::agent_usage::profile_usage_snapshot(agent, data_dir),
    )
}

/// The account a fresh start should run on: the signed-in, healthy profile
/// of `agent` with the MOST HEADROOM — lowest session (5h) percent, then
/// weekly, then the `model` window — skipping any profile whose window sits
/// at 100 % with a reset still in the future. `None` = keep the launch's own
/// account (no eligible profile at all).
///
/// Ties keep the listing order, so a caller that lists the launch's own
/// account first keeps it unless another login has STRICTLY more headroom.
pub fn pick_start_account(
    profiles: &[ProfileUsage],
    agent: CodingAgent,
    model: Option<&str>,
    now_ms: i64,
) -> Option<String> {
    if !rotates(agent) {
        return None;
    }
    profiles
        .iter()
        .filter(|profile| profile.eligible(agent) && !profile.spent_for(model, now_ms))
        .min_by_key(|profile| profile.headroom_key(model, now_ms))
        .map(|profile| profile.profile_id.clone())
}

/// The profile a run of `agent` (on `model`) that hit `window_hit` on
/// `current` should move to: never one that hit the SAME window inside its
/// own reset, never one spent on ANY window the run would draw on (a
/// candidate open on the 5h window but at 100 % weekly walls the moment the
/// transcript is replayed into it, and the next probe would hop straight
/// back — the ping-pong the cooldown alone cannot stop), never while
/// `guard.cooldown_until` is in the future or `rotations_this_wall >= cap`.
/// `None` = wait for the reset.
///
/// `window_hit` is the wall's own vocabulary (`session` | `weekly` |
/// `model`); a window a candidate does not report counts as open. Among the
/// candidates the one with the most headroom on the HIT window wins, the
/// overall headroom breaking ties.
pub fn pick_rotation_target(
    current: &str,
    agent: CodingAgent,
    profiles: &[ProfileUsage],
    window_hit: &str,
    model: Option<&str>,
    guard: &RotationGuard,
    now_ms: i64,
) -> Option<String> {
    if guard.cooldown_until.is_some_and(|until| now_ms < until) {
        return None;
    }
    if guard.rotations_this_wall >= guard.cap {
        return None;
    }
    if !rotates(agent) {
        return None;
    }
    profiles
        .iter()
        .filter(|profile| profile.profile_id != current && profile.eligible(agent))
        .filter(|profile| !profile.spent_for(model, now_ms))
        .filter(|profile| {
            !profile
                .windows
                .by_key(window_hit, now_ms)
                .is_some_and(|window| window.spent(now_ms))
        })
        .min_by_key(|profile| {
            let hit = profile
                .windows
                .by_key(window_hit, now_ms)
                .map(|window| window.effective_percent(now_ms))
                .unwrap_or(0);
            (hit, profile.headroom_key(model, now_ms))
        })
        .map(|profile| profile.profile_id.clone())
}

// ---------------------------------------------------------------------------
// The start pick, as `coding::prepare` applies it
// ---------------------------------------------------------------------------

/// What the start pick changed, for the log line and the workflow event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartPick {
    /// The profile the launch named (`system` for the ambient login).
    pub from: String,
    /// The profile it runs on instead.
    pub to: String,
    pub to_label: String,
    /// One sentence: which account, why (the log line, the run note and
    /// the `account_picked` event) — profile labels only, the event is a
    /// team shape.
    pub message: String,
}

impl StartPick {
    /// The line the RUN carries (prefixed to its seed prompt, so the hop is
    /// visible in the transcript and the agent knows nothing else changed).
    pub fn run_note(&self) -> String {
        format!("Note: Exponential moved this run to another account before it started — {}.", self.message)
    }
}

/// The pick for a launch on `agent`/`model` whose options name `account`
/// (`None` = the ambient login), off the usage cache. `None` = the launch's
/// own account stands: rotation is off, the agent never rotates, no
/// eligible profile has more headroom, or the launch's account IS the pick.
///
/// The launch's own account is listed FIRST, so a tie keeps it.
pub fn start_pick(
    profiles: &[ProfileUsage],
    auto_rotate: bool,
    agent: CodingAgent,
    model: Option<&str>,
    account: Option<&str>,
    now_ms: i64,
) -> Option<StartPick> {
    if !auto_rotate || !rotates(agent) {
        return None;
    }
    let from = crate::agent_profiles::profile_id(account);
    let mut ordered: Vec<&ProfileUsage> = profiles.iter().collect();
    ordered.sort_by_key(|profile| profile.profile_id != from);
    let ordered: Vec<ProfileUsage> = ordered.into_iter().cloned().collect();
    let to = pick_start_account(&ordered, agent, model, now_ms)?;
    if to == from {
        return None;
    }
    let target = ordered.iter().find(|profile| profile.profile_id == to)?;
    // A launch account the usage cache does not list (a login past
    // MAX_USAGE_PROFILES, an API-key login that is never monitored) is not
    // known to be walled: the launch keeps it rather than blaming a sign-in.
    let own = ordered.iter().find(|profile| profile.profile_id == from)?;
    let reason = if own.spent_for(model, now_ms) {
        let (key, window) = spent_window(own, model, now_ms);
        format!(
            "{} hit its {} limit{}",
            own.label,
            window_label(key),
            until_suffix(window.and_then(|w| w.resets_at))
        )
    } else {
        format!("{} has less headroom", own.label)
    };
    let message = format!("Starting on {} — {reason}", target.label);
    Some(StartPick {
        from,
        to: to.clone(),
        to_label: target.label.clone(),
        message,
    })
}

/// Apply [`start_pick`] to a launch: rewrite `account` in place and hand
/// back what changed. The ONE call `coding::prepare` makes for every fresh
/// issue, batch and action launch on the device. An account the launch
/// named explicitly (a composer or automation pick) is kept only on a
/// headroom TIE: a login with strictly more headroom overrides it.
pub fn apply_start_pick(
    account: &mut Option<String>,
    profiles: &[ProfileUsage],
    auto_rotate: bool,
    agent: CodingAgent,
    model: Option<&str>,
    now_ms: i64,
) -> Option<StartPick> {
    let pick = start_pick(profiles, auto_rotate, agent, model, account.as_deref(), now_ms)?;
    *account = (!crate::agent_profiles::is_system(Some(&pick.to))).then(|| pick.to.clone());
    Some(pick)
}

fn spent_window<'a>(
    profile: &'a ProfileUsage,
    model: Option<&str>,
    now_ms: i64,
) -> (&'static str, Option<&'a Window>) {
    if profile.windows.session.as_ref().is_some_and(|w| w.spent(now_ms)) {
        ("session", profile.windows.session.as_ref())
    } else if profile.windows.weekly.as_ref().is_some_and(|w| w.spent(now_ms)) {
        ("weekly", profile.windows.weekly.as_ref())
    } else {
        ("model", profile.windows.model_window(model))
    }
}

/// The wall vocabulary as a person reads it.
pub fn window_label(key: &str) -> &'static str {
    match key {
        "session" => "5h",
        "weekly" => "weekly",
        "model" => "model",
        _ => "usage",
    }
}

/// ` (resets 23:10)` in local time, or nothing when the reset is unknown.
pub fn until_suffix(resets_at_ms: Option<i64>) -> String {
    resets_at_ms
        .and_then(local_hhmm)
        .map(|at| format!(" (resets {at})"))
        .unwrap_or_default()
}

fn local_hhmm(unix_ms: i64) -> Option<String> {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_millis_opt(unix_ms)
        .single()
        .map(|at| at.format("%H:%M").to_string())
}

// ---------------------------------------------------------------------------
// The wall beat — what a host runs on its tick
// ---------------------------------------------------------------------------

/// One live run behind its agent's usage wall, as the host reads it off the
/// engine session (`blocked()`, the turn slot) and its own registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalledRun {
    pub session_id: String,
    /// The run's worktree — the identity of the run CHAIN: a switch ends this
    /// row and resumes a new one in the same tree, and the cooldown and cap
    /// must follow the chain, not the row.
    pub chain_key: String,
    pub agent: CodingAgent,
    /// The profile the run is ON (`system` for the ambient login).
    pub account: String,
    /// The model the run spends (the run registry's), for the model window.
    pub model: Option<String>,
    /// `blocked.window` — `session` | `weekly` | `model`.
    pub window: String,
    /// `blocked.resetsAt`, unix ms, when the agent named one.
    pub resets_at_ms: Option<i64>,
    /// The turn slot: `true` = between turns.
    pub idle: bool,
    /// No clone to re-create a worktree from (a repo-less scratch run, or a
    /// run this host has no record of): the switch's end purges the run
    /// and its resume has nowhere to go, so it waits for the reset.
    pub repo_less: bool,
}

/// Why a walled run is NOT being rotated right now — logged once per change
/// by the host, never shown as an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hold {
    /// `Settings.auto_rotate_accounts` is off on this device.
    Off,
    /// Codex: one login per session by contract; it waits for the reset.
    AgentNeverRotates,
    /// A repo-less run: ending it purges its dir, so it cannot be resumed
    /// on another account. It waits for the reset.
    ScratchRun,
    /// A turn is in flight — the switch waits for it to finish.
    MidTurn,
    /// Rotated recently; the chain is left alone until `until_ms`.
    CoolingDown { until_ms: i64 },
    /// The cap for this chain is spent; it waits for the reset.
    Capped,
    /// A probe found no target; the next one is due at `until_ms`.
    WaitingForReset { until_ms: i64 },
}

/// What the beat wants the host to do for one walled run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Spend a forced usage read on `agent`'s profiles and call
    /// [`RotationTracker::decide`] with the result.
    Probe,
    Hold(Hold),
}

/// The outcome of a probe for one walled run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Resume the run on `target` (a profile id), with `prompt` as the
    /// resume's own first message and `event_message` for a workflow's
    /// audit trail.
    Switch {
        target: String,
        target_label: String,
        prompt: String,
        event_message: String,
    },
    /// No profile has headroom on the window the run hit: stay walled until
    /// `until_ms` (the wall's reset, or the retry spacing).
    Wait { until_ms: i64, event_message: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ChainState {
    /// Unix ms of every rotation inside the cap window, oldest first.
    rotations: Vec<i64>,
    /// A probe found nothing: no new probe before this.
    no_target_until: Option<i64>,
    /// The wall a `no_target` verdict was given for, so a NEW wall (another
    /// window, another account) probes at once.
    no_target_wall: Option<(String, String)>,
    /// Unix ms of the last beat that saw a live run on this chain.
    last_live_ms: i64,
}

impl ChainState {
    /// The newest moment this chain still matters from: its last rotation,
    /// its park, or its last live sighting.
    fn last_touched_ms(&self) -> i64 {
        self.rotations
            .iter()
            .copied()
            .chain(self.no_target_until)
            .chain(Some(self.last_live_ms))
            .max()
            .unwrap_or(0)
    }
}

/// The on-disk form of the chains: `{"chains": {"<chain key>": ...}}`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredChains {
    chains: BTreeMap<String, ChainState>,
}

/// The chain memory's file, beside `agent-usage.json`.
const STORE_FILE: &str = "account-rotation.json";

fn store_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(STORE_FILE)
}

fn store_lock_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(format!("{STORE_FILE}.lock"))
}

fn read_stored(data_dir: &Path) -> StoredChains {
    std::fs::read_to_string(store_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// The per-host memory the beat needs: one entry per run chain. The cooldown
/// and the cap are PERSISTED ([`Self::load`], [`Self::save`]): a daemon's
/// self-update re-exec or an IDE restart must not hand a thrashing chain a
/// fresh cap.
#[derive(Clone, Debug, Default)]
pub struct RotationTracker {
    chains: BTreeMap<String, ChainState>,
}

impl RotationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// The tracker as the store under `data_dir` remembers it, pruned to
    /// the chains touched inside [`ROTATION_WINDOW_MS`]: anything older
    /// cannot count against a cap or a cooldown any more. A missing or
    /// unreadable file is an empty tracker.
    pub fn load(data_dir: &Path, now_ms: i64) -> Self {
        let _guard = api::settings_lock::locked_at(&store_lock_path(data_dir));
        let mut chains = read_stored(data_dir).chains;
        chains.retain(|_, state| now_ms - state.last_touched_ms() < ROTATION_WINDOW_MS);
        Self { chains }
    }

    /// Persist this tracker's chains: a read-modify-write under the store's
    /// lock, so the daemon and the IDE (one data dir, REV-20) each keep the
    /// chains they host without clobbering the other's. Chains nobody
    /// touched inside [`ROTATION_WINDOW_MS`] are dropped on the way.
    /// Best-effort: a failed write costs at most one extra rotation after a
    /// restart.
    pub fn save(&self, data_dir: &Path, now_ms: i64) {
        let _guard = api::settings_lock::locked_at(&store_lock_path(data_dir));
        let mut stored = read_stored(data_dir);
        for (key, state) in &self.chains {
            stored.chains.insert(key.clone(), state.clone());
        }
        stored
            .chains
            .retain(|_, state| now_ms - state.last_touched_ms() < ROTATION_WINDOW_MS);
        let Ok(mut json) = serde_json::to_string_pretty(&stored) else {
            return;
        };
        json.push('\n');
        if let Err(err) = api::atomic_file::write_atomic(&store_path(data_dir), &json) {
            log::warn!("account rotation: {} could not be written: {err}", STORE_FILE);
        }
    }

    /// The guard for `chain_key` as [`pick_rotation_target`] reads it.
    pub fn guard(&self, chain_key: &str, now_ms: i64) -> RotationGuard {
        let state = self.chains.get(chain_key);
        let recent = state
            .map(|state| {
                state
                    .rotations
                    .iter()
                    .filter(|at| now_ms - **at < ROTATION_WINDOW_MS)
                    .count() as u32
            })
            .unwrap_or(0);
        let cooldown_until = state
            .and_then(|state| state.rotations.last().copied())
            .map(|last| last + ROTATION_COOLDOWN_MS)
            .filter(|until| now_ms < *until);
        RotationGuard {
            cooldown_until,
            rotations_this_wall: recent,
            cap: ROTATIONS_PER_WALL,
        }
    }

    /// What to do about `run` this beat.
    pub fn step(&self, run: &WalledRun, auto_rotate: bool, now_ms: i64) -> Step {
        if !auto_rotate {
            return Step::Hold(Hold::Off);
        }
        if !rotates(run.agent) {
            return Step::Hold(Hold::AgentNeverRotates);
        }
        if run.repo_less {
            return Step::Hold(Hold::ScratchRun);
        }
        if !run.idle {
            return Step::Hold(Hold::MidTurn);
        }
        let guard = self.guard(&run.chain_key, now_ms);
        if let Some(until_ms) = guard.cooldown_until {
            return Step::Hold(Hold::CoolingDown { until_ms });
        }
        if guard.rotations_this_wall >= guard.cap {
            return Step::Hold(Hold::Capped);
        }
        if let Some(state) = self.chains.get(&run.chain_key) {
            let same_wall = state.no_target_wall.as_ref()
                == Some(&(run.account.clone(), run.window.clone()));
            if let (true, Some(until_ms)) = (same_wall, state.no_target_until) {
                if now_ms < until_ms {
                    return Step::Hold(Hold::WaitingForReset { until_ms });
                }
            }
        }
        Step::Probe
    }

    /// Fold a forced read into a decision for `run`, and remember it: a
    /// switch counts against the chain's cap and starts its cooldown; a wait
    /// parks the chain until the wall's reset or [`NO_TARGET_RETRY_MS`],
    /// whichever is sooner.
    pub fn decide(&mut self, run: &WalledRun, profiles: &[ProfileUsage], now_ms: i64) -> Decision {
        let guard = self.guard(&run.chain_key, now_ms);
        let state = self.chains.entry(run.chain_key.clone()).or_default();
        state.last_live_ms = now_ms;
        let from = profiles.iter().find(|profile| profile.profile_id == run.account);
        let from_label = from.map(|profile| profile.label.clone()).unwrap_or_else(|| run.account.clone());
        let from_name = from.map(|profile| profile.local_name().to_string()).unwrap_or_else(|| run.account.clone());
        match pick_rotation_target(
            &run.account,
            run.agent,
            profiles,
            &run.window,
            run.model.as_deref(),
            &guard,
            now_ms,
        ) {
            Some(target) => {
                let to = profiles.iter().find(|profile| profile.profile_id == target);
                let target_label = to.map(|profile| profile.label.clone()).unwrap_or_else(|| target.clone());
                let target_name = to.map(|profile| profile.local_name().to_string()).unwrap_or_else(|| target.clone());
                state.rotations.retain(|at| now_ms - *at < ROTATION_WINDOW_MS);
                state.rotations.push(now_ms);
                state.no_target_until = None;
                state.no_target_wall = None;
                Decision::Switch {
                    // The run's own note may name the login; the event may not.
                    prompt: switch_prompt(&from_name, &target_name, &run.window, run.resets_at_ms),
                    event_message: switch_event_message(
                        &from_label,
                        &target_label,
                        &run.window,
                        run.resets_at_ms,
                    ),
                    target,
                    target_label,
                }
            }
            None => {
                let retry = now_ms + NO_TARGET_RETRY_MS;
                let until_ms = run
                    .resets_at_ms
                    .filter(|reset| *reset > now_ms)
                    .map_or(retry, |reset| reset.min(retry));
                state.no_target_until = Some(until_ms);
                state.no_target_wall = Some((run.account.clone(), run.window.clone()));
                Decision::Wait {
                    until_ms,
                    event_message: waiting_event_message(&run.window, run.resets_at_ms),
                }
            }
        }
    }

    /// Note which chains have a LIVE run this beat, and forget the ones no
    /// run has been seen on for [`CHAIN_GRACE_MS`] — never the ones merely
    /// between an ended row and its resumed successor.
    pub fn retain_chains(&mut self, live: &[String], now_ms: i64) {
        for key in live {
            if let Some(state) = self.chains.get_mut(key) {
                state.last_live_ms = now_ms;
            }
        }
        self.chains
            .retain(|_, state| now_ms - state.last_live_ms <= CHAIN_GRACE_MS);
    }
}

/// The resume's own first message on a rotation: what happened and why,
/// then `prompt::ACCOUNT_SWITCH_CONTINUE_PROMPT` (the launcher appends it to
/// every switch resume). Read by the agent AND shown in the run.
pub fn switch_prompt(from: &str, to: &str, window: &str, resets_at_ms: Option<i64>) -> String {
    format!(
        "Exponential moved this run from {from} to {to}: the {} window hit its limit{}.",
        window_label(window),
        until_suffix(resets_at_ms)
    )
}

/// The `account_switched` workflow event line.
pub fn switch_event_message(
    from: &str,
    to: &str,
    window: &str,
    resets_at_ms: Option<i64>,
) -> String {
    format!(
        "Moved from {from} to {to} after the {} window hit its limit{}",
        window_label(window),
        until_suffix(resets_at_ms)
    )
}

/// The `waiting_reset` workflow event line.
pub fn waiting_event_message(window: &str, resets_at_ms: Option<i64>) -> String {
    format!(
        "No account with headroom on the {} window — waiting for the reset{}",
        window_label(window),
        until_suffix(resets_at_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000;

    fn profile(id: &str, session: u8, weekly: u8) -> ProfileUsage {
        ProfileUsage {
            profile_id: id.to_string(),
            agent: CodingAgent::Claude,
            signed_in: true,
            health: Health::Ok,
            windows: UsageWindows {
                session: Some(Window {
                    percent: session,
                    resets_at: Some(NOW + 3_600_000),
                }),
                weekly: Some(Window {
                    percent: weekly,
                    resets_at: Some(NOW + 86_400_000),
                }),
                model: BTreeMap::new(),
            },
            label: id.to_string(),
            email: Some(format!("{id}@example.com")),
        }
    }

    fn walled(account: &str, window: &str, idle: bool) -> WalledRun {
        WalledRun {
            session_id: "s1".to_string(),
            chain_key: "/tmp/wt".to_string(),
            agent: CodingAgent::Claude,
            account: account.to_string(),
            model: None,
            window: window.to_string(),
            resets_at_ms: Some(NOW + 3_600_000),
            idle,
            repo_less: false,
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("exp-rotation-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn settings(auto_rotate: bool) -> crate::settings::Settings {
        crate::settings::Settings {
            auto_rotate_accounts: auto_rotate,
            ..crate::settings::Settings::default()
        }
    }

    #[test]
    fn start_picks_the_profile_with_the_most_headroom() {
        // (profiles, expected): lowest session first, weekly breaks a tie,
        // unhealthy and spent-until-reset profiles are skipped.
        let mut unhealthy = profile("c", 0, 0);
        unhealthy.health = Health::NeedsRelogin;
        let table: Vec<(Vec<ProfileUsage>, Option<&str>)> = vec![
            (vec![profile("a", 80, 10), profile("b", 20, 90)], Some("b")),
            (vec![profile("a", 20, 50), profile("b", 20, 10)], Some("b")),
            (vec![profile("a", 60, 10), unhealthy], Some("a")),
            (vec![profile("a", 100, 10)], None),
            (Vec::new(), None),
        ];
        for (profiles, expected) in table {
            assert_eq!(
                pick_start_account(&profiles, CodingAgent::Claude, None, NOW).as_deref(),
                expected
            );
        }
    }

    #[test]
    fn a_passed_reset_reopens_a_spent_window() {
        let mut spent = profile("a", 100, 10);
        spent.windows.session.as_mut().unwrap().resets_at = Some(NOW - 1);
        assert_eq!(
            pick_start_account(&[spent], CodingAgent::Claude, None, NOW).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn the_model_window_counts_for_the_run_that_needs_it() {
        let mut a = profile("a", 10, 10);
        a.windows.model.insert(
            "fable".to_string(),
            Window { percent: 100, resets_at: Some(NOW + 1000) },
        );
        let b = profile("b", 30, 30);
        // A fable run skips a; an opus run keeps a (most headroom).
        assert_eq!(
            pick_start_account(&[a.clone(), b.clone()], CodingAgent::Claude, Some("fable"), NOW)
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            pick_start_account(&[a, b], CodingAgent::Claude, Some("opus"), NOW).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn codex_never_rotates() {
        let mut codex = profile("a", 0, 0);
        codex.agent = CodingAgent::Codex;
        assert_eq!(pick_start_account(&[codex.clone()], CodingAgent::Codex, None, NOW), None);
        let mut other = codex.clone();
        other.profile_id = "b".to_string();
        assert_eq!(
            pick_rotation_target(
                "a",
                CodingAgent::Codex,
                &[codex, other],
                "session",
                None,
                &RotationGuard { cap: 3, ..Default::default() },
                NOW
            ),
            None
        );
    }

    #[test]
    fn rotation_respects_the_guard_and_the_same_window() {
        let profiles = vec![
            profile("a", 100, 10),
            profile("b", 100, 10),
            profile("c", 5, 5),
        ];
        let open = RotationGuard {
            cooldown_until: None,
            rotations_this_wall: 0,
            cap: 3,
        };
        let pick = |current: &str, profiles: &[ProfileUsage], window: &str, guard: &RotationGuard| {
            pick_rotation_target(current, CodingAgent::Claude, profiles, window, None, guard, NOW)
        };
        // Into the one profile that did NOT hit the session window.
        assert_eq!(pick("a", &profiles, "session", &open).as_deref(), Some("c"));
        // A candidate spent on ANOTHER window is no target either: the
        // replay would wall it at once and the next probe would hop back.
        assert_eq!(
            pick("a", &[profile("a", 100, 10), profile("b", 10, 100)], "session", &open),
            None
        );
        // Cooling down, or at the cap: stay.
        let cooling = RotationGuard {
            cooldown_until: Some(NOW + 1),
            ..open.clone()
        };
        assert_eq!(pick("a", &profiles, "session", &cooling), None);
        let capped = RotationGuard {
            rotations_this_wall: 3,
            ..open
        };
        assert_eq!(pick("a", &profiles, "session", &capped), None);
    }

    #[test]
    fn rotation_weighs_the_hit_window_first() {
        // b has the lower 5h number but the weekly wall is what was hit:
        // c's weekly headroom wins.
        let profiles = vec![profile("a", 10, 100), profile("b", 5, 60), profile("c", 40, 10)];
        let open = RotationGuard { cap: 3, ..Default::default() };
        let pick = |current: &str, profiles: &[ProfileUsage], window: &str, model: Option<&str>| {
            pick_rotation_target(current, CodingAgent::Claude, profiles, window, model, &open, NOW)
        };
        assert_eq!(pick("a", &profiles, "weekly", None).as_deref(), Some("c"));
        // A `model` wall: the candidate's WORST model window must be open.
        let mut d = profile("d", 0, 0);
        d.windows.model.insert("fable".into(), Window { percent: 100, resets_at: Some(NOW + 1) });
        let mut e = profile("e", 50, 50);
        e.windows.model.insert("fable".into(), Window { percent: 20, resets_at: Some(NOW + 1) });
        assert_eq!(
            pick("a", &[profile("a", 0, 0), d.clone(), e.clone()], "model", None).as_deref(),
            Some("e")
        );
        // The run's OWN model window counts on a session wall too: a fable
        // run skips a candidate whose fable window is spent.
        let mut f = profile("f", 30, 30);
        f.windows.model.insert("fable".into(), Window { percent: 100, resets_at: Some(NOW + 1) });
        assert_eq!(
            pick("a", &[profile("a", 100, 0), f.clone(), e.clone()], "session", Some("fable")).as_deref(),
            Some("e")
        );
        assert_eq!(
            pick("a", &[profile("a", 100, 0), f], "session", Some("opus")).as_deref(),
            Some("f")
        );
    }

    #[test]
    fn start_pick_keeps_the_launch_account_on_a_tie_and_says_why_it_moved() {
        let profiles = vec![profile("system", 20, 10), profile("b", 20, 10)];
        assert_eq!(start_pick(&profiles, true, CodingAgent::Claude, None, None, NOW), None);
        // Off, or codex: nothing.
        assert_eq!(start_pick(&profiles, false, CodingAgent::Claude, None, None, NOW), None);
        assert_eq!(start_pick(&profiles, true, CodingAgent::Codex, None, None, NOW), None);
        // The default is walled: move, and say so.
        let walled = vec![profile("system", 100, 10), profile("b", 20, 10)];
        let pick = start_pick(&walled, true, CodingAgent::Claude, None, None, NOW).unwrap();
        assert_eq!(pick.from, "system");
        assert_eq!(pick.to, "b");
        // Labels, never emails: the message reaches the workflow's event trail.
        assert!(pick.run_note().starts_with("Note: Exponential moved this run to another account before it started — Starting on b"), "{}", pick.run_note());
        assert!(pick.message.starts_with("Starting on b — system hit its 5h limit"), "{}", pick.message);
        assert!(!pick.message.contains("@example.com"), "{}", pick.message);
        // Less headroom, not walled: move too (Danny: most headroom, always).
        let less = vec![profile("system", 60, 10), profile("b", 20, 10)];
        let pick = start_pick(&less, true, CodingAgent::Claude, None, None, NOW).unwrap();
        assert!(pick.message.ends_with("has less headroom"), "{}", pick.message);
        // Applied: the ambient login becomes `None`, a profile its id.
        let mut account = Some("b".to_string());
        let back = vec![profile("b", 90, 10), profile("system", 5, 5)];
        let pick = apply_start_pick(&mut account, &back, true, CodingAgent::Claude, None, NOW).unwrap();
        assert_eq!(pick.to, "system");
        assert_eq!(account, None);
    }

    #[test]
    fn the_tracker_holds_probes_and_counts_rotations_per_chain() {
        let mut tracker = RotationTracker::new();
        let run = walled("a", "session", true);
        assert_eq!(tracker.step(&run, false, NOW), Step::Hold(Hold::Off));
        assert_eq!(
            tracker.step(&walled("a", "session", false), true, NOW),
            Step::Hold(Hold::MidTurn)
        );
        let mut codex = run.clone();
        codex.agent = CodingAgent::Codex;
        assert_eq!(tracker.step(&codex, true, NOW), Step::Hold(Hold::AgentNeverRotates));
        assert_eq!(tracker.step(&run, true, NOW), Step::Probe);

        let profiles = vec![profile("a", 100, 10), profile("b", 10, 10)];
        let Decision::Switch { target, prompt, event_message, .. } = tracker.decide(&run, &profiles, NOW) else {
            panic!("expected a switch");
        };
        assert_eq!(target, "b");
        // The run's own note names the logins; the workflow event (a team
        // shape) names the profiles' labels only.
        assert!(prompt.starts_with("Exponential moved this run from a@example.com to b@example.com: the 5h window hit its limit"), "{prompt}");
        assert!(event_message.starts_with("Moved from a to b after the 5h window hit its limit"), "{event_message}");
        assert!(!event_message.contains('@'), "{event_message}");
        // The chain keeps its state while its run is momentarily absent (the
        // switch ended the row, the resume has not registered yet) — only a
        // chain unseen past the grace is forgotten.
        tracker.retain_chains(&[], NOW + 1);
        assert!(matches!(
            tracker.step(&run, true, NOW + 2),
            Step::Hold(Hold::CoolingDown { .. })
        ), "a beat with no live run on the chain keeps the cooldown");
        tracker.retain_chains(&[run.chain_key.clone()], NOW + 3);
        let mut lost = tracker.clone();
        lost.retain_chains(&[], NOW + 3 + CHAIN_GRACE_MS + 1);
        assert_eq!(lost.step(&run, true, NOW + 4 + CHAIN_GRACE_MS), Step::Probe);
        // The chain cools down, then counts toward the cap.
        assert!(matches!(
            tracker.step(&run, true, NOW + 1),
            Step::Hold(Hold::CoolingDown { .. })
        ));
        let later = NOW + ROTATION_COOLDOWN_MS;
        assert_eq!(tracker.step(&run, true, later), Step::Probe);
        for i in 1..ROTATIONS_PER_WALL as i64 {
            let at = NOW + i * ROTATION_COOLDOWN_MS;
            assert!(matches!(tracker.decide(&run, &profiles, at), Decision::Switch { .. }));
        }
        let at = NOW + ROTATIONS_PER_WALL as i64 * ROTATION_COOLDOWN_MS;
        assert_eq!(tracker.step(&run, true, at), Step::Hold(Hold::Capped));
        // Past the cap window the chain rotates again.
        assert_eq!(tracker.step(&run, true, NOW + ROTATION_WINDOW_MS + 1), Step::Probe);
        // Another chain is untouched.
        let mut other = run.clone();
        other.chain_key = "/tmp/other".to_string();
        assert_eq!(tracker.step(&other, true, at), Step::Probe);
    }

    #[test]
    fn no_target_parks_the_chain_until_the_reset_or_the_retry_spacing() {
        let mut tracker = RotationTracker::new();
        let run = walled("a", "session", true);
        let spent = vec![profile("a", 100, 10), profile("b", 100, 10)];
        let Decision::Wait { until_ms, event_message } = tracker.decide(&run, &spent, NOW) else {
            panic!("expected a wait");
        };
        // The wall resets in an hour; the retry spacing is shorter.
        assert_eq!(until_ms, NOW + NO_TARGET_RETRY_MS);
        assert!(event_message.starts_with("No account with headroom on the 5h window"), "{event_message}");
        assert_eq!(
            tracker.step(&run, true, NOW + 1),
            Step::Hold(Hold::WaitingForReset { until_ms })
        );
        assert_eq!(tracker.step(&run, true, until_ms), Step::Probe);
        // A different wall (the run moved windows) probes at once.
        assert_eq!(tracker.step(&walled("a", "weekly", true), true, NOW + 1), Step::Probe);
        // A sooner reset wins over the spacing.
        let mut soon = run.clone();
        soon.resets_at_ms = Some(NOW + 1000);
        let Decision::Wait { until_ms, .. } = tracker.decide(&soon, &spent, NOW) else {
            panic!("expected a wait");
        };
        assert_eq!(until_ms, NOW + 1000);
    }

    #[test]
    fn the_messages_name_account_window_and_reset() {
        assert_eq!(
            switch_event_message("a@x", "b@x", "weekly", None),
            "Moved from a@x to b@x after the weekly window hit its limit"
        );
        assert!(waiting_event_message("model", Some(NOW)).contains("(resets "));
        assert_eq!(window_label("session"), "5h");
        assert_eq!(until_suffix(None), "");
    }

    /// A repo-less run is never rotated: the switch would purge its scratch
    /// dir and the resume would have nowhere to go. It waits like codex.
    #[test]
    fn a_scratch_run_holds_for_the_reset() {
        let tracker = RotationTracker::new();
        let mut scratch = walled("a", "session", true);
        scratch.repo_less = true;
        assert_eq!(tracker.step(&scratch, true, NOW), Step::Hold(Hold::ScratchRun));
        // The hold ranks after the agent check and before the turn slot: a
        // mid-turn scratch run is still a scratch run.
        scratch.idle = false;
        assert_eq!(tracker.step(&scratch, true, NOW), Step::Hold(Hold::ScratchRun));
        assert_eq!(tracker.step(&walled("a", "session", true), true, NOW), Step::Probe);
    }

    /// `handled` mutes the owner's wall push, so it is true only when this
    /// device can actually move the run: two or more eligible claude logins.
    #[test]
    fn a_wall_is_handled_only_with_two_eligible_logins() {
        let two = vec![profile("a", 100, 10), profile("b", 10, 10)];
        assert!(wall_handled_by(CodingAgent::Claude, &settings(true), &two));
        assert!(!wall_handled_by(CodingAgent::Claude, &settings(false), &two));
        assert!(!wall_handled_by(CodingAgent::Codex, &settings(true), &two));
        // One login, or a second one that is signed out or unhealthy: the
        // owner hears about the wall (EXP-980).
        assert!(!wall_handled_by(CodingAgent::Claude, &settings(true), &two[..1]));
        let mut signed_out = profile("b", 10, 10);
        signed_out.signed_in = false;
        assert!(!wall_handled_by(CodingAgent::Claude, &settings(true), &[profile("a", 100, 10), signed_out]));
        let mut unhealthy = profile("b", 10, 10);
        unhealthy.health = Health::NeedsRelogin;
        assert!(!wall_handled_by(CodingAgent::Claude, &settings(true), &[profile("a", 100, 10), unhealthy]));
        // A spent second login still counts: the cap and the cooldown, not
        // the wall push, decide what happens next.
        assert!(wall_handled_by(CodingAgent::Claude, &settings(true), &[profile("a", 100, 10), profile("b", 100, 100)]));
        assert!(!wall_handled_by(CodingAgent::Claude, &settings(true), &[]));
    }

    /// The cap and the cooldown survive a host restart: the chains are
    /// written after every decision and read back, pruned to the 5h window.
    #[test]
    fn the_chains_survive_a_restart_and_age_out_of_the_window() {
        let dir = temp_dir("persist");
        let run = walled("a", "session", true);
        let profiles = vec![profile("a", 100, 10), profile("b", 10, 10)];
        let mut tracker = RotationTracker::load(&dir, NOW);
        assert!(tracker.chains.is_empty(), "no file yet reads empty");
        assert!(matches!(tracker.decide(&run, &profiles, NOW), Decision::Switch { .. }));
        tracker.save(&dir, NOW);
        assert!(dir.join(STORE_FILE).is_file());
        // A fresh host: still cooling down.
        let restarted = RotationTracker::load(&dir, NOW + 1);
        assert!(matches!(
            restarted.step(&run, true, NOW + 1),
            Step::Hold(Hold::CoolingDown { .. })
        ));
        assert_eq!(restarted.guard(&run.chain_key, NOW + 1).rotations_this_wall, 1);
        // Another host's chain in the same file is kept by a save.
        let mut other = RotationTracker::new();
        let mut elsewhere = run.clone();
        elsewhere.chain_key = "/tmp/other".to_string();
        assert!(matches!(other.decide(&elsewhere, &profiles, NOW + 2), Decision::Switch { .. }));
        other.save(&dir, NOW + 2);
        let both = RotationTracker::load(&dir, NOW + 3);
        assert_eq!(both.chains.len(), 2);
        // Past the window nothing counts any more, and the file shrinks.
        let later = NOW + 3 + ROTATION_WINDOW_MS;
        let aged = RotationTracker::load(&dir, later);
        assert!(aged.chains.is_empty());
        aged.save(&dir, later);
        assert!(read_stored(&dir).chains.is_empty());
        // A corrupt file reads empty rather than failing the beat.
        std::fs::write(dir.join(STORE_FILE), "{not json").unwrap();
        assert!(RotationTracker::load(&dir, NOW).chains.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
