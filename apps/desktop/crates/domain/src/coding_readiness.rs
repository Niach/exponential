//! EXP-1121: whether an issue can Start coding RIGHT NOW, as three ordered
//! steps every client derives from data it already has — GitHub connected,
//! a repository on the board, a device online. Start coding always renders
//! for a member (dashed amber while a step is missing, the caption naming
//! the FIRST missing one); a click opens the "Ready to code?" checklist, each
//! unmet row carrying the fix. Pure and FIXTURE-LOCKED ×4
//! (`packages/domain-contract/fixtures/coding-readiness.json`): web
//! `lib/coding-readiness.ts`'s byte-identical twin, copy included.
//!
//! Remote start (the steer relay) is NOT a step: an instance without it has
//! no remote coding at all, so the button stays hidden (`visible: false`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStepKey {
    Github,
    Repository,
    Device,
}

/// `Met` = green tick; `Current` = the FIRST unmet step (highlighted, fixes
/// shown); `Pending` = unmet behind it (no fixes until it is current).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStepState {
    Met,
    Current,
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessFix {
    ConnectGithub,
    ChooseRepository,
    BoardSettings,
    OpenDevices,
    GetDesktopApp,
    SetUpServer,
}

impl ReadinessFix {
    /// The fix button's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::ConnectGithub => copy::FIX_CONNECT_GITHUB,
            Self::ChooseRepository => copy::FIX_CHOOSE_REPOSITORY,
            Self::BoardSettings => copy::FIX_BOARD_SETTINGS,
            Self::OpenDevices => copy::FIX_OPEN_DEVICES,
            Self::GetDesktopApp => copy::FIX_GET_DESKTOP_APP,
            Self::SetUpServer => copy::FIX_SET_UP_SERVER,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessDevice {
    pub label: String,
    /// Registered by the caller (a teammate's shared server is not).
    pub own: bool,
    pub online: bool,
    /// Unix ms, `None` when never seen.
    pub last_seen_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessGithub {
    pub connected: bool,
    /// The connected account (`acme-inc`) for the met row's detail.
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodingReadinessInput {
    pub is_member: bool,
    /// `steer.config.enabled`; `None` while loading.
    pub remote_start_enabled: Option<bool>,
    pub team_name: String,
    pub board_name: String,
    /// The board's repository full name (`owner/name`), `None` = none. A
    /// board with a `repository_id` whose row has not resolved yet passes `""`.
    pub board_repository: Option<String>,
    /// Asked only while the board has no repository: does the team have a
    /// GitHub installation or repository? `None` = still loading.
    pub github: Option<ReadinessGithub>,
    /// Own devices + servers shared with the team; `None` while loading.
    pub devices: Option<Vec<ReadinessDevice>>,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessStep {
    pub key: ReadinessStepKey,
    pub state: ReadinessStepState,
    pub title: String,
    pub body: Option<String>,
    /// Right-aligned detail of a met row (account, repo, device label).
    pub detail: Option<String>,
    pub fixes: Vec<ReadinessFix>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodingReadiness {
    /// Non-member or no remote start on this instance: render nothing.
    pub visible: bool,
    /// Inputs still loading: the pill shows but stays inert, no caption.
    pub loading: bool,
    pub ready: bool,
    pub met_count: usize,
    pub total: usize,
    pub steps: Vec<ReadinessStep>,
    /// "1 of 3 set up. Fix the rest here."
    pub summary: String,
    /// The pill's one-line caption: the first missing step, `None` when
    /// ready or loading.
    pub caption: Option<String>,
}

/// Copy (byte-identical ×4).
pub mod copy {
    pub const TITLE: &str = "Ready to code?";
    pub const START: &str = "Start coding";
    pub const CLOSE: &str = "Close";
    pub const CAPTION_GITHUB: &str = "Needs GitHub";
    pub const CAPTION_REPOSITORY: &str = "Needs a repository";
    pub const CAPTION_DEVICE: &str = "No device online";
    pub const GITHUB_MET: &str = "GitHub connected";
    pub const GITHUB_UNMET: &str = "Connect GitHub";
    pub const GITHUB_BODY: &str = "Start coding clones a repository from a GitHub account or organization connected to the team.";
    pub const REPOSITORY_MET: &str = "Repository connected";
    pub const DEVICE_MET: &str = "Device online";
    pub const DEVICE_UNMET: &str = "A device online";
    pub const DEVICE_NEVER_BODY: &str = "Coding runs on the desktop app or on a server running the CLI. You haven\u{2019}t set one up yet.";
    pub const FIX_CONNECT_GITHUB: &str = "Connect GitHub";
    pub const FIX_CHOOSE_REPOSITORY: &str = "Choose repository";
    pub const FIX_BOARD_SETTINGS: &str = "Board settings";
    pub const FIX_OPEN_DEVICES: &str = "Open Devices";
    pub const FIX_GET_DESKTOP_APP: &str = "Get the desktop app";
    pub const FIX_SET_UP_SERVER: &str = "Set up a server";
    pub const PICKER_SEARCH: &str = "Search repositories\u{2026}";
    pub const PICKER_MATCHES_BOARD: &str = "matches board";
    pub const PICKER_ADD_FROM_GITHUB: &str = "Add another repository from GitHub\u{2026}";
    pub const PICKER_EMPTY: &str = "No repositories connected to the team yet.";
    pub const ALL_SET: &str = "All set.";
    pub const ONE_LEFT: &str = "One step left.";
    pub const FIX_REST: &str = "Fix the rest here.";
    pub const JUST_NOW: &str = "just now";

    /// The fixture's `copy` key → const table, in fixture order (the lock
    /// test walks it).
    pub const TABLE: &[(&str, &str)] = &[
        ("title", TITLE),
        ("start", START),
        ("close", CLOSE),
        ("captionGithub", CAPTION_GITHUB),
        ("captionRepository", CAPTION_REPOSITORY),
        ("captionDevice", CAPTION_DEVICE),
        ("githubMet", GITHUB_MET),
        ("githubUnmet", GITHUB_UNMET),
        ("githubBody", GITHUB_BODY),
        ("repositoryMet", REPOSITORY_MET),
        ("deviceMet", DEVICE_MET),
        ("deviceUnmet", DEVICE_UNMET),
        ("deviceNeverBody", DEVICE_NEVER_BODY),
        ("fixConnectGithub", FIX_CONNECT_GITHUB),
        ("fixChooseRepository", FIX_CHOOSE_REPOSITORY),
        ("fixBoardSettings", FIX_BOARD_SETTINGS),
        ("fixOpenDevices", FIX_OPEN_DEVICES),
        ("fixGetDesktopApp", FIX_GET_DESKTOP_APP),
        ("fixSetUpServer", FIX_SET_UP_SERVER),
        ("pickerSearch", PICKER_SEARCH),
        ("pickerMatchesBoard", PICKER_MATCHES_BOARD),
        ("pickerAddFromGithub", PICKER_ADD_FROM_GITHUB),
        ("pickerEmpty", PICKER_EMPTY),
        ("allSet", ALL_SET),
        ("oneLeft", ONE_LEFT),
        ("fixRest", FIX_REST),
        ("justNow", JUST_NOW),
    ];
}

pub fn repository_title(board: &str) -> String {
    format!("Connect a repository to {board}")
}

pub fn repository_body(board: &str) -> String {
    format!("Start coding clones the board\u{2019}s repository. {board} has none yet.")
}

pub fn device_body(team: &str) -> String {
    format!("None of your devices, or the ones shared with {team}, is online.")
}

pub fn last_seen(label: &str, ago: &str) -> String {
    format!("Your {label} was last seen {ago}.")
}

pub fn picker_used_by(board: &str) -> String {
    format!("used by {board}")
}

pub fn summary(met: usize, total: usize) -> String {
    let tail = if met == total {
        copy::ALL_SET
    } else if total - met == 1 {
        copy::ONE_LEFT
    } else {
        copy::FIX_REST
    };
    format!("{met} of {total} set up. {tail}")
}

/// "just now" / "5 min ago" / "2 h ago" / "3 d ago" (floored).
pub fn ago(now_ms: i64, then_ms: i64) -> String {
    let seconds = ((now_ms - then_ms) / 1000).max(0);
    // JS floors toward -inf; a negative delta clamps to 0 either way.
    if seconds < 60 {
        return copy::JUST_NOW.to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} min ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} h ago");
    }
    format!("{} d ago", hours / 24)
}

fn caption_for(key: ReadinessStepKey) -> &'static str {
    match key {
        ReadinessStepKey::Github => copy::CAPTION_GITHUB,
        ReadinessStepKey::Repository => copy::CAPTION_REPOSITORY,
        ReadinessStepKey::Device => copy::CAPTION_DEVICE,
    }
}

/// The caller's most recently seen OWN device, for "Your … was last seen"
/// (a teammate's shared server is never "yours"). First wins a tie.
fn last_seen_device(devices: &[ReadinessDevice]) -> Option<&ReadinessDevice> {
    let mut best: Option<&ReadinessDevice> = None;
    for device in devices {
        let Some(seen) = device.last_seen_at_ms else { continue };
        if !device.own {
            continue;
        }
        if best.is_none_or(|b| seen > b.last_seen_at_ms.unwrap_or(0)) {
            best = Some(device);
        }
    }
    best
}

pub fn coding_readiness(input: &CodingReadinessInput) -> CodingReadiness {
    use ReadinessStepKey as K;
    use ReadinessStepState as S;

    let has_repository = input.board_repository.is_some();
    let visible = input.is_member && input.remote_start_enabled != Some(false);
    let loading = input.remote_start_enabled.is_none()
        || input.devices.is_none()
        || (!has_repository && input.github.is_none());

    let devices: &[ReadinessDevice] = input.devices.as_deref().unwrap_or(&[]);
    let online = devices.iter().find(|device| device.online);
    let github_met = has_repository || input.github.as_ref().is_some_and(|g| g.connected);
    let met = |key: K| match key {
        K::Github => github_met,
        K::Repository => has_repository,
        K::Device => online.is_some(),
    };
    let order = [K::Github, K::Repository, K::Device];
    let first_missing = order.iter().copied().find(|key| !met(*key));
    let state = |key: K| {
        if met(key) {
            S::Met
        } else if Some(key) == first_missing {
            S::Current
        } else {
            S::Pending
        }
    };

    let last_seen_line = last_seen_device(devices).and_then(|seen| {
        seen.last_seen_at_ms
            .map(|then| last_seen(&seen.label, &ago(input.now_ms, then)))
    });
    let registered = devices.iter().any(|device| device.own);

    let steps: Vec<ReadinessStep> = order
        .iter()
        .map(|&key| {
            let s = state(key);
            match key {
                K::Github => {
                    if s == S::Met {
                        ReadinessStep {
                            key,
                            state: s,
                            title: copy::GITHUB_MET.to_string(),
                            body: None,
                            detail: input.github.as_ref().and_then(|g| g.label.clone()),
                            fixes: vec![],
                        }
                    } else {
                        ReadinessStep {
                            key,
                            state: s,
                            title: copy::GITHUB_UNMET.to_string(),
                            body: Some(copy::GITHUB_BODY.to_string()),
                            detail: None,
                            fixes: vec![ReadinessFix::ConnectGithub],
                        }
                    }
                }
                K::Repository => {
                    if s == S::Met {
                        ReadinessStep {
                            key,
                            state: s,
                            title: copy::REPOSITORY_MET.to_string(),
                            body: None,
                            detail: input
                                .board_repository
                                .clone()
                                .filter(|repo| !repo.is_empty()),
                            fixes: vec![],
                        }
                    } else {
                        ReadinessStep {
                            key,
                            state: s,
                            title: repository_title(&input.board_name),
                            body: Some(repository_body(&input.board_name)),
                            detail: None,
                            fixes: if s == S::Current {
                                vec![ReadinessFix::ChooseRepository, ReadinessFix::BoardSettings]
                            } else {
                                vec![]
                            },
                        }
                    }
                }
                K::Device => match s {
                    S::Met => ReadinessStep {
                        key,
                        state: s,
                        title: copy::DEVICE_MET.to_string(),
                        body: None,
                        detail: online.map(|device| device.label.clone()),
                        fixes: vec![],
                    },
                    S::Pending => ReadinessStep {
                        key,
                        state: s,
                        title: copy::DEVICE_UNMET.to_string(),
                        body: last_seen_line.clone(),
                        detail: None,
                        fixes: vec![],
                    },
                    S::Current => {
                        let body = if !registered {
                            copy::DEVICE_NEVER_BODY.to_string()
                        } else {
                            let mut parts = vec![device_body(&input.team_name)];
                            if let Some(line) = &last_seen_line {
                                parts.push(line.clone());
                            }
                            parts.join(" ")
                        };
                        ReadinessStep {
                            key,
                            state: s,
                            title: copy::DEVICE_UNMET.to_string(),
                            body: Some(body),
                            detail: None,
                            fixes: if registered {
                                vec![
                                    ReadinessFix::OpenDevices,
                                    ReadinessFix::GetDesktopApp,
                                    ReadinessFix::SetUpServer,
                                ]
                            } else {
                                vec![ReadinessFix::GetDesktopApp, ReadinessFix::SetUpServer]
                            },
                        }
                    }
                },
            }
        })
        .collect();

    let met_count = order.iter().filter(|key| met(**key)).count();
    CodingReadiness {
        visible,
        loading,
        ready: !loading && first_missing.is_none(),
        met_count,
        total: order.len(),
        steps,
        summary: summary(met_count, order.len()),
        caption: match first_missing {
            Some(key) if visible && !loading => Some(caption_for(key).to_string()),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `coding-readiness.test.ts`, iOS `CodingReadinessTests`, Android
    /// `CodingReadinessTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/coding-readiness.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AgoCase {
        now_ms: i64,
        then_ms: i64,
        expected: String,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: CodingReadinessInput,
        expected: Value,
    }

    #[derive(Deserialize)]
    struct Fixture {
        copy: serde_json::Map<String, Value>,
        ago: Vec<AgoCase>,
        cases: Vec<Case>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the coding-readiness fixture parses")
    }

    #[test]
    fn coding_readiness_copy_table() {
        let mut derived: Vec<(String, String)> = copy::TABLE
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        derived.extend([
            ("repositoryTitle(App)".into(), repository_title("App")),
            ("repositoryBody(App)".into(), repository_body("App")),
            ("deviceBody(Acme)".into(), device_body("Acme")),
            ("lastSeen(MacBook Pro, 2 h ago)".into(), last_seen("MacBook Pro", "2 h ago")),
            ("pickerUsedBy(Website)".into(), picker_used_by("Website")),
            ("summary(0,3)".into(), summary(0, 3)),
            ("summary(2,3)".into(), summary(2, 3)),
            ("summary(3,3)".into(), summary(3, 3)),
        ]);
        let fixture = fixture();
        assert_eq!(derived.len(), fixture.copy.len(), "every copy key mirrored");
        for (key, value) in &derived {
            assert_eq!(
                fixture.copy.get(key).and_then(Value::as_str),
                Some(value.as_str()),
                "copy: {key}"
            );
        }
    }

    #[test]
    fn coding_readiness_ago() {
        for case in fixture().ago {
            assert_eq!(ago(case.now_ms, case.then_ms), case.expected, "ago {}", case.expected);
        }
    }

    #[test]
    fn coding_readiness_contract_fixture() {
        let cases = fixture().cases;
        assert!(!cases.is_empty());
        for case in &cases {
            let actual = serde_json::to_value(coding_readiness(&case.input)).unwrap();
            assert_eq!(actual, case.expected, "case: {}", case.name);
        }
    }

    #[test]
    fn fix_labels_match_the_copy() {
        assert_eq!(ReadinessFix::ChooseRepository.label(), "Choose repository");
        assert_eq!(ReadinessFix::SetUpServer.label(), "Set up a server");
    }
}
