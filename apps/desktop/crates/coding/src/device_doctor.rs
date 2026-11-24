//! EXP-1196/1218/1219: THE device readiness block. The device builds it from
//! its own [`DoctorReport`] plus the computer-use state and sends it on
//! `devices.register` + `devices.heartbeat` (`devices.doctor`, synced); every
//! client renders the same rows from it, and `exponential doctor` prints it.
//!
//! The spec is `packages/domain-contract/fixtures/device-doctor.json`
//! (fixture-locked below): groups, labels, states, actions, copy. The device
//! writes `detail`; clients never compose it.
//!
//! [`build`] is pure (the caller passes the computer state); [`current`] is
//! the host helper that also reads the usage cache (a `needs_relogin` login)
//! and asks [`crate::computer`] what this machine can do.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::agent_accounts::{AgentAccount, Health};
use crate::doctor::{
    parse_claude_version, DoctorReport, ToolCheck, MIN_CLAUDE_ACP_VERSION, MIN_CODEX_ACP_VERSION,
};
use crate::settings::Settings;

pub const KEY_GIT: &str = "git";
pub const KEY_COMPUTER_USE: &str = "computer_use";
/// NEW-78: the voice group's one row (see [`add_microphone`]).
pub const KEY_MICROPHONE: &str = "microphone";
/// NEW-55: the voice group's second row (see [`add_speech`]).
pub const KEY_SPEECH: &str = "speech";
/// S-3: the voice group's wake word row (see [`add_wake`]).
pub const KEY_WAKE: &str = "wake";
/// EXP-1375: the storage group's free-disk row (see [`add_storage`]).
pub const KEY_DISK: &str = "disk";
/// EXP-1375: a kept worktree row's key prefix (`worktree-1`, `worktree-2`, …).
pub const KEY_WORKTREE_PREFIX: &str = "worktree-";
/// NEW-57: the voice group's desktop audio row (see [`add_desktop_audio`]).
pub const KEY_DESKTOP_AUDIO: &str = "desktop_audio";
/// #FEED-192: the storage row of a key holder that is not running.
pub const KEY_KEY_HOLDER: &str = "key_holder";
/// EXP-1794: the storage row of diverged conversation copies the claude
/// store migration parked (see [`add_store_conflicts`]).
pub const KEY_STORE_CONFLICTS: &str = "store_conflicts";
/// #EXP-1865: the storage row of the last shared cargo target sweep (see
/// [`add_cargo_sweep`]).
pub const KEY_CARGO_SWEEP: &str = "cargo_sweep";
/// #EXP-1868: the storage row of a memory-throttled slice (see
/// [`add_memory`]).
pub const KEY_MEMORY: &str = "memory";
/// #EXP-1892: the storage row of writers a full disk refuses (see
/// [`add_disk_writes`]).
pub const KEY_DISK_WRITES: &str = "disk_writes";
/// #EXP-1514: the row under the computer-use switch that says whether this
/// desktop gives a run an agent space (see [`ComputerState::Supported`]).
pub const KEY_AGENT_SPACE: &str = "agent_space";
pub const AGENT_SPACE_LABEL: &str = "Agent space";
/// #EXP-1514: the agent space row's details (device-doctor.json `copy`).
pub const SPACE_READY: &str = "Ready";
pub const SPACE_NEEDS_DESKTOP: &str = "Needs Exponential Desktop";
pub const SPACE_NEEDS_NEWER_DESKTOP: &str = "Update Exponential Desktop";
pub const SPACE_DRIVER: &str = "Not on this driver yet";
/// NEW-55: the speech row when the voice is loaded.
pub const READY: &str = "Ready";
/// NEW-55: the speech row without a usable GPU (grey, no pill).
pub const NO_USABLE_GPU: &str = "No usable GPU";

pub const NOT_INSTALLED: &str = "Not installed";
pub const SIGNED_OUT: &str = "Signed out";
pub const NEEDS_RELOGIN: &str = "Needs re-login";
pub const GRANTED: &str = "Granted";
pub const NOT_GRANTED: &str = "Not granted";
pub const UNAVAILABLE: &str = "Not available here";
/// An agent whose version passes the floor but whose deep ACP probe failed
/// (`exponential doctor`'s codex handshake). Not in the fixture's copy.
pub const NOT_RESPONDING: &str = "Not responding";
/// NEW-78: the microphone row with no input device (grey, no pill).
pub const NO_INPUT_DEVICE: &str = "No input device";
/// NEW-54: Windows without the MSVC OpenMP runtime the CPU transcriber
/// needs (an error with a local-only install).
pub const NEEDS_RUNTIME: &str = "Needs the Visual C++ runtime";
/// EXP-1232: the managed Codex build is being fetched (no pill).
pub const DOWNLOADING: &str = "Downloading…";
/// EXP-1232: the fetch failed; Update re-fetches.
pub const DOWNLOAD_FAILED: &str = "Download failed";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDoctor {
    pub checked_at: String,
    pub items: Vec<DoctorItem>,
    /// NEW-59: the device's displays (`doctor.displays`,
    /// `fixtures/work-screen.json`) in the OS's order, pixels; omitted when
    /// none are reported (a display-less daemon). The Work screen row's
    /// options come from here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub displays: Vec<DoctorDisplay>,
    /// NEW-57: what desktop audio the voice device can capture and the apps
    /// playing audio right now (`meeting.json`); only on the voice device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_audio: Option<DoctorDesktopAudio>,
}

/// NEW-57: `doctor.desktopAudio` (absent = `none` and `[]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorDesktopAudio {
    pub capability: etalk::meeting::DesktopAudioCapability,
    #[serde(default)]
    pub apps: Vec<etalk::meeting::AudioApp>,
}

/// NEW-59: one entry of `doctor.displays`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorDisplay {
    /// #FEED-161: stable per OS (the stored
    /// `launch_defaults.computerUseDisplayId`); blank from a build before it.
    #[serde(default)]
    pub id: String,
    /// 1-based, the OS's order, for the label only.
    pub number: u32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub primary: bool,
}

