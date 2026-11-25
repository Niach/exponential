//! #EXP-2161 (member-space.json): THE SPACE half of member spaces: what runs
//! INSIDE the isolation (`__space-daemon`, `__space-save`), started by the
//! host through [`member_space`]. The same daemon as ever, signed in with the
//! space key as the member's own device `space-<uuid>`, without what belongs
//! to the host: sign-ins, updates, CI and Actions runners, calls, computer
//! use.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::Context as _;

use crate::commands::CommandResult;
use crate::members::{ReportLine, SpaceFile, EXIT_UNAUTHORIZED, SPACE_FILE};

/// Where the host's files are in a space.
fn host_dir() -> PathBuf {
    PathBuf::from(member_space::HOST_DIR)
}

/// Whether this command is a space's (asked first thing in `main`).
pub fn is_space_command(command: &str) -> bool {
    matches!(command, "__space-daemon" | "__space-save")
}

static UNAUTHORIZED: AtomicBool = AtomicBool::new(false);

/// The server refused the space key: the daemon stops, the host asks for a
/// new one (or removes the space).
pub fn unauthorized() {
    UNAUTHORIZED.store(true, Ordering::SeqCst);
}

pub fn exit_code() -> ExitCode {
    if UNAUTHORIZED.load(Ordering::SeqCst) {
        ExitCode::from(EXIT_UNAUTHORIZED as u8)
    } else {
        ExitCode::SUCCESS
    }
}

fn space_file() -> anyhow::Result<SpaceFile> {
    let text = std::fs::read_to_string(host_dir().join(SPACE_FILE)).context("the host's file")?;
    serde_json::from_str(&text).context("the host's file")
}

/// Signs this space's data dir in with the key the host brought.
fn sign_in(data_dir: &Path, file: &SpaceFile) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    coding::atomic_config::write_atomic(&data_dir.join("cli-device-id"), &file.device_id)?;
    // A process id of an earlier start says nothing in a new PID namespace.
    let _ = std::fs::remove_file(data_dir.join("cli-daemon.pid"));
    let instance = crate::members::instance_url_inside(&file.instance_url);
    let auth = api::accounts::AuthStore::load(data_dir.to_path_buf());
    let account = auth
        .sign_in(
            &instance,
            &file.key,
            &api::login::AuthUser {
                id: file.user_id.clone(),
                email: file.email.clone(),
                name: Some(file.name.clone()),
                is_admin: None,
                onboarding_completed_at: None,
            },
        )
        .context("sign in")?;
    for other in auth.accounts() {
        if other.id != account.id {
            auth.remove_account(&other.id);
        }
    }
    let path = coding::Settings::default_path(data_dir);
    let mut settings = coding::Settings::load(&path);
    settings.computer_use = false;
    let codex = Path::new(member_space::TOOLS_DIR).join("codex");
    // Never downloaded here: the host's program, or none.
    settings.codex_path = codex.to_string_lossy().into_owned();
    settings.save(&path).context("the settings")?;
    crate::prefs::set_auto_update(data_dir, false);
    Ok(())
}

/// The host's logins become this space's, now and whenever they change.
fn follow_logins(data_dir: PathBuf) -> anyhow::Result<()> {
    let modified = || std::fs::metadata(host_dir().join(coding::space_logins::LOGINS_FILE)).and_then(|meta| meta.modified()).ok();
    let apply = move |data_dir: &Path| {
        if let Some(logins) = coding::space_logins::read(&host_dir()) {
            coding::space_logins::apply(data_dir, &logins);
        }
    };
    let mut seen: Option<SystemTime> = modified();
    apply(&data_dir);
    std::thread::Builder::new().name("exp-space-logins".into()).spawn(move || loop {
        std::thread::sleep(Duration::from_secs(3));
        let now = modified();
        if now != seen {
            seen = now;
            apply(&data_dir);
        }
    })?;
    Ok(())
}

/// What the host reports of its logins, for this device's row.
pub fn host_status() -> coding::AgentStatusPayload {
    coding::space_logins::read(&host_dir()).map(|logins| logins.status).unwrap_or_default()
}

/// One line on stdout (the host's report pipe) whenever it changes.
pub fn report(node: &steer::enet::InstallNode, last: &mut Option<ReportLine>) {
    use steer::install_trust::TrustState;
    let now = match node.trust().map(|trust| trust.snapshot()) {
        None => ReportLine { state: "starting".into(), code: None },
        Some(snapshot) => ReportLine {
            state: match snapshot.state {
                TrustState::Trusted => "ready",
                TrustState::Unknown => "starting",
                _ => "waiting",
            }
            .into(),
            code: snapshot.code,
        },
    };
    if last.as_ref() != Some(&now) {
        if let Ok(text) = serde_json::to_string(&now) {
            println!("{text}");
        }
        *last = Some(now);
    }
}

pub fn daemon() -> CommandResult {
    let file = space_file()?;
    let data_dir = crate::context::data_dir();
    sign_in(&data_dir, &file)?;
    follow_logins(data_dir)?;
    crate::commands::daemon::run(&["--foreground".to_string(), format!("--label={}", file.label)])
}

/// The last save before the space's directory is deleted: every session
/// worktree's commits and uncommitted work to its hidden ref.
pub fn save() -> CommandResult {
    let data_dir = crate::context::data_dir();
    for (target, _) in coding::auto_push::registry_targets(&data_dir) {
        coding::auto_push::sync(&target, true);
    }
    Ok(ExitCode::SUCCESS)
}
