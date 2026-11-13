//! The `exponential` provider: credentials from an Exponential server's
//! `repositories.credentials` op (#EXP-1614), `{remoteUrl, username, token,
//! expiresAt}` for any host, cached until the contract's refresh margin.
//! tRPC mutation, plain JSON (`POST /api/trpc/repositories.credentials`,
//! `{result: {data}}`), bearer auth.

use std::sync::Mutex;

use serde_json::json;

use crate::http::{client, send};
use crate::{CredentialSource, Credentials, Error, Result};

/// Credentials for one repository, minted by the server.
pub struct ExponentialCredentials {
    client: reqwest::blocking::Client,
    base_url: String,
    bearer: String,
    /// `{"repositoryId": …}` or `{"remote": "exp://team/repo"}`.
    target: serde_json::Value,
    cached: Mutex<Option<Credentials>>,
    now_ms: fn() -> i64,
}

fn system_now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

impl ExponentialCredentials {
    /// `base_url` = the instance (`https://app.exponential.at`); `bearer` =
    /// the device's session token.
    pub fn new(base_url: &str, bearer: impl Into<String>, repository_id: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: client("ecode")?,
            base_url: base_url.trim_end_matches('/').to_string(),
            bearer: bearer.into(),
            target: json!({ "repositoryId": repository_id.into() }),
            cached: Mutex::new(None),
            now_ms: system_now_ms,
        })
    }

    /// The repo an `exp://<team>/<repo>` URL names (#EXP-1616): what
    /// `git-remote-exp` knows; the server resolves it.
    pub fn for_remote(base_url: &str, bearer: impl Into<String>, remote: &str) -> Result<Self> {
        let mut source = Self::new(base_url, bearer, "")?;
        source.target = json!({ "remote": remote });
        Ok(source)
    }

    /// The clock the refresh rule reads (tests).
    pub fn with_clock(mut self, now_ms: fn() -> i64) -> Self {
        self.now_ms = now_ms;
        self
    }

    fn mint(&self) -> Result<Credentials> {
        let url = format!("{}/api/trpc/repositories.credentials", self.base_url);
        let request = self
            .client
            .post(url)
            .bearer_auth(&self.bearer);
        let request = crate::http::with_json(request, &self.target);
        let body = send(request, &[&self.bearer])?.ok(&[&self.bearer])?;
        let data = body.pointer("/result/data").cloned().unwrap_or(body);
        let credentials: Credentials = serde_json::from_value(data)
            .map_err(|e| Error::Http { status: 200, detail: format!("repositories.credentials: {e}") })?;
        credentials.validated()
    }
}

impl CredentialSource for ExponentialCredentials {
    fn credentials(&self) -> Result<Credentials> {
        let mut cached = self.cached.lock().unwrap();
        if let Some(credentials) = cached.as_ref().filter(|c| !c.needs_refresh((self.now_ms)())) {
            return Ok(credentials.clone());
        }
        let fresh = self.mint()?;
        *cached = Some(fresh.clone());
        Ok(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::mock::MockServer;

    #[test]
    fn mints_caches_and_refreshes() {
        let server = MockServer::start(|method, path, body| {
            assert_eq!((method, path), ("POST", "/api/trpc/repositories.credentials"));
            assert_eq!(body["repositoryId"], "repo-1");
            (200, json!({ "result": { "data": {
                "remoteUrl": "https://git.example.com/team/app",
                "username": "exp-bot",
                "token": "tok-1",
                "expiresAt": "1970-01-01T01:00:00.000Z"
            } } }))
        });
        let source = ExponentialCredentials::new(&server.url(), "session-tok", "repo-1").unwrap().with_clock(|| 0);
        let c = source.credentials().unwrap();
        assert_eq!(c.remote_url(), "https://git.example.com/team/app.git");
        assert_eq!(c.helper_key().unwrap(), "credential.https://git.example.com/team/app.git.helper");
        source.credentials().unwrap();
        assert_eq!(server.requests().len(), 1, "cached within the margin");
        assert!(server.requests()[0].to_lowercase().contains("authorization: bearer session-tok"));

        let late = ExponentialCredentials::new(&server.url(), "s", "repo-1").unwrap().with_clock(|| 3_600_000);
        late.credentials().unwrap();
        late.credentials().unwrap();
        assert_eq!(server.requests().len(), 3, "an expiring token re-mints");
    }
}