impl From<&crate::computer::Display> for DoctorDisplay {
    fn from(display: &crate::computer::Display) -> Self {
        // Physical pixels (work-screen.json DISPLAYS): macOS rects are points.
        let (width, height) = crate::computer::pixel_size(display);
        DoctorDisplay {
            id: display.id.clone(),
            number: display.number,
            width,
            height,
            primary: display.primary,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorItem {
    pub key: String,
    pub group: DoctorGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// EXP-1375: the row's own label (a kept worktree's directory name),
    /// replacing the fixture's `labels` entry; only on kept worktree rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub state: DoctorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<DoctorAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorGroup {
    Required,
    Agents,
    ComputerUse,
    /// NEW-78: the microphone row, on the member's voice device only.
    Voice,
    /// EXP-1375: free disk + the worktrees the cleanup kept.
    Storage,
}

impl DoctorGroup {
    pub const ALL: [DoctorGroup; 5] = [
        DoctorGroup::Required,
        DoctorGroup::Agents,
        DoctorGroup::ComputerUse,
        DoctorGroup::Voice,
        DoctorGroup::Storage,
    ];

    /// The fixture's `groups[].label`.
    pub fn label(self) -> &'static str {
        match self {
            DoctorGroup::Required => "Required",
            DoctorGroup::Agents => "Coding agents",
            DoctorGroup::ComputerUse => "Computer use",
            DoctorGroup::Voice => "Voice",
            DoctorGroup::Storage => "Storage",
        }
    }

    /// The fixture's `groups[].tag`.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            DoctorGroup::Required | DoctorGroup::Storage => None,
            DoctorGroup::Agents | DoctorGroup::ComputerUse | DoctorGroup::Voice => {
                Some("optional")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorState {
    Ok,
    Action,
    Missing,
    Off,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorAction {
    Install,
    Update,
    SignIn,
    Grant,
}

impl DoctorAction {
    /// The fixture's `actions[].label`.
    pub fn label(self) -> &'static str {
        match self {
            DoctorAction::Install => "Install",
            DoctorAction::Update => "Update",
            DoctorAction::SignIn => "Sign in",
            DoctorAction::Grant => "Open System Settings",
        }
    }

    /// Whether ANOTHER device may trigger it (the fixture's `remote`).
    pub fn remote(self) -> bool {
        matches!(self, DoctorAction::Update | DoctorAction::SignIn)
    }
}

/// The fixture's `labels` map.
pub fn label(key: &str) -> &str {
    match key {
        "git" => "Git",
        "claude" => "Claude Code",
        "codex" => "Codex",
        "computer_use" => "Computer use",
        "screen_recording" => "Screen Recording",
        "accessibility" => "Accessibility",
        "microphone" => "Microphone",
        "speech" => "Speech",
        "wake" => "Wake word",
        "disk" => "Disk",
        KEY_DESKTOP_AUDIO => &etalk::meeting::MEETING.copy.doctor_label,
        other => other,
    }
}

/// A row's label: its own (EXP-1375: a kept worktree), else [`label`].
pub fn item_label(item: &DoctorItem) -> &str {
    item.label.as_deref().unwrap_or_else(|| label(&item.key))
}

/// What computer use can do on this machine, as [`crate::computer`] says
/// (read by [`ComputerState::read`]; tests build it by hand).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerState {
    /// No backend for this OS/session; the device's reason, when it has one
    /// (a Wayland session, no display), is the row's detail.
    Unsupported(Option<String>),
    /// Supported; each OS permission it needs and whether it is granted
    /// (empty on X11/Windows, which ask for none).
    Supported {
        permissions: Vec<(crate::computer::Permission, bool)>,
        /// #EXP-1514: whether this desktop gives a run an agent space, as
        /// the row shows it ([`space_seen`]: `Unknown` = no answer for three
        /// beats). `None` = no row: macOS and Windows.
        space: Option<crate::computer::SpaceOffer>,
    },
}

impl ComputerState {
    /// Ask the epilot crate, never prompting. The agent space offer is
    /// ePilot's cached answer: never asked of the desktop here, on the beat.
    pub fn read() -> ComputerState {
        match crate::computer::readiness() {
            crate::computer::Readiness::Unsupported(reason) => ComputerState::Unsupported(Some(reason)),
            _ => ComputerState::Supported {
                permissions: crate::computer::permissions(),
                space: cfg!(target_os = "linux").then(|| space_seen(crate::computer::space_offer())),
            },
        }
    }
}

/// #EXP-1514: the agent space offer across reads (beats).
#[derive(Debug, Default)]
struct SpaceSeen {
    /// The last answer the desktop gave.
    last: Option<crate::computer::SpaceOffer>,
    /// Reads in a row without one.
    silent: u32,
    /// What the row showed last (for the log and `exponential doctor`).
    shown: Option<crate::computer::SpaceOffer>,
}

/// After this many reads in a row without an answer the row says Not
/// responding; until then the last answer stands.
const SPACE_SILENT_BEATS: u32 = 3;

impl SpaceSeen {
    /// What the row shows for this read's offer (`Unknown` = no answer).
    fn see(&mut self, now: crate::computer::SpaceOffer) -> crate::computer::SpaceOffer {
        use crate::computer::SpaceOffer;
        match now {
            SpaceOffer::Unknown => {
                self.silent = self.silent.saturating_add(1);
                match &self.last {
                    Some(last) if self.silent < SPACE_SILENT_BEATS => last.clone(),
                    _ => SpaceOffer::Unknown,
                }
            }
            answer => {
                self.silent = 0;
                self.last = Some(answer.clone());
                answer
            }
        }
    }
}

static SPACE_SEEN: std::sync::Mutex<SpaceSeen> = std::sync::Mutex::new(SpaceSeen { last: None, silent: 0, shown: None });

/// #EXP-1514: the agent space row's offer for this read; a change is logged
/// with its cause.
fn space_seen(now: crate::computer::SpaceOffer) -> crate::computer::SpaceOffer {
    let mut seen = SPACE_SEEN.lock().unwrap_or_else(|p| p.into_inner());
    let shown = seen.see(now);
    if seen.shown.as_ref() != Some(&shown) {
        log::info!("computer use: agent space: {}", space_words(&shown));
        seen.shown = Some(shown.clone());
    }
    shown
}

/// The row's detail and, behind Not available here, the cause.
fn space_words(offer: &crate::computer::SpaceOffer) -> String {
    let (_, detail) = space_row(offer);
    match offer {
        crate::computer::SpaceOffer::No(crate::computer::SpaceBlock::Unavailable(cause)) => format!("{detail} ({cause})"),
        _ => detail.to_string(),
    }
}

/// #EXP-1514: the cause behind the agent space row's Not available here, as
/// the last read showed it (`exponential doctor` prints it beside the row).
pub fn agent_space_cause() -> Option<String> {
    match &SPACE_SEEN.lock().unwrap_or_else(|p| p.into_inner()).shown {
        Some(crate::computer::SpaceOffer::No(crate::computer::SpaceBlock::Unavailable(cause))) => Some(cause.clone()),
        _ => None,
    }
}

/// The agent space row's state and detail for an offer (device-doctor.json).
fn space_row(offer: &crate::computer::SpaceOffer) -> (DoctorState, &'static str) {
    use crate::computer::{SpaceBlock, SpaceOffer};
    match offer {
        SpaceOffer::Offered => (DoctorState::Ok, SPACE_READY),
        SpaceOffer::No(SpaceBlock::NotDesktop) => (DoctorState::Missing, SPACE_NEEDS_DESKTOP),
        SpaceOffer::No(SpaceBlock::OlderDesktop) => (DoctorState::Missing, SPACE_NEEDS_NEWER_DESKTOP),
        SpaceOffer::No(SpaceBlock::Unavailable(_)) => (DoctorState::Missing, UNAVAILABLE),
        SpaceOffer::No(SpaceBlock::Driver) => (DoctorState::Missing, SPACE_DRIVER),
        SpaceOffer::Unknown => (DoctorState::Missing, NOT_RESPONDING),
    }
}

/// #EXP-1514: the switch row's detail while the Work screen pick is Agent
/// space only and no space is offered (`spaceOnlyBlocked`).
pub fn space_only_blocked(reason: &str) -> String {
    format!("{AGENT_SPACE_LABEL} · {reason}")
}

fn item(key: &str, group: DoctorGroup, state: DoctorState) -> DoctorItem {
    DoctorItem {
        key: key.to_string(),
        group,
        parent: None,
        label: None,
        state,
        detail: None,
        action: None,
    }
}

impl DoctorItem {
    fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    fn action(mut self, action: DoctorAction) -> Self {
        self.action = Some(action);
        self
    }

    fn parent(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }
}

/// A version line's bare number: `2.55.0 (Apple Git-156)` → `2.55.0`,
/// `2.1.289 (Claude Code)` → `2.1.289`, `2.45.0.windows.3` stays.
pub fn bare_version(line: &str) -> String {
    let line = line.trim();
    let line = line.strip_prefix("git version ").unwrap_or(line);
    line.split_whitespace().next().unwrap_or_default().to_string()
}

fn format_version((major, minor, patch): (u32, u32, u32)) -> String {
    format!("{major}.{minor}.{patch}")
}

/// The fixture's `needsVersion` template.
fn needs_version(version: &str, min: (u32, u32, u32)) -> String {
    format!("{version} · needs {}", format_version(min))
}

/// The version an agent needs to RUN SESSIONS (EXP-1218: the ACP floor, the
/// higher of the two for claude).
pub fn session_floor(agent: CodingAgent) -> (u32, u32, u32) {
    match agent {
        CodingAgent::Claude => MIN_CLAUDE_ACP_VERSION,
        CodingAgent::Codex => MIN_CODEX_ACP_VERSION,
    }
}

fn git_item(check: &ToolCheck) -> DoctorItem {
    match check.version.as_deref() {
        Some(version) if check.ok => {
            item(KEY_GIT, DoctorGroup::Required, DoctorState::Ok).detail(bare_version(version))
        }
        _ => item(KEY_GIT, DoctorGroup::Required, DoctorState::Error)
            .detail(NOT_INSTALLED)
            .action(DoctorAction::Install),
    }
}

fn needs_relogin(check: &ToolCheck) -> bool {
    check
        .account
        .as_ref()
        .and_then(|account| account.health.as_deref())
        .map(Health::parse)
        == Some(Health::NeedsRelogin)
}

fn agent_item(agent: CodingAgent, check: &ToolCheck) -> DoctorItem {
    let key = agent.id();
    let row = |state| item(key, DoctorGroup::Agents, state);
    if agent == CodingAgent::Codex {
        if let Some(managed) = managed_codex_item(check) {
            return managed;
        }
    }
    let Some(version_line) = check.version.as_deref() else {
        return row(DoctorState::Missing)
            .detail(NOT_INSTALLED)
            .action(DoctorAction::Install);
    };
    let version = bare_version(version_line);
    let floor = session_floor(agent);
    // An unparseable version never reads as too old (the doctor's rule:
    // never falsely block a nonstandard build).
    if parse_claude_version(&version).is_some_and(|parsed| parsed < floor) {
        return row(DoctorState::Action)
            .detail(needs_version(&version, floor))
            .action(DoctorAction::Update);
    }
    if check.signed_out() {
        return row(DoctorState::Action)
            .detail(SIGNED_OUT)
            .action(DoctorAction::SignIn);
    }
    if needs_relogin(check) {
        return row(DoctorState::Action)
            .detail(NEEDS_RELOGIN)
            .action(DoctorAction::SignIn);
    }
    if !check.ok || check.acp == Some(false) {
        return row(DoctorState::Action)
            .detail(NOT_RESPONDING)
            .action(DoctorAction::Update);
    }
    row(DoctorState::Ok).detail(version)
}

/// EXP-1232: Codex is a managed download, so its row never reads Not
/// installed and never offers Install. With no binary in place the doctor's
/// `error` says which of three things is true ([`crate::doctor::managed_codex_check`]):
/// a fetch is running (`Downloading…`, no pill), the last one failed
/// (`Download failed` · Update re-fetches), or there is no login yet
/// (`Signed out` · Sign in, grey — the sign-in fetches). `None` = a binary
/// is there (or a custom path): the ordinary rules apply.
fn managed_codex_item(check: &ToolCheck) -> Option<DoctorItem> {
    use crate::managed_codex::{DOWNLOADING_MESSAGE, DOWNLOAD_FAILED_PREFIX};
    if check.version.is_some() {
        return None;
    }
    let error = check.error.as_deref()?;
    let row = |state| item(CodingAgent::Codex.id(), DoctorGroup::Agents, state);
    if error == DOWNLOADING_MESSAGE {
        return Some(row(DoctorState::Action).detail(DOWNLOADING));
    }
    if error.starts_with(DOWNLOAD_FAILED_PREFIX) {
        return Some(
            row(DoctorState::Action)
                .detail(DOWNLOAD_FAILED)
                .action(DoctorAction::Update),
        );
    }
    if check.signed_out() {
        return Some(
            row(DoctorState::Missing)
                .detail(SIGNED_OUT)
                .action(DoctorAction::SignIn),
        );
    }
    None
}

fn computer_items(settings: &Settings, computer: &ComputerState) -> Vec<DoctorItem> {
    let group = DoctorGroup::ComputerUse;
    if !settings.computer_use {
        return vec![item(KEY_COMPUTER_USE, group, DoctorState::Off)];
    }
    let (permissions, space) = match computer {
        ComputerState::Unsupported(reason) => {
            let detail = reason.as_deref().unwrap_or(UNAVAILABLE);
            return vec![item(KEY_COMPUTER_USE, group, DoctorState::Missing).detail(detail)];
        }
        ComputerState::Supported { permissions, space } => (permissions, space),
    };
    // #EXP-1514, BLOCKED PICK: with the Work screen pick Agent space only
    // and no space offered agents can do nothing here: the switch row says
    // why, and there is no child row.
    let row = space.as_ref().map(space_row);
    let space_only = settings.computer_use_display_id.as_deref() == Some(crate::computer::work_screen::SPACE_PICK);
    if space_only && !matches!(space, Some(crate::computer::SpaceOffer::Offered)) {
        let reason = row.map_or(SPACE_NEEDS_DESKTOP, |(_, detail)| detail);
        return vec![item(KEY_COMPUTER_USE, group, DoctorState::Missing).detail(space_only_blocked(reason))];
    }
    let all_granted = permissions.iter().all(|(_, granted)| *granted);
    let state = if all_granted { DoctorState::Ok } else { DoctorState::Action };
    let mut items = vec![item(KEY_COMPUTER_USE, group, state)];
    for (permission, granted) in permissions {
        let child = item(permission.key(), group, DoctorState::Ok).parent(KEY_COMPUTER_USE);
        items.push(if *granted {
            child.detail(GRANTED)
        } else {
            DoctorItem { state: DoctorState::Action, ..child }
                .detail(NOT_GRANTED)
                .action(DoctorAction::Grant)
        });
    }
    // #EXP-1514: the agent space row, after the permission rows: its own
    // label, no action, never a problem.
    if let Some((state, detail)) = row {
        let mut space = item(KEY_AGENT_SPACE, group, state).parent(KEY_COMPUTER_USE).detail(detail);
        space.label = Some(AGENT_SPACE_LABEL.to_string());
        items.push(space);
    }
    items
}

/// THE readiness block for `report` (pure). Items in the fixture's order:
/// git, claude, codex, computer use (+ its permission rows).
pub fn build(report: &DoctorReport, settings: &Settings, computer: ComputerState) -> DeviceDoctor {
    let mut items = vec![git_item(&report.git)];
    for agent in CodingAgent::ALL {
        items.push(agent_item(agent, report.check_for(agent)));
    }
    items.extend(computer_items(settings, &computer));
    DeviceDoctor {
        checked_at: crate::agent_accounts::now_iso(),
        items,
        displays: Vec::new(),
        desktop_audio: None,
    }
}

/// The host helper (desktop IDE + CLI daemon + `exponential doctor`):
/// [`build`] with the usage cache's `needs_relogin` verdict stamped onto the
/// login a default launch of each agent spends, and the computer state read
/// from [`crate::computer`] (only while the switch is on: nothing is asked
/// of the OS for a switched-off feature).
pub fn current(settings: &Settings, data_dir: &Path, report: &DoctorReport) -> DeviceDoctor {
    let mut report = report.clone();
    let cache = crate::usage_cache::load(data_dir);
    for agent in CodingAgent::ALL {
        let check = match agent {
            CodingAgent::Claude => &mut report.claude,
            CodingAgent::Codex => &mut report.codex,
        };
        let Some(profile) = check
            .signed_in_profile
            .clone()
            .or_else(|| crate::agent_profiles::active_profile(data_dir, agent))
        else {
            continue;
        };
        let health = cache
            .get(&crate::usage_cache::entry_key(agent.id(), &profile))
            .and_then(|entry| entry.health.clone());
        if health.as_deref().map(Health::parse) == Some(Health::NeedsRelogin) {
            check.account.get_or_insert_with(AgentAccount::default).health = health;
        }
    }
    let computer = computer_state(settings, ComputerState::read);
    let mut doctor = build(&report, settings, computer);
    add_storage(
        &mut doctor,
        crate::worktree_lifecycle::free_bytes(&settings.repos_root_path()),
        &crate::worktree_lifecycle::kept_rows(data_dir),
    );
    add_store_conflicts(&mut doctor, crate::agent_profiles::claude_store_conflicts(data_dir));
    add_cargo_sweep(&mut doctor, crate::cargo_target_gc::last_sweep(data_dir).as_ref());
    add_memory(&mut doctor, crate::memory_pressure::throttled_slice().as_ref());
    add_disk_writes(&mut doctor, eagent::disk_health::failing());
    // NEW-59: the displays ride every doctor (register + heartbeat), so the
    // Work screen row can offer them before the switch is on.
    doctor.displays = crate::computer::displays().iter().map(DoctorDisplay::from).collect();
    doctor
}

/// The computer state, asked only while the switch is on: nothing is asked
/// of the OS or of the desktop for a switched-off feature.
fn computer_state(settings: &Settings, read: impl FnOnce() -> ComputerState) -> ComputerState {
    if settings.computer_use {
        read()
    } else {
        ComputerState::Unsupported(None)
    }
}

/// EXP-1375: append the storage group (pure): the `disk` row (`{size} free`;
/// `action` below [`crate::worktree_lifecycle::LOW_DISK_BYTES`], no pill),
/// then one indented `off` row per kept worktree. `free = None` (unreadable)
/// adds nothing — the kept rows hang off the disk row.
pub fn add_storage(
    doctor: &mut DeviceDoctor,
    free: Option<u64>,
    kept: &[crate::worktree_lifecycle::KeptRow],
) {
    let Some(free) = free else { return };
    let state = if free < crate::worktree_lifecycle::LOW_DISK_BYTES {
        DoctorState::Action
    } else {
        DoctorState::Ok
    };
    doctor.items.push(
        item(KEY_DISK, DoctorGroup::Storage, state)
            .detail(disk_free(&crate::worktree_lifecycle::format_free(free))),
    );
    for (index, row) in kept.iter().enumerate() {
        let key = format!("{KEY_WORKTREE_PREFIX}{}", index + 1);
        let mut kept = item(&key, DoctorGroup::Storage, DoctorState::Off)
            .parent(KEY_DISK)
            .detail(row.reason.detail());
        kept.label = Some(row.label.clone());
        doctor.items.push(kept);
    }
}

/// #FEED-192: the row of a key holder that is not running (a warning, no
/// pill, never a blocker); a running holder adds none. #EXP-1959: `off` =
/// why eNet gave up for good ([`crate::sealing::HolderOff::detail`]).
pub fn add_key_holder(doctor: &mut DeviceDoctor, running: bool, off: Option<&str>) {
    if running {
        return;
    }
    let detail = match off {
        Some(reason) => key_holder_off(reason),
        None => "Key holder not running · restart while signed in and online".to_string(),
    };
    let mut row = item(KEY_KEY_HOLDER, DoctorGroup::Storage, DoctorState::Action).detail(detail);
    row.label = Some("Encryption keys".to_string());
    doctor.items.push(row);
}

/// EXP-1794: the row of diverged conversation copies the claude store
/// migration kept under `store/conflicts/` (a warning, no pill, never a
/// blocker: nothing was deleted, this says where to look). None = no row.
pub fn add_store_conflicts(doctor: &mut DeviceDoctor, count: usize) {
    if count == 0 {
        return;
    }
    let mut row = item(KEY_STORE_CONFLICTS, DoctorGroup::Storage, DoctorState::Action)
        .detail(store_conflicts(count));
    row.label = Some("Conversations".to_string());
    doctor.items.push(row);
}

/// #EXP-1865: the `Build cache` row under `disk`: what the shared cargo
/// target sweep freed (or, dry, found) and when, device-local time (grey,
/// never a problem). No sweep yet, or no `disk` row to hang off = no row.
/// #EXP-1892: a later sweep that freed nothing adds when it ran
/// (`cargoSweepChecked`), so a sweep that stopped shows as an old one.
/// #EXP-1996: the size is the low-disk episode's total (0 included), and
/// the profiles the last sweep found building close the line
/// (`cargoSweepSkipped`).
pub fn add_cargo_sweep(doctor: &mut DeviceDoctor, last: Option<&crate::cargo_target_gc::SweepRecord>) {
    let Some(last) = last else { return };
    let when = local_time(last.at);
    let checked = last.checked_at.filter(|checked| *checked > last.at + 60).map(local_time);
    add_cargo_sweep_at(doctor, last.freed_bytes, last.dry_run, &when, checked.as_deref(), last.skipped_locked);
}

/// Unix seconds as the doctor's device-local `16 Nov 08:14`.
fn local_time(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|at| at.with_timezone(&chrono::Local).format("%-d %b %H:%M").to_string())
        .unwrap_or_default()
}

/// [`add_cargo_sweep`] with the times already formatted (pure).
pub fn add_cargo_sweep_at(
    doctor: &mut DeviceDoctor,
    freed_bytes: u64,
    dry_run: bool,
    when: &str,
    checked: Option<&str>,
    skipped: usize,
) {
    if !doctor.items.iter().any(|item| item.key == KEY_DISK) {
        return;
    }
    let size = crate::worktree_lifecycle::format_free(freed_bytes);
    let mut detail = cargo_sweep(&size, dry_run, when);
    if let Some(checked) = checked {
        detail = cargo_sweep_checked(&detail, checked);
    }
    if skipped > 0 {
        detail = cargo_sweep_skipped(&detail, skipped);
    }
    let mut row = item(KEY_CARGO_SWEEP, DoctorGroup::Storage, DoctorState::Off)
        .parent(KEY_DISK)
        .detail(detail);
    row.label = Some(CARGO_SWEEP_LABEL.to_string());
    doctor.items.push(row);
}

/// #EXP-1892: the row of the writers a full disk refuses (a warning, no
/// pill, never a blocker): the log, the journal, the state files are
/// skipped while it lasts, the device keeps beating. None = no row.
pub fn add_disk_writes(doctor: &mut DeviceDoctor, failing: Option<(Vec<&'static str>, std::time::SystemTime)>) {
    let Some((writers, since)) = failing else { return };
    let since = since.duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);
    add_disk_writes_at(doctor, &writers.join(", "), &local_time(since));
}

/// [`add_disk_writes`] with the time already formatted (pure).
pub fn add_disk_writes_at(doctor: &mut DeviceDoctor, writers: &str, since: &str) {
    let mut row = item(KEY_DISK_WRITES, DoctorGroup::Storage, DoctorState::Action).detail(disk_writes_failing(writers, since));
    row.label = Some(DISK_WRITES_LABEL.to_string());
    doctor.items.push(row);
}

/// The fixture's `diskWritesLabel`.
pub const DISK_WRITES_LABEL: &str = "Disk writes";

/// The fixture's `diskWritesFailing` template.
pub fn disk_writes_failing(writers: &str, since: &str) -> String {
    format!("No space left · {writers} skipped since {since}")
}

/// The fixture's `cargoSweepChecked` template.
pub fn cargo_sweep_checked(sweep: &str, when: &str) -> String {
    format!("{sweep} · checked {when}")
}

/// The fixture's `cargoSweepSkipped` template.
pub fn cargo_sweep_skipped(sweep: &str, count: usize) -> String {
    format!("{sweep} · {count} building, skipped")
}

/// #EXP-1868: the row of a slice over its `memory.high` (a warning, no pill,
/// never a blocker): every command in it runs throttled, which reads like an
/// outage from inside. Not throttled = no row.
pub fn add_memory(doctor: &mut DeviceDoctor, throttled: Option<&crate::memory_pressure::SliceMemory>) {
    let Some(slice) = throttled.filter(|slice| slice.throttled()) else { return };
    let mut row = item(KEY_MEMORY, DoctorGroup::Storage, DoctorState::Action).detail(memory_throttled(
        &slice.name,
        &crate::worktree_lifecycle::format_free(slice.current),
        &crate::worktree_lifecycle::format_free(slice.high),
    ));
    row.label = Some(MEMORY_LABEL.to_string());
    doctor.items.push(row);
}

/// The fixture's `memoryLabel`.
pub const MEMORY_LABEL: &str = "Memory";

/// The fixture's `memoryThrottled` template.
pub fn memory_throttled(slice: &str, used: &str, high: &str) -> String {
    format!("{slice} at {used} of {high} · throttled, not offline")
}

/// The fixture's `cargoSweepLabel`.
pub const CARGO_SWEEP_LABEL: &str = "Build cache";

/// The fixture's `cargoSweepFreed` / `cargoSweepDryRun` templates.
pub fn cargo_sweep(size: &str, dry_run: bool, when: &str) -> String {
    if dry_run {
        format!("{size} stale · dry run · {when}")
    } else {
        format!("{size} freed · {when}")
    }
}

/// The fixture's `storeConflicts` template.
pub fn store_conflicts(count: usize) -> String {
    format!("{count} diverged · kept in store/conflicts")
}

/// #FEED-192: present while the daemon's key holder is not running. The
/// holder lives in the daemon's process, so `exponential doctor` reads this.
pub fn key_holder_marker(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("cli-key-holder-missing")
}

/// Keeps [`key_holder_marker`] in step with the daemon's holder (best
/// effort); #EXP-1959: its content = why eNet gave up, when it did.
pub fn record_key_holder(data_dir: &Path, missing: bool, off: Option<&str>) {
    let marker = key_holder_marker(data_dir);
    let _ = if missing { std::fs::write(&marker, off.unwrap_or_default()) } else { std::fs::remove_file(&marker) };
}

/// What [`record_key_holder`] left: `None` = the holder runs, `Some(off)`
/// = it does not, `off` = why eNet gave up (when it said).
pub fn read_key_holder(data_dir: &Path) -> Option<Option<String>> {
    let content = std::fs::read_to_string(key_holder_marker(data_dir)).ok()?;
    let off = content.trim();
    Some((!off.is_empty()).then(|| off.to_string()))
}

/// #EXP-1959: the fixture's `keyHolderOff` template.
pub fn key_holder_off(reason: &str) -> String {
    format!("P2P is off: {reason} · restart while signed in and online")
}

/// The fixture's `diskFree` template.
pub fn disk_free(size: &str) -> String {
    format!("{size} free")
}

/// EXP-1375: whether the doctor says the device is low on disk (its `disk`
/// row is `action`).
pub fn low_disk(doctor: &DeviceDoctor) -> bool {
    doctor
        .items
        .iter()
        .any(|item| item.key == KEY_DISK && item.state == DoctorState::Action)
}

/// NEW-78: what the IDE's microphone probe found (`etalk::microphone`, mapped
/// by the IDE: coding never depends on talk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicrophoneState {
    /// macOS: access granted; Linux/Windows (no OS permission): an input
    /// device is there.
    Granted,
    /// macOS only: access not granted (or not asked yet).
    NotGranted,
    /// No input device at all.
    NoDevice,
    /// NEW-54 (Windows): VCOMP140.dll is missing, so the CPU transcriber
    /// cannot load.
    NeedsRuntime,
}

/// NEW-78: append the voice group's `microphone` row (pure). The IDE calls it
/// only on the member's voice device while it listens (S-3: a key bound or
/// the wake word); the CLI daemon never does.
pub fn add_microphone(doctor: &mut DeviceDoctor, state: MicrophoneState) {
    let row = |state| item(KEY_MICROPHONE, DoctorGroup::Voice, state);
    doctor.items.push(match state {
        MicrophoneState::Granted => row(DoctorState::Ok).detail(GRANTED),
        MicrophoneState::NotGranted => row(DoctorState::Action)
            .detail(NOT_GRANTED)
            .action(DoctorAction::Grant),
        MicrophoneState::NoDevice => row(DoctorState::Missing).detail(NO_INPUT_DEVICE),
        MicrophoneState::NeedsRuntime => row(DoctorState::Error).detail(NEEDS_RUNTIME).action(DoctorAction::Install),
    });
}

/// NEW-55: what the speech worker said it can speak (mapped by the IDE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechState {
    /// The voice is loaded and the GPU speaks.
    Ready,
    /// The voice is being fetched.
    Downloading,
    /// No usable GPU: replies are shown, never spoken.
    NoGpu,
    /// NEW-99: the voice failed to load (the download failed before any
    /// voice was known, or the worker crashed three times in a row).
    Failed,
}

