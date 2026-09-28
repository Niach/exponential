//! `exponential login` — device-code (RFC 8628 via the instance's Better
//! Auth `deviceAuthorization` plugin), with `EXP_TOKEN` for scripted
//! provisioning. One mechanism covers local terminals, SSH, password users
//! and OIDC users: approval happens in ANY browser where the user is signed
//! in. EXP-238 removed the `--password`/EXP_EMAIL/EXP_PASSWORD path — the
//! non-interactive credential is an API key now. EXP-1111 added the web
//! "Add device" dialog's one-time `expi_…` install token (`--install-token` /
//! `EXP_INSTALL_TOKEN`), redeemed for a session. Without a tty the device
//! flow still works: it prints the URL + code and waits for approval.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use api::accounts::AuthStore;
use api::login::{normalize_instance_url, AuthClient, DevicePoll};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::term;

const DEFAULT_INSTANCE: &str = "https://app.exponential.at";

/// How to sign in, given the environment and the server's capabilities.
#[derive(Debug, PartialEq, Eq)]
enum LoginMode {
    /// EXP-1111: a one-time `expi_…` token from the web "Add device" dialog
    /// (`--install-token` / `EXP_INSTALL_TOKEN`) — redeemed for a session.
    InstallToken(String),
    /// `EXP_TOKEN` is set — skip authentication entirely.
    Token(String),
    /// The default: RFC 8628 device code.
    Device,
    /// No token, and the server offers no device flow — nothing we can do.
    Unsupported,
}

/// Pure precedence decision, unit-tested below: a non-blank install token
/// wins (it is the one credential minted FOR this install), then a non-blank
/// `EXP_TOKEN`; otherwise the device flow, when the server offers it.
fn login_mode(
    install_token: Option<&str>,
    env_token: Option<&str>,
    device_flow_enabled: bool,
) -> LoginMode {
    if let Some(token) = install_token.map(str::trim).filter(|token| !token.is_empty()) {
        return LoginMode::InstallToken(token.to_string());
    }
    if let Some(token) = env_token {
        let token = token.trim();
        if !token.is_empty() {
            return LoginMode::Token(token.to_string());
        }
    }
    if device_flow_enabled {
        LoginMode::Device
    } else {
        LoginMode::Unsupported
    }
}

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let instance_flag = take_value(&mut args, "--instance");
    let install_token = take_value(&mut args, "--install-token")
        .or_else(|| std::env::var("EXP_INSTALL_TOKEN").ok());
    // EXP-1111: never try to open a browser (the installer's headless path;
    // implied anyway on a machine with no display).
    let no_browser = take_flag(&mut args, "--no-browser");
    // Removed by EXP-238 — a bespoke message beats "unknown flag" for the
    // provisioning scripts that still pass it.
    if take_flag(&mut args, "--password") {
        bail!(
            "--password was removed. Use the device-code login, or set EXP_TOKEN to a \
             personal API key (Settings → Security in the web app) and rerun \
             `exponential login`."
        );
    }
    reject_unknown_flags(&args)?;

    let instance = match instance_flag
        .or_else(|| std::env::var("EXP_INSTANCE").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(instance) => instance,
        None if term::stdin_is_tty() => {
            let typed = term::prompt_line(&format!("Instance URL [{DEFAULT_INSTANCE}]: "))?;
            if typed.is_empty() {
                DEFAULT_INSTANCE.to_string()
            } else {
                typed
            }
        }
        None => DEFAULT_INSTANCE.to_string(),
    };
    let instance = normalize_instance_url(&instance);
    let auth_client = AuthClient::new();

    // Pre-provisioned installs: EXP_TOKEN skips authentication entirely.
    // The value is an API key (`expu_…`, minted under Settings → Security)
    // or a raw session token — the server resolves both to a session, and
    // everything downstream rides the same Bearer path.
    match login_mode(
        install_token.as_deref(),
        std::env::var("EXP_TOKEN").ok().as_deref(),
        true,
    ) {
        LoginMode::InstallToken(token) => {
            let session = auth_client
                .redeem_install_token(&instance, &token)
                .map_err(|err| anyhow::anyhow!(err.user_message()))
                .with_context(|| format!("redeem the install token on {instance}"))?;
            return finish(&auth_client, &instance, session);
        }
        LoginMode::Token(token) => return finish(&auth_client, &instance, token),
        LoginMode::Device | LoginMode::Unsupported => {}
    }

    // Also the reachability check — a wrong instance URL fails here, not
    // three steps deep into the device flow.
    let config = auth_client
        .fetch_auth_config(&instance)
        .with_context(|| format!("reach {instance} — is the instance URL right?"))?;

    match login_mode(None, None, config.device_flow_enabled) {
        LoginMode::Token(_) | LoginMode::InstallToken(_) => unreachable!("no token in this branch"),
        LoginMode::Device => device_login(&auth_client, &instance, no_browser),
        LoginMode::Unsupported => bail!(
            "This server offers no device-code login. Update the server, or set EXP_TOKEN \
             to a personal API key (Settings → Security in the web app) and rerun \
             `exponential login`."
        ),
    }
}

