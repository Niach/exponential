//! An OAuth MCP client (rmcp `auth`: protected-resource and authorization
//! server discovery, dynamic client registration, PKCE S256, refresh) whose
//! credentials live in a [`Vault`] the host provides: the OS keychain on a
//! device, a server-backed store for a member's upstream connection
//! (`mcp_credentials`, sealed with [`SealedVault`] = the contract's `ev1`
//! format), or [`MemoryVault`] in tests.
//!
//! A connection follows `upstream.json`: [`begin`] = `connect` (the browser
//! opens [`Authorization::url`]), [`Authorization::finish`] = `callbackOk` /
//! `callbackError`, [`connect`] calls through stored credentials (refreshed
//! before they expire), a refused token = `unauthorized` (an
//! [`ClientError::Http`] 401), [`disconnect`] deletes the credential.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::transport::auth::{AuthClient, AuthError, AuthorizationManager, AuthorizationRequest, CredentialStore, OAuthState, StoredCredentials};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;

use crate::client::{Client, ClientError};
use crate::contract::vault::{self, Keyring, Slot};
use crate::runtime::block_on;

/// Where a host keeps secrets, one string per key. Implementations must be
/// safe to call from any thread.
pub trait Vault: Send + Sync + 'static {
    fn read(&self, key: &str) -> Option<String>;
    fn write(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// A process-local vault (tests, a one-shot CLI login).
#[derive(Default)]
pub struct MemoryVault {
    values: Mutex<HashMap<String, String>>,
}

impl Vault for MemoryVault {
    fn read(&self, key: &str) -> Option<String> {
        self.values.lock().unwrap().get(key).cloned()
    }

    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        self.values.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Seals every value with the vault format before it reaches `inner` (a
/// store of rows: the key is the row id, `table`/`column` the slot), and
/// opens it on the way back; a value that does not open reads as absent
/// ("not connected", never an error).
pub struct SealedVault<V> {
    pub inner: V,
    pub keyring: Keyring,
    pub table: String,
    pub column: String,
}

impl<V: Vault> Vault for SealedVault<V> {
    fn read(&self, key: &str) -> Option<String> {
        let slot = Slot { table: &self.table, column: &self.column, row_id: key };
        vault::open(&self.keyring, &self.inner.read(key)?, slot)
    }

    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        let slot = Slot { table: &self.table, column: &self.column, row_id: key };
        self.inner.write(key, &vault::seal(&self.keyring, value, slot))
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        self.inner.delete(key)
    }
}

/// rmcp's credential store over one vault key (the credentials as JSON).
struct VaultCredentials {
    vault: Arc<dyn Vault>,
    key: String,
}

#[async_trait::async_trait]
impl CredentialStore for VaultCredentials {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        Ok(self.vault.read(&self.key).and_then(|text| serde_json::from_str(&text).ok()))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let text = serde_json::to_string(&credentials).map_err(|err| AuthError::InternalError(err.to_string()))?;
        self.vault.write(&self.key, &text).map_err(AuthError::InternalError)
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.vault.delete(&self.key).map_err(AuthError::InternalError)
    }
}

/// What to authorize against.
#[derive(Clone, Debug)]
pub struct OAuthConfig {
    /// The MCP endpoint (`https://mcp.example.com/mcp`).
    pub url: String,
    /// Where the authorization server sends the browser back.
    pub redirect_uri: String,
    /// The client name a dynamic registration announces.
    pub client_name: String,
    pub scopes: Vec<String>,
    /// A preregistered client id (no dynamic registration).
    pub client_id: Option<String>,
}

fn auth_error(error: AuthError) -> ClientError {
    match error {
        AuthError::AuthorizationRequired => ClientError::Http(401),
        other => ClientError::Transport(format!("oauth: {other}")),
    }
}

async fn manager(url: &str, vault: Arc<dyn Vault>, key: &str) -> Result<AuthorizationManager, ClientError> {
    let mut manager = AuthorizationManager::new(url).await.map_err(auth_error)?;
    manager.set_credential_store(VaultCredentials { vault, key: key.to_string() });
    Ok(manager)
}

/// A started authorization (the `connecting` state).
pub struct Authorization {
    url: String,
    state: OAuthState,
}

impl Authorization {
    /// The page the person opens in a browser.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The callback's `code` and `state`: exchanges the code and stores the
    /// credentials in the vault (`callbackOk`); an `Err` = `callbackError`.
    pub fn finish(mut self, code: &str, csrf_state: &str) -> Result<(), ClientError> {
        let (code, csrf_state) = (code.to_string(), csrf_state.to_string());
        block_on(async move {
            self.state.handle_callback(&code, &csrf_state).await.map_err(auth_error)?;
            self.state.complete_authorization().await.map_err(auth_error)
        })
    }
}

/// Discover, register (unless `client_id` is set) and start an
/// authorization whose credentials will land in `vault` under `key`.
pub fn begin(config: &OAuthConfig, vault: Arc<dyn Vault>, key: &str) -> Result<Authorization, ClientError> {
    let (config, key) = (config.clone(), key.to_string());
    block_on(async move {
        let manager = manager(&config.url, vault, &key).await?;
        let mut state = OAuthState::Unauthorized(manager);
        let mut request = AuthorizationRequest::new(config.redirect_uri.clone())
            .with_client_name(config.client_name.clone())
            .with_scopes(config.scopes.clone());
        if let Some(client_id) = &config.client_id {
            request = request.with_preregistered_client(client_id.clone());
        }
        state.start_authorization(request).await.map_err(auth_error)?;
        let url = state.get_authorization_url().await.map_err(auth_error)?;
        Ok(Authorization { url, state })
    })
}

/// Is there a stored credential under `key`?
pub fn connected(vault: &dyn Vault, key: &str) -> bool {
    vault.read(key).is_some_and(|text| serde_json::from_str::<StoredCredentials>(&text).is_ok_and(|c| c.token_response.is_some()))
}

/// Connect to `url` with the credentials under `key`, refreshing an expired
/// access token first. No credential = [`ClientError::Http`] 401.
pub fn connect(url: &str, vault: Arc<dyn Vault>, key: &str, client_name: &str, timeout: Duration) -> Result<Client, ClientError> {
    let (owned_url, key_owned) = (url.to_string(), key.to_string());
    let manager = block_on(async move {
        let mut manager = manager(&owned_url, vault, &key_owned).await?;
        if !manager.initialize_from_store().await.map_err(auth_error)? {
            return Err(ClientError::Http(401));
        }
        Ok(manager)
    })?;
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| ClientError::Transport(format!("{url}: {err}")))?;
    let config = StreamableHttpClientTransportConfig::with_uri(url.to_string());
    let transport = move || StreamableHttpClientTransport::with_client(AuthClient::new(http, manager), config);
    Client::start(url, client_name, transport, timeout)
}

/// `disconnect`: the credential is deleted.
pub fn disconnect(vault: &dyn Vault, key: &str) -> Result<(), String> {
    vault.delete(key)
}
