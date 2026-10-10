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

pub const NOT_INSTALLED: &str = "Not installed";
pub const SIGNED_OUT: &str = "Signed out";
pub const NEEDS_RELOGIN: &str = "Needs re-login";
pub const GRANTED: &str = "Granted";
pub const NOT_GRANTED: &str = "Not granted";
pub const UNAVAILABLE: &str = "Not available here";
/// An agent whose version passes the floor but whose deep ACP probe failed
/// (`exponential doctor`'s codex handshake). Not in the fixture's copy.
pub const NOT_RESPONDING: &str = "Not responding";
/// EXP-1232: the managed Codex build is being fetched (no pill).
pub const DOWNLOADING: &str = "Downloading…";
/// EXP-1232: the fetch failed; Update re-fetches.
pub const DOWNLOAD_FAILED: &str = "Download failed";
/// The Import confirm (the fixture's `copy.importTitle` / `importBody`).
pub const IMPORT_BODY: &str = "Your login moves into Exponential. Sign in again outside of it.";

/// The Import confirm's title for `email` (the fixture's `copy.importTitle`).
pub fn import_title(email: &str) -> String {
    format!("Import {email}?")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDoctor {
    pub checked_at: String,
    pub items: Vec<DoctorItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorItem {
    pub key: String,
    pub group: DoctorGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub state: DoctorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<DoctorAction>,
    /// An agent row whose CLI has a signed-in AMBIENT login: that login's
    /// email. Clients render an `import` pill before the action pill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorGroup {
    Required,
    Agents,
    ComputerUse,
}

impl DoctorGroup {
    pub const ALL: [DoctorGroup; 3] =
        [DoctorGroup::Required, DoctorGroup::Agents, DoctorGroup::ComputerUse];

    /// The fixture's `groups[].label`.
    pub fn label(self) -> &'static str {
        match self {
            DoctorGroup::Required => "Required",
            DoctorGroup::Agents => "Coding agents",
            DoctorGroup::ComputerUse => "Computer use",
        }
    }

    /// The fixture's `groups[].tag`.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            DoctorGroup::Required => None,
            DoctorGroup::Agents | DoctorGroup::ComputerUse => Some("optional"),
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
    /// Move the ambient login into a profile — never an item's `action`, the
    /// pill rides [`DoctorItem::import`].
    Import,
    Grant,
}

impl DoctorAction {
    /// The fixture's `actions[].label`.
    pub fn label(self) -> &'static str {
        match self {
            DoctorAction::Install => "Install",
            DoctorAction::Update => "Update",
            DoctorAction::SignIn => "Sign in",
            DoctorAction::Import => "Import",
            DoctorAction::Grant => "Open System Settings",
        }
    }

    /// Whether ANOTHER device may trigger it (the fixture's `remote`).
    pub fn remote(self) -> bool {
        matches!(self, DoctorAction::Update | DoctorAction::SignIn | DoctorAction::Import)
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
        other => other,
    }
}

/// What computer use can do on this machine, as [`crate::computer`] says
/// (read by [`ComputerState::read`]; tests build it by hand).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerState {
    /// No backend for this OS/session.
    Unsupported,
    /// Supported; each OS permission it needs and whether it is granted
    /// (empty on X11/Windows, which ask for none).
    Supported {
        permissions: Vec<(crate::computer::Permission, bool)>,
    },
}

impl ComputerState {
    /// Ask the computer crate, never prompting.
    pub fn read() -> ComputerState {
        match crate::computer::readiness() {
            crate::computer::Readiness::Unsupported(_) => ComputerState::Unsupported,
            _ => ComputerState::Supported {
                permissions: crate::computer::permissions(),
            },
        }
    }
}

fn item(key: &str, group: DoctorGroup, state: DoctorState) -> DoctorItem {
    DoctorItem {
        key: key.to_string(),
        group,
        parent: None,
        state,
        detail: None,
        action: None,
        import: None,
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
    let mut item = agent_row(agent, check);
    item.import = check.importable.as_ref().map(|found| {
        crate::agent_accounts::account_name(found.email.as_deref(), found.plan.as_deref())
    });
    item
}

fn agent_row(agent: CodingAgent, check: &ToolCheck) -> DoctorItem {
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
    let permissions = match computer {
        ComputerState::Unsupported => {
            return vec![item(KEY_COMPUTER_USE, group, DoctorState::Missing).detail(UNAVAILABLE)]
        }
        ComputerState::Supported { permissions } => permissions,
    };
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
    let computer = if settings.computer_use {
        ComputerState::read()
    } else {
        ComputerState::Unsupported
    };
    build(&report, settings, computer)
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
    serde_json::to_string(&doctor.items).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::Permission;
    use crate::doctor::Tool;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/device-doctor.json");

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
            importable: None,
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

    fn settings(computer_use: bool) -> Settings {
        Settings {
            computer_use,
            ..Settings::default()
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
                ComputerState::Unsupported,
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
                },
            ),
            2 => (
                DoctorReport {
                    git: check(Tool::Git, None),
                    claude: signed_out(check(Tool::Claude, Some("2.1.289 (Claude Code)"))),
                    codex: check(Tool::Codex, Some("0.156.1")),
                },
                settings(true),
                ComputerState::Supported { permissions: Vec::new() },
            ),
            3 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: check(Tool::Claude, Some("2.1.289 (Claude Code)")),
                    codex: managed_codex(crate::managed_codex::State::Downloading),
                },
                settings(false),
                ComputerState::Unsupported,
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
                ComputerState::Unsupported,
            ),
            5 => (
                DoctorReport {
                    git: check(Tool::Git, Some("2.55.0")),
                    claude: ToolCheck {
                        importable: Some(crate::agent_accounts::Importable {
                            email: Some("dev@acme.test".into()),
                            plan: Some("max".into()),
                        }),
                        ..signed_out(check(Tool::Claude, Some("2.1.289 (Claude Code)")))
                    },
                    codex: managed_codex(crate::managed_codex::State::NotWanted),
                },
                settings(false),
                ComputerState::Unsupported,
            ),
            _ => panic!("no input for fixture case {index}: add one"),
        }
    }

    #[test]
    fn every_fixture_case_builds_exactly() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 6, "a new fixture case needs an input here");
        for (index, case) in cases.iter().enumerate() {
            let name = case["name"].as_str().unwrap();
            let (report, settings, computer) = case_input(index);
            let doctor = build(&report, &settings, computer);
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
            let pills = case["importPills"].as_object().cloned().unwrap_or_default();
            for row in doctor.items.iter().filter(|item| item.group == DoctorGroup::Agents) {
                assert_eq!(
                    row.import.as_deref(),
                    pills.get(&row.key).and_then(Value::as_str),
                    "{name}: {}",
                    row.key
                );
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
            ("downloading", DOWNLOADING),
            ("downloadFailed", DOWNLOAD_FAILED),
            ("importBody", IMPORT_BODY),
        ] {
            assert_eq!(copy[key].as_str().unwrap(), text, "{key}");
        }
        assert_eq!(import_title("{email}"), copy["importTitle"].as_str().unwrap());
        assert_eq!(
            needs_version("{version}", (0, 0, 0)).replace("0.0.0", "{min}"),
            copy["needsVersion"].as_str().unwrap()
        );
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
        let doctor = build(&report, &settings(true), ComputerState::Unsupported);
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
            ComputerState::Supported { permissions: Vec::new() },
        );
        assert_eq!(doctor.items.len(), 4);
        assert_eq!(doctor.items[3].state, DoctorState::Ok);
    }
}
