//! `exponential doctor` — THE device readiness block (EXP-1196/1218/1219,
//! `packages/domain-contract/fixtures/device-doctor.json`) as text: the same
//! rows every client renders from the `doctor` blob this machine sends,
//! built by `coding::device_doctor`. `status` and `login` print the same
//! block through [`render_block`].
//!
//! Exit code is non-zero when nothing can run: git is not ok, or NO agent is
//! runnable (`device_doctor::runnable_agents`). One agent is enough — a
//! blocked default agent beside a runnable one never fails the doctor.
//!
//! EXP-755/EXP-849: this command is the ONE deep pass (the codex handshake
//! below). Every other caller runs the plain doctor.

use std::path::Path;
use std::process::ExitCode;

use coding::device_doctor::{
    self, DeviceDoctor, DoctorAction, DoctorGroup, DoctorItem, DoctorState, KEY_COMPUTER_USE,
    KEY_GIT,
};
use coding::doctor::ToolCheck;
use coding::CodingAgent;

use super::{reject_unknown_flags, CommandResult};
use crate::context;

pub fn run(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let data_dir = context::data_dir();
    let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
    let doctor = local_doctor(&settings, &data_dir, true);
    for line in render_block(&doctor, Some(&api::users::hostname())) {
        println!("{line}");
    }
    let blocked = nothing_can_run(&doctor);
    if blocked {
        println!();
        println!("Nothing can run a coding session on this device yet.");
    }
    Ok(if blocked { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// The exit rule (pure): git not ok, or no agent runnable. The closing line
/// and the non-zero exit both hang off it.
fn nothing_can_run(doctor: &DeviceDoctor) -> bool {
    let git_ok = doctor
        .items
        .iter()
        .any(|item| item.key == KEY_GIT && item.state == DoctorState::Ok);
    !git_ok || device_doctor::runnable_agents(doctor).is_empty()
}

/// This machine's block. `deep` adds the real codex app-server handshake
/// (only `exponential doctor` pays for it).
pub fn local_doctor(settings: &coding::Settings, data_dir: &Path, deep: bool) -> DeviceDoctor {
    let mut report = coding::run_doctor(settings, data_dir);
    if deep {
        // EXP-746: `run_doctor` also runs on the launch path and inline in
        // the daemon every 5 minutes, so it takes codex's readiness on
        // presence. A hand-typed `exponential doctor` can afford the real
        // handshake.
        deep_probe_codex_acp(settings, &mut report.codex);
    }
    device_doctor::current(settings, data_dir, &report)
}

fn glyph(state: DoctorState) -> &'static str {
    match state {
        DoctorState::Ok => "✓",
        DoctorState::Action => "!",
        DoctorState::Missing | DoctorState::Off => "–",
        DoctorState::Error => "✗",
    }
}

fn group_header(group: DoctorGroup) -> String {
    match group.tag() {
        Some(tag) => format!("{} ({tag})", group.label()),
        None => group.label().to_string(),
    }
}

/// The concrete fix for an `action`/`error` row, when there is one to type.
fn fix_hint(item: &DoctorItem) -> Option<String> {
    let agent = CodingAgent::parse(&item.key);
    let hint = match (item.action?, agent) {
        // EXP-1232: Codex is a managed download — its Update re-fetches the
        // pinned build and its Sign in fetches first; both run from any
        // Exponential app pointed at this device, never a typed command.
        (DoctorAction::Update, Some(CodingAgent::Codex)) => {
            "fetch again from the app: Devices → this device → Update".to_string()
        }
        (DoctorAction::Update, Some(agent)) => format!("run: {} update", agent.id()),
        (DoctorAction::SignIn, Some(CodingAgent::Claude)) => "run: claude".to_string(),
        (DoctorAction::SignIn, Some(CodingAgent::Codex)) => {
            "sign in from the app: Devices → this device → Sign in".to_string()
        }
        (DoctorAction::Install, Some(CodingAgent::Claude)) => {
            "install: curl -fsSL https://claude.ai/install.sh | bash".to_string()
        }
        (DoctorAction::Install, None) if item.key == KEY_GIT => git_install_hint().to_string(),
        (DoctorAction::Grant, _) => "System Settings > Privacy & Security".to_string(),
        _ => return None,
    };
    Some(hint)
}

fn git_install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "install: xcode-select --install"
    } else if cfg!(target_os = "windows") {
        "install: winget install Git.Git"
    } else {
        "install: git from your package manager"
    }
}