/// NEW-55: append the voice group's `speech` row (pure). The IDE calls it
/// on the member's voice device (S-3: speak-back is always on), once the
/// worker answered; callers read it to enable the telephone (voice-call.json).
pub fn add_speech(doctor: &mut DeviceDoctor, state: SpeechState) {
    let row = |state| item(KEY_SPEECH, DoctorGroup::Voice, state);
    doctor.items.push(match state {
        SpeechState::Ready => row(DoctorState::Ok).detail(READY),
        SpeechState::Downloading => row(DoctorState::Off).detail(DOWNLOADING),
        SpeechState::NoGpu => row(DoctorState::Missing).detail(NO_USABLE_GPU),
        SpeechState::Failed => row(DoctorState::Error).detail(DOWNLOAD_FAILED),
    });
}

/// S-3: append the voice group's `wake` row (pure; voice.json WAKE DETAIL).
/// The IDE calls it on the member's voice device while the wake word is on:
/// ok while listening, else off; `detail` = the device's own `wake_detail`
/// text, which every other client shows verbatim on its Wake word row.
pub fn add_wake(doctor: &mut DeviceDoctor, listening: bool, detail: &str) {
    let state = if listening { DoctorState::Ok } else { DoctorState::Off };
    doctor.items.push(item(KEY_WAKE, DoctorGroup::Voice, state).detail(detail));
}

