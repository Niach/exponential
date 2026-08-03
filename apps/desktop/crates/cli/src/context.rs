//! Shared command context: data dir, accounts, the tRPC client and coding
//! settings — the CLI's slice of what the desktop's `AuthContext` +
//! `CodingHub` globals hold.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context as _};
use api::accounts::{Account, AuthStore};
use api::token_store::TokenStore;
use api::trpc::TrpcClient;
use coding::Settings;

pub struct Ctx {
    pub data_dir: PathBuf,
    pub auth: Arc<AuthStore>,
    pub account: Account,
    pub trpc: Arc<TrpcClient>,
    pub token_store: Arc<TokenStore>,
    pub settings: Settings,
}

impl Ctx {
    /// The CLI's persistent per-machine device id (`cliDeviceId` in the
    /// shared settings.json). Deliberately DISTINCT from the desktop app's
    /// `deviceId`: the relay evicts same-id reconnects (CLOSE_REPLACED), so
    /// a desktop and a daemon on one machine must be two devices.
    pub fn device_id(&self) -> String {
        api::device_identity::cli_device_id(&self.data_dir)
    }
}

pub fn data_dir() -> PathBuf {
    api::default_data_dir()
}

/// Load the signed-in context or fail with a "run `exponential login`" hint.
/// Several accounts may exist (shared data dir with the desktop app);
/// `EXP_ACCOUNT` (account id or email) picks one, else a single signed-in
/// account is used, else the ambiguity is an error listing the candidates.
pub fn load() -> anyhow::Result<Ctx> {
    let data_dir = data_dir();
    let auth = AuthStore::load(data_dir.clone());
    let signed_in = auth.signed_in_accounts();
    if signed_in.is_empty() {
        bail!("Not signed in. Run `exponential login` first.");
    }
    let account = match std::env::var("EXP_ACCOUNT").ok().filter(|value| !value.is_empty()) {
        Some(selector) => signed_in
            .iter()
            .find(|account| account.id == selector || account.email == selector)
            .cloned()
            .ok_or_else(|| anyhow!("EXP_ACCOUNT `{selector}` does not match a signed-in account"))?,
        None if signed_in.len() == 1 => signed_in[0].clone(),
        None => {
            let list = signed_in
                .iter()
                .map(|account| format!("  {} ({})", account.email, account.instance_url))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("Several accounts are signed in — set EXP_ACCOUNT=<email> to pick one:\n{list}");
        }
    };
    let trpc = Arc::new(TrpcClient::new(
        &account.instance_url,
        auth.token_provider(&account.id),
    ));
    let settings = Settings::load(&Settings::default_path(&data_dir));
    let token_store = Arc::new(TokenStore::new(data_dir.clone()));
    Ok(Ctx {
        data_dir,
        auth,
        account,
        trpc,
        token_store,
        settings,
    })
}

/// The hidden `expu_` personal key for this account — minted on demand
/// (`Device: <hostname>`), stored in the file token store. The launcher
/// needs it for the agent MCP wiring; the activity redactor gets it as an
/// exact-match secret.
pub fn ensure_personal_key(ctx: &Ctx) -> anyhow::Result<String> {
    api::users::ensure_personal_key(&ctx.trpc, &ctx.token_store, &ctx.account.id)
        .context("mint the personal API key")
}