/// Label column width at the top level: the indented `Screen Recording`,
/// the longest label, still gets a two-space gap.
const LABEL_WIDTH: usize = 20;

fn row(indent: usize, item: &DoctorItem, label: &str) -> String {
    let pad = (LABEL_WIDTH + 2).saturating_sub(indent);
    let mut line = format!("{}{} {label:<pad$}", " ".repeat(indent), glyph(item.state));
    if let Some(detail) = &item.detail {
        line.push_str(detail);
    }
    if matches!(item.state, DoctorState::Action | DoctorState::Error) {
        if let Some(hint) = fix_hint(item) {
            line.push_str(&format!("  ({hint})"));
        }
    }
    line.trim_end().to_string()
}

/// THE block as text lines (pure): the device label line (when given), then
/// per group a header and its rows. The computer-use switch row reads `On`
/// (or its detail), `– Off` while switched off; permission rows indent under
/// it.
pub fn render_block(doctor: &DeviceDoctor, device_label: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(label) = device_label {
        lines.push(format!("This device: {label}"));
    }
    for group in DoctorGroup::ALL {
        let items: Vec<&DoctorItem> =
            doctor.items.iter().filter(|item| item.group == group).collect();
        if items.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(group_header(group));
        for item in items {
            if item.key == KEY_COMPUTER_USE {
                let text = match (item.state, item.detail.as_deref()) {
                    (DoctorState::Off, _) => "Off",
                    (_, Some(detail)) => detail,
                    _ => "On",
                };
                lines.push(format!("  {} {text}", glyph(item.state)));
            } else if item.parent.is_some() {
                lines.push(row(4, item, device_doctor::label(&item.key)));
            } else {
                lines.push(row(2, item, device_doctor::label(&item.key)));
            }
        }
    }
    lines
}

/// The real `codex app-server` handshake, replacing the presence-only answer
/// `run_doctor` gives (bounded by the doctor's own 10 s probe timeout, killed
/// on drop). Only ever DOWNGRADES: a codex that is not installed or not
/// signed in already reads as not ready, and there is nothing to probe.
fn deep_probe_codex_acp(settings: &coding::Settings, check: &mut ToolCheck) {
    if check.acp != Some(true) {
        return;
    }
    let program = settings.resolved_path_for(CodingAgent::Codex);
    if coding::doctor::probe_codex_acp(&program, &terminal::pty::login_path()) {
        return;
    }
    check.acp = Some(false);
    check.acp_note = Some("`codex app-server` did not answer".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../../packages/domain-contract/fixtures/device-doctor.json");

    fn case(index: usize) -> DeviceDoctor {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        serde_json::from_value(fixture["cases"][index]["doctor"].clone()).unwrap()
    }

    #[test]
    fn renders_the_ready_case() {
        assert_eq!(
            render_block(&case(0), Some("mac")),
            [
                "This device: mac",
                "",
                "Required",
                "  ✓ Git                 2.55.0",
                "",
                "Coding agents (optional)",
                "  ✓ Claude Code         2.1.289",
                "  – Codex               Signed out",
                "",
                "Computer use (optional)",
                "  – Off",
            ]
        );
    }

    /// One agent is enough: the ready case passes whatever the default
    /// agent is (codex is missing there); a block with no runnable agent, or
    /// no git, fails.
    #[test]
    fn exits_non_zero_only_when_nothing_can_run() {
        assert!(!nothing_can_run(&case(0)));
        assert!(nothing_can_run(&case(1)), "claude too old, codex missing");
        assert!(nothing_can_run(&case(2)), "git missing");
        let mut no_git = case(0);
        no_git.items.retain(|item| item.key != KEY_GIT);
        assert!(nothing_can_run(&no_git));
    }

    #[test]
    fn renders_actions_with_their_fix() {
        assert_eq!(
            render_block(&case(1), None)[3..],
            [
                "Coding agents (optional)",
                "  ! Claude Code         2.1.222 · needs 2.1.263  (run: claude update)",
                "  – Codex               Signed out",
                "",
                "Computer use (optional)",
                "  ! On",
                "    ✓ Screen Recording  Granted",
                "    ! Accessibility     Not granted  (System Settings > Privacy & Security)",
            ]
        );
        let lines = render_block(&case(2), None);
        assert!(lines.contains(&format!("  ✗ Git                 Not installed  ({})", git_install_hint())));
        assert!(lines.contains(&"  ! Claude Code         Signed out  (run: claude)".to_string()));
        assert!(!lines.iter().any(|line| line.contains("Remote desktop")));
    }
}