/// NEW-57: what this machine's desktop audio capture can do (mapped by the
/// IDE from `etalk::apps`: coding never depends on talk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopAudioState {
    /// One channel per app.
    PerApp,
    /// Only the whole system's output.
    System,
    /// Nothing to capture here.
    None,
    /// macOS: the Audio Recording permission process taps need is missing.
    NotGranted,
}

/// NEW-57: append the voice group's `desktop_audio` row (pure;
/// `meeting.json` DOCTOR). The IDE calls it only on the member's voice
/// device with Desktop audio not off.
pub fn add_desktop_audio(doctor: &mut DeviceDoctor, state: DesktopAudioState) {
    let copy = &etalk::meeting::MEETING.copy;
    let row = |state| item(KEY_DESKTOP_AUDIO, DoctorGroup::Voice, state);
    doctor.items.push(match state {
        DesktopAudioState::PerApp => row(DoctorState::Ok).detail(&copy.doctor_per_app),
        DesktopAudioState::System => row(DoctorState::Ok).detail(&copy.doctor_system),
        DesktopAudioState::None => row(DoctorState::Missing).detail(&copy.doctor_none),
        DesktopAudioState::NotGranted => row(DoctorState::Action).detail(NOT_GRANTED).action(DoctorAction::Grant),
    });
}