fn device_login(auth_client: &AuthClient, instance: &str, no_browser: bool) -> CommandResult {
    let grant = auth_client
        .request_device_code(instance)
        .context("start the device-code flow")?;

    println!();
    println!("  Visit  {}", grant.verification_uri);
    println!("  Enter  {}", display_user_code(&grant.user_code));
    println!();
    if let Some(complete) = &grant.verification_uri_complete {
        // Best-effort browser open — useful on a local machine, harmless
        // no-op on a headless server.
        if !no_browser && local_display_available() {
            let _ = open::that(complete);
        }
    }
    println!("Waiting for approval (Ctrl-C to cancel)...");

    let deadline = Instant::now() + Duration::from_secs(grant.expires_in);
    let mut interval = Duration::from_secs(grant.interval.max(1));
    loop {
        if Instant::now() >= deadline {
            bail!("The code expired before it was approved. Run `exponential login` again.");
        }
        std::thread::sleep(interval);
        match auth_client.poll_device_token(instance, &grant.device_code)? {
            DevicePoll::Authorized { token } => return finish(auth_client, instance, token),
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval += Duration::from_secs(5),
            DevicePoll::Expired => {
                bail!("The code expired before it was approved. Run `exponential login` again.")
            }
            DevicePoll::Denied => bail!("The request was denied in the browser."),
        }
    }
}

/// `ABCD1234` prints as `ABCD-1234` (matching the web page's normalizer,
/// which strips the dash again server-side).
fn display_user_code(code: &str) -> String {
    if code.len() == 8 && !code.contains('-') {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}

fn local_display_available() -> bool {
    cfg!(target_os = "macos")
        || std::env::var("DISPLAY").is_ok()
        || std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Common tail: validate the credential, persist the account, mint the
/// hidden `expu_` personal key the agent MCP wiring rides. (When EXP_TOKEN
/// was itself an API key this mints a second, device-named key — fine: the
/// provisioned key stays the user's, the device key is the launcher's.)
fn finish(auth_client: &AuthClient, instance: &str, token: String) -> CommandResult {
    let user = auth_client
        .fetch_session(instance, &token)
        .context("validate the credential")?
        .context("the token did not resolve to a session — check EXP_TOKEN or sign in again")?;

    let data_dir = crate::context::data_dir();
    let auth = AuthStore::load(data_dir);
    let account = auth
        .sign_in(instance, &token, &user)
        .context("persist the account")?;

    let trpc = api::trpc::TrpcClient::new(instance, auth.token_provider(&account.id));
    match api::users::ensure_personal_key(&trpc, auth.token_store(), &account.id) {
        Ok(_) => {}
        // Non-fatal: the launcher mints on demand at first `code`.
        Err(err) => log::warn!("personal API key mint deferred: {err}"),
    }

    println!("Signed in as {} on {}", account.email, account.instance_url);
    // EXP-1110: signing in does NOT make this machine a device — only a
    // running daemon registers it (and keeps it online).
    match super::daemon::daemon_pid(&crate::context::data_dir()) {
        Some(_) => println!("The daemon is running: this machine is registered as a device."),
        None => {
            println!("{}", super::account::NOT_REGISTERED_HINT);
            println!("Next: `exponential doctor`, then `exponential daemon install`.");
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_wins_over_everything() {
        assert_eq!(
            login_mode(None, Some("expu_abc"), false),
            LoginMode::Token("expu_abc".to_string())
        );
        assert_eq!(
            login_mode(None, Some("  session-token  "), true),
            LoginMode::Token("session-token".to_string())
        );
    }

    #[test]
    fn blank_token_is_ignored() {
        assert_eq!(login_mode(None, Some("   "), true), LoginMode::Device);
        assert_eq!(login_mode(None, Some(""), false), LoginMode::Unsupported);
        assert_eq!(login_mode(Some("  "), None, true), LoginMode::Device);
    }

    #[test]
    fn device_is_the_default() {
        assert_eq!(login_mode(None, None, true), LoginMode::Device);
    }

    #[test]
    fn no_device_flow_and_no_token_is_unsupported() {
        assert_eq!(login_mode(None, None, false), LoginMode::Unsupported);
    }

    /// EXP-1111: the install token is minted FOR this install — it beats a
    /// stale `EXP_TOKEN` in the environment and needs no device flow.
    #[test]
    fn install_token_wins_over_everything() {
        assert_eq!(
            login_mode(Some(" expi_abc "), Some("expu_old"), false),
            LoginMode::InstallToken("expi_abc".to_string())
        );
    }
}
