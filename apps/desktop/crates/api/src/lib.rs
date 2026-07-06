//! `api` — tRPC-over-HTTP mutations + Better Auth session lifecycle
//! (masterplan-v3 §3.1, §5.7, §7.2).
//!
//! Phase 2 surface (this crate is **gpui-free** — plain types + blocking
//! `ureq` HTTP; the UI awaits results via its own executors):
//!
//! - [`login`] — email/password sign-in against Better Auth
//!   (`/api/auth/sign-in/email`), session validation (`/api/auth/get-session`),
//!   sign-out, `/api/auth-config` gating, and the OAuth-via-system-browser
//!   plumbing hooks: start-URL builders for `/api/mobile-oauth-start`, the
//!   `exp://oauth-return#token=…` deep-link parser, and the `127.0.0.1`
//!   loopback fallback listener (§5.7).
//! - [`token_store`] — the file-based secret store (0600/0700; never the OS
//!   Service / Windows Credential Manager) with the §5.7-specified
//!   `0600`-permission file fallback, per-account named entries.
//! - [`accounts`] — the signed-in account set + in-memory tokens, the
//!   call-time [`TokenProvider`] impls the sync engine consumes, and the
//!   401→reauth signal surface ([`accounts::AuthEvent::Unauthorized`],
//!   a dead token routes to login, never an empty board).
//! - [`opener`] — browser-open with the Linux fallback chain (a broken
//!   `xdg-open` degrades to a copyable URL, never a dead end).
//! - [`trpc`] — the tRPC-over-HTTP client. The server runs
//!   `initTRPC.context().create()` with **no transformer** (verified against
//!   `apps/web/src/lib/trpc.ts`), so the wire format is plain JSON: queries
//!   are `GET /api/trpc/<proc>?input=<raw-JSON percent-encoded>` (non-batched),
//!   mutations are `POST /api/trpc/<proc>` with a raw JSON body, and every
//!   success decodes the `{"result":{"data":…}}` envelope (the proven iOS
//!   `TrpcClient.swift` mirror).
//! - [`users`] — typed helpers for `users.mintPersonalApiKey` /
//!   `listPersonalApiKeys` / `revokePersonalApiKey`, plus the hidden
//!   auto-minted `expu_` personal key (mint silently on first need, file store
//!   storage, mint-new-then-revoke-old regenerate — never a UI text field).
//! - [`repositories`] / [`coding_sessions`] / [`run_configs`] — the Phase-5
//!   launcher's typed procs (§7.1/§7.3): `repositories.forIssue` +
//!   `installationToken` (JIT GitHub-App token, Debug-redacted, never
//!   persisted/logged), `codingSessions.start`/`end` (idempotent), and
//!   `runConfigs.list` + the §7.3.5 Trust & Run `command_set_hash`.
//!
//! Phase-3 surface (§4.1/§4.2): typed per-router mutation mirrors of
//! `apps/web/src/lib/trpc/*` — [`issues`] (also carries the §7.8 `prFiles`
//! query), [`projects`], [`workspaces`] (+ members + invites), [`labels`]
//! (+ issueLabels), [`comments`], [`notifications`] — plus [`patch`], the
//! tri-state omit/null/set field for zod `.nullable().optional()` updates.
//! Mutation outputs decode the server `txId` for the §4.1 `awaitTxId` gate.
//!
//! Phase-6 surface (§8): [`steer`] — the ticket-CONSUMER mirrors
//! (`steer.config`, `steer.mintTicket` control/publisher/viewer,
//! `steer.myDevices`). The desktop never signs tickets and dials the
//! server-returned `url` as-is.
//!
//! **Two distinct credentials — never confuse them (§5.7):** the Better Auth
//! *session token* is the `Authorization: Bearer` on every shape + tRPC
//! request; the `expu_` *personal API key* exists only for the coding
//! launcher's `.mcp.json` and is never a sync/tRPC credential. They live in
//! separate token-store entries.

pub mod accounts;
pub mod coding_sessions;
pub mod comments;
pub mod error;
pub mod issues;
pub mod labels;
pub mod login;
pub mod notifications;
pub mod opener;
pub mod patch;
pub mod projects;
pub mod repositories;
pub mod run_configs;
pub mod steer;
pub mod token_store;
pub mod trpc;
pub mod trust_store;
pub mod users;
pub mod workspaces;

mod encode;

pub use accounts::{Account, AuthEvent, AuthStore};
pub use error::ApiError;
pub use login::{AuthClient, AuthConfig, AuthUser, OidcProvider, SignInSuccess};
pub use patch::Patch;
pub use token_store::{SecretKind, TokenStore};
pub use trpc::TrpcClient;
pub use trust_store::{device_id, TrustStore, TrustStoreError};

use std::path::PathBuf;

/// Call-time token access (§5.7): the sync `client.rs` and the tRPC client
/// read the bearer through this **at request time** (never captured once) so
/// a re-login updates every in-flight loop's next request.
///
/// The sync crate must not depend on `api` (dependency direction, §3.1), so
/// it consumes the plain-closure form instead —
/// `Arc<dyn Fn() -> Option<String> + Send + Sync>`, produced by
/// [`AuthStore::token_provider_fn`]. Any such closure also implements this
/// trait via the blanket impl below.
pub trait TokenProvider: Send + Sync {
    /// The current session token for the bound account, or `None` when signed
    /// out (callers must then skip/park the request, never degrade to an
    /// anonymous credential).
    fn token(&self) -> Option<String>;
}

impl<F> TokenProvider for F
where
    F: Fn() -> Option<String> + Send + Sync,
{
    fn token(&self) -> Option<String> {
        self()
    }
}

/// A fixed-token provider — handy for one-shot flows (e.g. validating a
/// freshly returned sign-in token before it is persisted) and for tests.
pub struct StaticToken(pub String);

impl TokenProvider for StaticToken {
    fn token(&self) -> Option<String> {
        Some(self.0.clone())
    }
}

/// The per-user application data dir (§5.4): `~/Library/Application
/// Support/at.exponential/` on macOS, `$XDG_DATA_HOME/exponential/` (default
/// `~/.local/share/exponential/`) on Linux. Holds `accounts.json`, the
/// per-account `accounts/{id}/` dirs (sync SQLite + token-file fallback), etc.
/// The app shell computes this once and hands it to both `api` and `sync`.
///
/// Channel-isolated: the `staging` feature moves the whole dir aside so a
/// staging build never reads or writes production accounts/state (a shared
/// `accounts.json` was surfacing saved staging instance URLs in production
/// builds). Production keeps the historical path, so existing installs keep
/// their data.
pub fn default_data_dir() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        #[cfg(not(feature = "staging"))]
        {
            base.join("at.exponential")
        }
        #[cfg(feature = "staging")]
        {
            base.join("at.exponential.staging")
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        #[cfg(not(feature = "staging"))]
        {
            base.join("exponential")
        }
        #[cfg(feature = "staging")]
        {
            base.join("exponential-staging")
        }
    }
}

/// The per-account sync SQLite path (§5.4):
/// `{data_dir}/accounts/{account_id}/sync.sqlite`. `account_id` is the
/// filesystem-safe id from [`accounts::account_id_for`].
pub fn account_db_path(data_dir: &std::path::Path, account_id: &str) -> PathBuf {
    data_dir
        .join("accounts")
        .join(account_id)
        .join("sync.sqlite")
}