/// The agents whose row is `ok`, and only while git is (the fixture's
/// `runnable`).
pub fn runnable_agents(doctor: &DeviceDoctor) -> Vec<&str> {
    let git_ok = doctor
        .items
        .iter()
        .any(|item| item.key == KEY_GIT && item.state == DoctorState::Ok);
    if !git_ok {
        return Vec::new();
    }
    doctor
        .items
        .iter()
        .filter(|item| item.group == DoctorGroup::Agents && item.state == DoctorState::Ok)
        .map(|item| item.key.as_str())
        .collect()
}

/// The row whose action is the PRIMARY pill on the device itself: the first
/// `action`/`error` row of the block THAT HAS an action (the fixture's
/// `localPrimary`; EXP-1232: a `Downloading…` row has none to offer).
pub fn first_action(doctor: &DeviceDoctor) -> Option<&DoctorItem> {
    doctor.items.iter().find(|item| {
        matches!(item.state, DoctorState::Action | DoctorState::Error) && item.action.is_some()
    })
}

/// The identity of a block for change detection: its items, never
/// `checkedAt` (the heartbeat sends the block only when this moved).
pub fn items_key(doctor: &DeviceDoctor) -> String {
    let items = serde_json::to_string(&doctor.items).unwrap_or_default();
    // NEW-57: the live apps ride the beat once they change.
    match &doctor.desktop_audio {
        Some(audio) => format!("{items}{}", serde_json::to_string(audio).unwrap_or_default()),
        None => items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::Permission;
    use crate::doctor::Tool;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/device-doctor.json");

    /// #FEED-161: `doctor.displays[]` carries the stable `id` beside the
    /// number; a doctor from a build before it still parses (blank id).
    #[test]
    fn doctor_displays_carry_the_stable_id() {
        let display = crate::computer::Display {
            id: "37D8832A-2D66-02CA-B9F7-8F30A301B230".into(),
            number: 1,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            primary: true,
        };
        let wire = serde_json::to_value(DoctorDisplay::from(&display)).unwrap();
        assert_eq!(wire["id"], "37D8832A-2D66-02CA-B9F7-8F30A301B230");
        assert_eq!(wire["number"], 1);
        let old: DoctorDisplay =
            serde_json::from_value(serde_json::json!({ "number": 2, "width": 1, "height": 1 })).unwrap();
        assert_eq!(old.id, "");
    }

    fn check(tool: Tool, version: Option<&str>) -> ToolCheck {
        ToolCheck {
            tool,
            ok: version.is_some(),
            version: version.map(str::to_string),
            error: version.is_none().then(|| "not found".to_string()),
            authed: tool.agent().and(version).map(|_| true),
            account: None,
            usage_eligible: false,
            acp: tool.agent().map(|_| version.is_some()),
            acp_note: None,
            signed_in_profile: None,
            profiles: Vec::new(),
        }
    }

    fn signed_out(mut check: ToolCheck) -> ToolCheck {
        check.ok = false;
        check.authed = Some(false);
        check.acp = Some(false);
        check
    }

    /// EXP-1232: the managed Codex build in one of its three absent states.
    fn managed_codex(state: crate::managed_codex::State) -> ToolCheck {
        crate::doctor::managed_codex_check(state)
    }

    /// The reason epilot gives on a Wayland session (backend/linux.rs).
    const WAYLAND: &str = "This is a Wayland session; computer use needs an X11 session for now (sign in with the \"on Xorg\" session).";

    fn settings(computer_use: bool) -> Settings {
        Settings {
            computer_use,
            ..Settings::default()
        }
    }

    /// NEW-78: the microphone row a fixture case carries, by index (the
    /// IDE's [`add_microphone`] after [`build`]).
    fn case_microphone(index: usize) -> Option<MicrophoneState> {
        match index {
            6 | 7 => Some(MicrophoneState::Granted),
            8 => Some(MicrophoneState::NotGranted),
            9 => Some(MicrophoneState::NoDevice),
            10 => Some(MicrophoneState::NeedsRuntime),
            _ => None,
        }
    }

    /// NEW-55: the speech row a fixture case carries (after the microphone).
    fn case_speech(index: usize) -> Option<SpeechState> {
        match index {
            7 => Some(SpeechState::NoGpu),
            _ => None,
        }
    }

    /// EXP-1375: the storage group a fixture case carries.
    fn case_storage(index: usize) -> Option<(u64, Vec<crate::worktree_lifecycle::KeptRow>)> {
        use crate::worktree_lifecycle::{KeptReason, KeptRow};
        match index {
            11 | 12 => Some((214_300_000_000, Vec::new())),
            14 => Some((
                6_240_000_000,
                vec![
                    KeptRow { label: "exp-session-9497089a".into(), reason: KeptReason::Uncommitted },
                    KeptRow { label: "exp-EXP-1375".into(), reason: KeptReason::Unpushed },
                ],
            )),
            _ => None,
        }
    }

    /// The hand-built input behind each fixture case, by index.
    fn case_input(index: usize) -> (DoctorReport, Settings, ComputerState) {
        match index {
            0 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0 (Apple Git-156)")),
                    claude: check(Tool::Claude, Some("2.1.289 (Claude Code)")),
                    codex: managed_codex(crate::managed_codex::State::NotWanted),
                },
                settings(false),
                ComputerState::Unsupported(None),
            ),
            1 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: check(Tool::Claude, Some("2.1.222 (Claude Code)")),
                    codex: managed_codex(crate::managed_codex::State::NotWanted),
                },
                settings(true),
                ComputerState::Supported {
                    permissions: vec![
                        (Permission::ScreenRecording, true),
                        (Permission::Accessibility, false),
                    ],
                    space: None,
                },
            ),
            2 => (
                DoctorReport {
                    git: check(Tool::Git, None),
                    claude: signed_out(check(Tool::Claude, Some("2.1.289 (Claude Code)"))),
                    codex: check(Tool::Codex, Some("0.156.1")),
                },
                settings(true),
                ComputerState::Supported { permissions: Vec::new(), space: None },
            ),
            3 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: check(Tool::Claude, Some("2.1.289 (Claude Code)")),
                    codex: managed_codex(crate::managed_codex::State::Downloading),
                },
                settings(false),
                ComputerState::Unsupported(None),
            ),
            4 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: check(Tool::Claude, Some("2.1.289 (Claude Code)")),
                    codex: managed_codex(crate::managed_codex::State::Failed(
                        "download of codex-aarch64-apple-darwin.tar.gz failed: no network".into(),
                    )),
                },
                settings(false),
                ComputerState::Unsupported(None),
            ),
            5 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: signed_out(check(Tool::Claude, Some("2.1.289 (Claude Code)"))),
                    codex: managed_codex(crate::managed_codex::State::NotWanted),
                },
                settings(false),
                ComputerState::Unsupported(None),
            ),
            // NEW-78: the first case's machine, plus its microphone row.
            6..=10 => case_input(0),
            // EXP-1375: the first case's machine, plus its storage group.
            11 | 12 | 14 => case_input(0),
            // #EXP-1899: the first case's machine, switched on in a Wayland session.
            13 => {
                let (report, _, _) = case_input(0);
                (report, settings(true), ComputerState::Unsupported(Some(WAYLAND.into())))
            }
            // #EXP-1514: the first case's machine, switched on, on Linux:
            // the agent space row says what this desktop can do.
            15..=18 => {
                use crate::computer::{SpaceBlock, SpaceOffer};
                let (report, _, _) = case_input(0);
                let offer = match index {
                    15 => SpaceOffer::Offered,
                    16 => SpaceOffer::No(SpaceBlock::NotDesktop),
                    17 => SpaceOffer::No(SpaceBlock::OlderDesktop),
                    _ => SpaceOffer::No(SpaceBlock::Unavailable("no systemd user manager".into())),
                };
                (report, settings(true), ComputerState::Supported { permissions: Vec::new(), space: Some(offer) })
            }
            // BLOCKED PICK: Agent space only on an eDE before space-readiness.
            19 => {
                let (report, _, _) = case_input(0);
                let settings = Settings { computer_use_display_id: Some("space".into()), ..settings(true) };
                let space = Some(crate::computer::SpaceOffer::No(crate::computer::SpaceBlock::OlderDesktop));
                (report, settings, ComputerState::Supported { permissions: Vec::new(), space })
            }
            _ => panic!("no input for fixture case {index}: add one"),
        }
    }

    #[test]
    fn every_fixture_case_builds_exactly() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 20, "a new fixture case needs an input here");
        for (index, case) in cases.iter().enumerate() {
            let name = case["name"].as_str().unwrap();
            let (report, settings, computer) = case_input(index);
            let mut doctor = build(&report, &settings, computer);
            if let Some(microphone) = case_microphone(index) {
                add_microphone(&mut doctor, microphone);
            }
            if let Some(speech) = case_speech(index) {
                add_speech(&mut doctor, speech);
            }
            if let Some((free, kept)) = case_storage(index) {
                add_storage(&mut doctor, Some(free), &kept);
            }
            // #EXP-1865: the low-disk case's machine also swept its build cache.
            if index == 14 {
                add_cargo_sweep_at(&mut doctor, 31_200_000_000, false, "14 Nov 23:02", None, 0);
            }
            add_key_holder(&mut doctor, index != 12, None);
            assert_eq!(
                low_disk(&doctor),
                case["composerWarning"].as_str() == Some(KEY_DISK),
                "{name}"
            );
            let expected: Vec<DoctorItem> =
                serde_json::from_value(case["doctor"]["items"].clone()).unwrap();
            assert_eq!(doctor.items, expected, "{name}");
            // Byte shape too: absent optionals are omitted on the wire.
            assert_eq!(
                serde_json::to_value(&doctor.items).unwrap(),
                case["doctor"]["items"],
                "{name}"
            );
            let runnable: Vec<&str> = case["runnable"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect();
            assert_eq!(runnable_agents(&doctor), runnable, "{name}");
            assert_eq!(
                first_action(&doctor).map(|item| item.key.as_str()),
                case["localPrimary"].as_str(),
                "{name}"
            );
            for (key, action) in case["remotePills"].as_object().unwrap() {
                let row = doctor.items.iter().find(|item| &item.key == key).unwrap();
                let action: DoctorAction = serde_json::from_value(action.clone()).unwrap();
                assert_eq!(row.action, Some(action), "{name}: {key}");
                assert!(action.remote(), "{name}: {key}");
            }
        }
    }

    #[test]
    fn the_spec_tables_match_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        for group in fixture["groups"].as_array().unwrap() {
            let parsed: DoctorGroup = serde_json::from_value(group["key"].clone()).unwrap();
            assert_eq!(parsed.label(), group["label"].as_str().unwrap());
            assert_eq!(parsed.tag(), group["tag"].as_str());
        }
        let order: Vec<DoctorGroup> = fixture["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| serde_json::from_value(group["key"].clone()).unwrap())
            .collect();
        assert_eq!(order, DoctorGroup::ALL);
        for (key, text) in fixture["labels"].as_object().unwrap() {
            assert_eq!(label(key), text.as_str().unwrap());
        }
        for (key, spec) in fixture["actions"].as_object().unwrap() {
            let action: DoctorAction = serde_json::from_value(Value::String(key.clone())).unwrap();
            assert_eq!(action.label(), spec["label"].as_str().unwrap());
            assert_eq!(action.remote(), spec["remote"].as_bool().unwrap());
        }
        for key in fixture["states"].as_object().unwrap().keys() {
            serde_json::from_value::<DoctorState>(Value::String(key.clone())).unwrap();
        }
        let copy = &fixture["copy"];
        for (key, text) in [
            ("notInstalled", NOT_INSTALLED),
            ("signedOut", SIGNED_OUT),
            ("needsRelogin", NEEDS_RELOGIN),
            ("granted", GRANTED),
            ("notGranted", NOT_GRANTED),
            ("unavailable", UNAVAILABLE),
            ("noInputDevice", NO_INPUT_DEVICE),
            ("needsRuntime", NEEDS_RUNTIME),
            ("downloading", DOWNLOADING),
            ("downloadFailed", DOWNLOAD_FAILED),
            ("keptUncommitted", crate::worktree_lifecycle::KeptReason::Uncommitted.detail()),
            ("keptUnpushed", crate::worktree_lifecycle::KeptReason::Unpushed.detail()),
            ("keptFailed", crate::worktree_lifecycle::KeptReason::Failed.detail()),
            ("notResponding", NOT_RESPONDING),
            ("agentSpaceLabel", AGENT_SPACE_LABEL),
            ("spaceReady", SPACE_READY),
            ("spaceNeedsDesktop", SPACE_NEEDS_DESKTOP),
            ("spaceNeedsNewerDesktop", SPACE_NEEDS_NEWER_DESKTOP),
            ("spaceDriver", SPACE_DRIVER),
        ] {
            assert_eq!(copy[key].as_str().unwrap(), text, "{key}");
        }
        assert_eq!(disk_free("{size}"), copy["diskFree"].as_str().unwrap());
        assert_eq!(space_only_blocked("{reason}"), copy["spaceOnlyBlocked"].as_str().unwrap());
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_key_holder(&mut doctor, true, Some("ignored"));
        assert!(doctor.items.is_empty(), "a running holder adds no row");
        add_key_holder(&mut doctor, false, None);
        let row = &doctor.items[0];
        assert_eq!((row.key.as_str(), row.group, row.state), (KEY_KEY_HOLDER, DoctorGroup::Storage, DoctorState::Action));
        assert_eq!(row.action, None);
        assert_eq!(row.label.as_deref(), copy["keyHolderLabel"].as_str());
        assert_eq!(row.detail.as_deref(), copy["keyHolderNotRunning"].as_str());
        // #EXP-1959: eNet off for good names why.
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        let off = crate::sealing::HolderOff::NodeFailed.detail();
        add_key_holder(&mut doctor, false, Some(off));
        assert_eq!(doctor.items[0].detail, Some(copy["keyHolderOff"].as_str().unwrap().replace("{reason}", off)));
        assert_eq!(doctor.items[0].label.as_deref(), copy["keyHolderLabel"].as_str());
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_store_conflicts(&mut doctor, 0);
        assert!(doctor.items.is_empty(), "no parked copy, no row");
        add_store_conflicts(&mut doctor, 2);
        let row = &doctor.items[0];
        assert_eq!((row.key.as_str(), row.group, row.state), (KEY_STORE_CONFLICTS, DoctorGroup::Storage, DoctorState::Action));
        assert_eq!(row.action, None);
        assert_eq!(row.label.as_deref(), copy["storeConflictsLabel"].as_str());
        assert_eq!(store_conflicts(2), copy["storeConflicts"].as_str().unwrap().replace("{count}", "2"));
        // #EXP-1868: a throttled slice adds the Memory warning; a calm one none.
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        let calm = crate::memory_pressure::SliceMemory { name: "expbg.slice".into(), current: 12_000_000_000, high: 16_000_000_000, full_avg10: 0.0 };
        add_memory(&mut doctor, Some(&calm));
        add_memory(&mut doctor, None);
        assert!(doctor.items.is_empty(), "not throttled, no row");
        let over = crate::memory_pressure::SliceMemory { current: 17_400_000_000, full_avg10: 75.0, ..calm };
        add_memory(&mut doctor, Some(&over));
        let row = &doctor.items[0];
        assert_eq!((row.key.as_str(), row.group, row.state), (KEY_MEMORY, DoctorGroup::Storage, DoctorState::Action));
        assert_eq!(row.action, None);
        assert_eq!(row.label.as_deref(), copy["memoryLabel"].as_str());
        assert_eq!(
            row.detail.as_deref(),
            Some(
                copy["memoryThrottled"]
                    .as_str()
                    .unwrap()
                    .replace("{slice}", "expbg.slice")
                    .replace("{used}", "17 GB")
                    .replace("{high}", "16 GB")
                    .as_str()
            )
        );
        // #EXP-1865: the Build cache row hangs off `disk`, grey.
        let last = crate::cargo_target_gc::SweepRecord {
            at: 1_763_280_000,
            freed_bytes: 31_234_000_000,
            dry_run: false,
            target: "/repos/acme/web.cargo-target".into(),
            checked_at: None,
            skipped_locked: 0,
        };
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_cargo_sweep(&mut doctor, Some(&last));
        assert!(doctor.items.is_empty(), "no disk row, no sweep row");
        add_storage(&mut doctor, Some(50_000_000_000), &[]);
        add_cargo_sweep(&mut doctor, None);
        assert_eq!(doctor.items.len(), 1, "no sweep yet, no row");
        add_cargo_sweep(&mut doctor, Some(&last));
        let row = &doctor.items[1];
        assert_eq!((row.key.as_str(), row.group, row.state), (KEY_CARGO_SWEEP, DoctorGroup::Storage, DoctorState::Off));
        assert_eq!(row.parent.as_deref(), Some(KEY_DISK));
        assert_eq!(row.action, None);
        assert_eq!(row.label.as_deref(), copy["cargoSweepLabel"].as_str());
        assert!(row.detail.as_deref().unwrap().starts_with("31 GB freed · "), "{:?}", row.detail);
        let fill = |template: &str| template.replace("{size}", "S").replace("{when}", "W");
        assert_eq!(cargo_sweep("S", false, "W"), fill(copy["cargoSweepFreed"].as_str().unwrap()));
        assert_eq!(cargo_sweep("S", true, "W"), fill(copy["cargoSweepDryRun"].as_str().unwrap()));
        // #EXP-1892: a later sweep that freed nothing says when it ran; one
        // within the minute of the freeing one says nothing more.
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_storage(&mut doctor, Some(50_000_000_000), &[]);
        add_cargo_sweep(&mut doctor, Some(&crate::cargo_target_gc::SweepRecord { checked_at: Some(last.at + 30), ..last.clone() }));
        add_cargo_sweep(&mut doctor, Some(&crate::cargo_target_gc::SweepRecord { checked_at: Some(last.at + 3_600), ..last.clone() }));
        assert!(!doctor.items[1].detail.as_deref().unwrap().contains("checked"));
        assert!(doctor.items[2].detail.as_deref().unwrap().contains(" · checked "), "{:?}", doctor.items[2].detail);
        assert_eq!(
            cargo_sweep_checked("S", "W"),
            copy["cargoSweepChecked"].as_str().unwrap().replace("{sweep}", "S").replace("{when}", "W")
        );
        // #EXP-1996: 2026-11-16 as the row tells it now: the episode's 4.2
        // GB, and the last sweep found two profiles building. A first
        // sweep that was locked out of everything is a row too.
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_storage(&mut doctor, Some(8_000_000_000), &[]);
        add_cargo_sweep_at(&mut doctor, 4_217_397_948, false, "16 Nov 18:27", Some("16 Nov 18:45"), 2);
        add_cargo_sweep_at(&mut doctor, 0, false, "16 Nov 18:45", None, 2);
        assert_eq!(
            doctor.items[1].detail.as_deref(),
            Some("4.2 GB freed · 16 Nov 18:27 · checked 16 Nov 18:45 · 2 building, skipped")
        );
        assert_eq!(doctor.items[2].detail.as_deref(), Some("0.0 GB freed · 16 Nov 18:45 · 2 building, skipped"));
        let record = crate::cargo_target_gc::SweepRecord { freed_bytes: 0, skipped_locked: 1, ..last.clone() };
        add_cargo_sweep(&mut doctor, Some(&record));
        assert!(doctor.items[3].detail.as_deref().unwrap().ends_with(" · 1 building, skipped"), "{:?}", doctor.items[3].detail);
        assert_eq!(
            cargo_sweep_skipped("S", 2),
            copy["cargoSweepSkipped"].as_str().unwrap().replace("{sweep}", "S").replace("{count}", "2")
        );
        // #EXP-1892: writers the full disk refuses = the Disk writes warning.
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_disk_writes(&mut doctor, None);
        assert!(doctor.items.is_empty(), "every write lands, no row");
        add_disk_writes_at(&mut doctor, "log, state", "16 Nov 18:45");
        let row = &doctor.items[0];
        assert_eq!((row.key.as_str(), row.group, row.state), (KEY_DISK_WRITES, DoctorGroup::Storage, DoctorState::Action));
        assert_eq!((row.action, row.parent.as_deref()), (None, None));
        assert_eq!(row.label.as_deref(), copy["diskWritesLabel"].as_str());
        assert_eq!(
            row.detail.as_deref(),
            Some(copy["diskWritesFailing"].as_str().unwrap().replace("{writers}", "log, state").replace("{when}", "16 Nov 18:45").as_str())
        );
        assert_eq!(
            needs_version("{version}", (0, 0, 0)).replace("0.0.0", "{min}"),
            copy["needsVersion"].as_str().unwrap()
        );
    }

    /// #EXP-1514: the agent space row's every state, under a switch that is
    /// on; none on macOS and Windows (no space to ask), none in a Wayland
    /// session that is not eDE (its switch row says why), none while the
    /// switch is off; the blocked pick turns the switch row itself missing.
    #[test]
    fn the_agent_space_row_and_where_there_is_none() {
        use crate::computer::{SpaceBlock, SpaceOffer};
        let (report, _, _) = case_input(0);
        let rows = |settings: &Settings, computer: ComputerState| -> Vec<(String, DoctorState, Option<String>)> {
            build(&report, settings, computer).items.into_iter().filter(|item| item.group == DoctorGroup::ComputerUse).map(|item| (item.key, item.state, item.detail)).collect()
        };
        let on = settings(true);
        let linux = |offer: SpaceOffer| ComputerState::Supported { permissions: Vec::new(), space: Some(offer) };
        let row = |state: DoctorState, detail: &str| vec![(KEY_COMPUTER_USE.to_string(), DoctorState::Ok, None), (KEY_AGENT_SPACE.to_string(), state, Some(detail.to_string()))];
        for (offer, state, detail) in [
            (SpaceOffer::Offered, DoctorState::Ok, SPACE_READY),
            (SpaceOffer::No(SpaceBlock::NotDesktop), DoctorState::Missing, SPACE_NEEDS_DESKTOP),
            (SpaceOffer::No(SpaceBlock::OlderDesktop), DoctorState::Missing, SPACE_NEEDS_NEWER_DESKTOP),
            (SpaceOffer::No(SpaceBlock::Unavailable("an agent space's session did not start".into())), DoctorState::Missing, UNAVAILABLE),
            (SpaceOffer::No(SpaceBlock::Driver), DoctorState::Missing, SPACE_DRIVER),
            (SpaceOffer::Unknown, DoctorState::Missing, NOT_RESPONDING),
        ] {
            assert_eq!(rows(&on, linux(offer.clone())), row(state, detail), "{offer:?}");
        }
        // macOS: its permission rows, no agent space row.
        let mac = ComputerState::Supported { permissions: vec![(Permission::ScreenRecording, true), (Permission::Accessibility, true)], space: None };
        assert!(!rows(&on, mac).iter().any(|(key, ..)| key == KEY_AGENT_SPACE));
        // Windows asks for no permission: the switch row alone.
        assert_eq!(rows(&on, ComputerState::Supported { permissions: Vec::new(), space: None }), [(KEY_COMPUTER_USE.to_string(), DoctorState::Ok, None)]);
        // A Wayland session that is not eDE: the switch row says why.
        assert_eq!(rows(&on, ComputerState::Unsupported(Some(WAYLAND.into()))).len(), 1);
        // The switch off: the off row alone, whatever was read.
        assert_eq!(rows(&settings(false), linux(SpaceOffer::Offered)), [(KEY_COMPUTER_USE.to_string(), DoctorState::Off, None)]);

        // BLOCKED PICK: Agent space only and the row would not be ok.
        let only = Settings { computer_use_display_id: Some(crate::computer::work_screen::SPACE_PICK.into()), ..settings(true) };
        for (offer, reason) in [
            (Some(SpaceOffer::No(SpaceBlock::NotDesktop)), SPACE_NEEDS_DESKTOP),
            (Some(SpaceOffer::No(SpaceBlock::Unavailable("no systemd user manager".into()))), UNAVAILABLE),
            (Some(SpaceOffer::Unknown), NOT_RESPONDING),
            (None, SPACE_NEEDS_DESKTOP),
        ] {
            let computer = ComputerState::Supported { permissions: Vec::new(), space: offer.clone() };
            assert_eq!(rows(&only, computer), [(KEY_COMPUTER_USE.to_string(), DoctorState::Missing, Some(space_only_blocked(reason)))], "{offer:?}");
        }
        // Offered: the rows as with any pick.
        assert_eq!(rows(&only, linux(SpaceOffer::Offered)), row(DoctorState::Ok, SPACE_READY));
        // A display pick is never blocked by the space.
        let display = Settings { computer_use_display_id: Some("output:DP-1".into()), ..settings(true) };
        assert_eq!(rows(&display, linux(SpaceOffer::No(SpaceBlock::OlderDesktop))), row(DoctorState::Missing, SPACE_NEEDS_NEWER_DESKTOP));
    }

    /// #EXP-1514: nothing is asked of the OS or the desktop while the switch
    /// is off.
    #[test]
    fn nothing_is_asked_while_the_switch_is_off() {
        assert_eq!(computer_state(&settings(false), || panic!("asked with the switch off")), ComputerState::Unsupported(None));
        let read = computer_state(&settings(true), || ComputerState::Supported { permissions: Vec::new(), space: Some(crate::computer::SpaceOffer::Offered) });
        assert!(matches!(read, ComputerState::Supported { space: Some(crate::computer::SpaceOffer::Offered), .. }));
    }

    /// #EXP-1514: the row reads ePilot's cached answer at each beat; no
    /// answer for fewer than three beats in a row keeps the last one, the
    /// third says Not responding, and the next answer stands at once.
    #[test]
    fn no_answer_for_three_beats_reads_not_responding() {
        use crate::computer::{SpaceBlock, SpaceOffer};
        let mut seen = SpaceSeen::default();
        assert_eq!(seen.see(SpaceOffer::Unknown), SpaceOffer::Unknown, "no answer ever: nothing stands");
        // #EXP-2065: a first read that timed out (eDE's consent question
        // open) says Not responding, never Update Exponential Desktop.
        assert_eq!(space_row(&SpaceOffer::Unknown), (DoctorState::Missing, NOT_RESPONDING));
        assert_eq!(seen.see(SpaceOffer::Offered), SpaceOffer::Offered);
        assert_eq!(seen.see(SpaceOffer::Unknown), SpaceOffer::Offered);
        assert_eq!(seen.see(SpaceOffer::Unknown), SpaceOffer::Offered);
        assert_eq!(seen.see(SpaceOffer::Unknown), SpaceOffer::Unknown, "the third beat without an answer");
        assert_eq!(seen.see(SpaceOffer::Unknown), SpaceOffer::Unknown);
        let older = SpaceOffer::No(SpaceBlock::OlderDesktop);
        assert_eq!(seen.see(older.clone()), older, "an answer stands at once");
        assert_eq!(seen.see(SpaceOffer::Unknown), older);
    }

    #[test]
    fn bare_versions() {
        assert_eq!(bare_version("2.55.0 (Apple Git-156)"), "2.55.0");
        assert_eq!(bare_version("git version 2.45.0.windows.3"), "2.45.0.windows.3");
        assert_eq!(bare_version("2.1.289 (Claude Code)"), "2.1.289");
        assert_eq!(bare_version("0.156.1"), "0.156.1");
    }

    #[test]
    fn needs_relogin_and_unsupported_and_an_old_codex() {
        let mut claude = check(Tool::Claude, Some("2.1.289 (Claude Code)"));
        claude.account = Some(AgentAccount {
            signed_in: true,
            health: Some("needs_relogin".to_string()),
            ..AgentAccount::default()
        });
        let report = DoctorReport {
            git: check(Tool::Git, Some("2.55.0")),
            claude,
            codex: check(Tool::Codex, Some("0.143.9")),
        };
        let doctor = build(&report, &settings(true), ComputerState::Unsupported(None));
        assert_eq!(
            doctor.items[1],
            item("claude", DoctorGroup::Agents, DoctorState::Action)
                .detail(NEEDS_RELOGIN)
                .action(DoctorAction::SignIn)
        );
        assert_eq!(
            doctor.items[2],
            item("codex", DoctorGroup::Agents, DoctorState::Action)
                .detail("0.143.9 · needs 0.144.0")
                .action(DoctorAction::Update)
        );
        assert_eq!(
            doctor.items[3..],
            [item(KEY_COMPUTER_USE, DoctorGroup::ComputerUse, DoctorState::Missing)
                .detail(UNAVAILABLE)]
        );
        assert!(runnable_agents(&doctor).is_empty());
        // A switched-on X11/Windows machine has no permission rows.
        let doctor = build(
            &report,
            &settings(true),
            ComputerState::Supported { permissions: Vec::new(), space: None },
        );
        assert_eq!(doctor.items.len(), 4);
        assert_eq!(doctor.items[3].state, DoctorState::Ok);
    }

    /// NEW-57: the desktop audio row's four states (`meeting.json` DOCTOR),
    /// and the doctor object riding the change key.
    #[test]
    fn wake_row_is_ok_while_listening_with_the_devices_detail() {
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        add_wake(&mut doctor, true, "Listening");
        add_wake(&mut doctor, false, "Microphone muted");
        let rows: Vec<_> = doctor.items.iter().map(|row| (row.key.as_str(), row.group, row.state, row.detail.as_deref())).collect();
        assert_eq!(
            rows,
            [
                (KEY_WAKE, DoctorGroup::Voice, DoctorState::Ok, Some("Listening")),
                (KEY_WAKE, DoctorGroup::Voice, DoctorState::Off, Some("Microphone muted")),
            ]
        );
        assert_eq!(label(KEY_WAKE), "Wake word");
    }

    #[test]
    fn speech_row_states() {
        let cases = [
            (SpeechState::Ready, DoctorState::Ok, "Ready"),
            (SpeechState::Downloading, DoctorState::Off, "Downloading…"),
            (SpeechState::NoGpu, DoctorState::Missing, "No usable GPU"),
            // NEW-99: a dead voice reads error · Download failed (voice-call.json `voiceFailed`).
            (SpeechState::Failed, DoctorState::Error, "Download failed"),
        ];
        for (state, expect, detail) in cases {
            let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
            add_speech(&mut doctor, state);
            let row = &doctor.items[0];
            assert_eq!((row.key.as_str(), row.group, row.state), (KEY_SPEECH, DoctorGroup::Voice, expect));
            assert_eq!((row.detail.as_deref(), row.action), (Some(detail), None));
        }
    }

    #[test]
    fn desktop_audio_row_and_object() {
        let empty = || DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        let cases = [
            (DesktopAudioState::PerApp, DoctorState::Ok, "Per app", None),
            (DesktopAudioState::System, DoctorState::Ok, "Everything only", None),
            (DesktopAudioState::None, DoctorState::Missing, "Not available here", None),
            (DesktopAudioState::NotGranted, DoctorState::Action, "Not granted", Some(DoctorAction::Grant)),
        ];
        for (state, expect, detail, action) in cases {
            let mut doctor = empty();
            add_desktop_audio(&mut doctor, state);
            let row = &doctor.items[0];
            assert_eq!((row.key.as_str(), row.group, row.state), (KEY_DESKTOP_AUDIO, DoctorGroup::Voice, expect));
            assert_eq!((row.detail.as_deref(), row.action), (Some(detail), action));
        }
        assert_eq!(label(KEY_DESKTOP_AUDIO), "Desktop audio");
        let mut doctor = empty();
        let bare = items_key(&doctor);
        assert!(!serde_json::to_string(&doctor).unwrap().contains("desktopAudio"));
        doctor.desktop_audio = Some(DoctorDesktopAudio {
            capability: etalk::meeting::DesktopAudioCapability::PerApp,
            apps: vec![etalk::meeting::AudioApp { id: "us.zoom.xos".into(), name: "zoom.us".into() }],
        });
        assert_ne!(items_key(&doctor), bare, "a new live app rides the beat");
        let value = serde_json::to_value(&doctor).unwrap();
        assert_eq!(value["desktopAudio"]["capability"], "per_app");
        assert_eq!(value["desktopAudio"]["apps"][0]["name"], "zoom.us");
    }
}
