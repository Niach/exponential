//! `exponential login` — device-code first (RFC 8628 via the instance's
//! Better Auth `deviceAuthorization` plugin), password/token fallbacks for
//! scripted provisioning. One mechanism covers local terminals, SSH,
//! password users and OIDC users: approval happens in ANY browser where the
//! user is signed in.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _};
use api::accounts::AuthStore;
use api::login::{normalize_instance_url, AuthClient, DevicePoll};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::term;

const DEFAULT_INSTANCE: &str = "https://app.exponential.at";

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let instance_flag = take_value(&mut args, "--instance");
    let force_password = take_flag(&mut args, "--password");
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

    // Pre-provisioned installs: a raw session token skips authentication
    // entirely (EXP_TOKEN from a secrets manager).
    if let Ok(token) = std::env::var("EXP_TOKEN") {
        if !token.trim().is_empty() {
            return finish(&auth_client, &instance, token.trim().to_string());
        }
    }

    let config = auth_client
        .fetch_auth_config(&instance)
        .with_context(|| format!("reach {instance} — is the instance URL right?"))?;

    let scripted_password = std::env::var("EXP_EMAIL").is_ok() && std::env::var("EXP_PASSWORD").is_ok();
    if force_password || scripted_password || !config.device_flow_enabled {
        if !config.password_enabled {
            bail!(
                "Password login is disabled on {instance} and this server offers no device-code flow. Update the server or sign in another way."
            );
        }
        return password_login(&auth_client, &instance);
    }

    device_login(&auth_client, &instance)
}

fn device_login(auth_client: &AuthClient, instance: &str) -> CommandResult {
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
        if local_display_available() {
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

fn password_login(auth_client: &AuthClient, instance: &str) -> CommandResult {
    let email = match std::env::var("EXP_EMAIL") {
        Ok(email) if !email.trim().is_empty() => email.trim().to_string(),
        _ => term::prompt_line("Email: ")?,
    };
    let password = match std::env::var("EXP_PASSWORD") {
        Ok(password) if !password.is_empty() => password,
        _ => term::prompt_password("Password: ")?,
    };
    if email.is_empty() || password.is_empty() {
        bail!("Email and password are required.");
    }
    let success = auth_client
        .sign_in_with_password(instance, &email, &password)
        .context("sign in")?;
    finish(auth_client, instance, success.token)
}

/// Common tail: validate the session, persist the account, mint the hidden
/// `expu_` personal key the agent MCP wiring rides.
fn finish(auth_client: &AuthClient, instance: &str, token: String) -> CommandResult {
    let user = auth_client
        .fetch_session(instance, &token)
        .context("validate the session")?
        .context("the token did not resolve to a session — sign in again")?;

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
    println!("Next: `exponential doctor`, then `exponential daemon install` to register this machine.");
    Ok(ExitCode::SUCCESS)
}
