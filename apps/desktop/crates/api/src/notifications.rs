//! Typed `notifications.*` tRPC helpers (masterplan-v3 §4.2 Inbox +
//! Account → Notifications email prefs). Verified against
//! `apps/web/src/lib/trpc/notifications.ts`:
//!
//! - `notifications.markRead({id})` → `{txId}` (ownership-guarded; the
//!   `read_at` update re-streams over the per-user notifications shape)
//! - `notifications.markAllRead()` → `{txId}`
//! - `notifications.emailPrefs()` → prefs (query; `user_notification_prefs`
//!   is server-only — read via tRPC, never synced)
//! - `notifications.updateEmailPrefs({...})` → prefs

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::labels::TxOutput;
use crate::trpc::TrpcClient;

/// `notifications.markRead` — mutation. The §4.2 inbox marks a whole issue
/// group read by calling this per notification row.
pub fn notifications_mark_read(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("notifications.markRead", &Input { id })
}

/// `notifications.markAllRead` — input-less mutation (inbox header action).
pub fn notifications_mark_all_read(trpc: &TrpcClient) -> Result<TxOutput, ApiError> {
    trpc.mutation_no_input("notifications.markAllRead")
}

/// Email-notification prefs (Account → Notifications pane). `type_prefs`
/// keys are `notification_type` contract values; a missing key means ON.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailPrefs {
    #[serde(default)]
    pub email_enabled: bool,
    #[serde(default)]
    pub type_prefs: HashMap<String, bool>,
    #[serde(default)]
    pub digest: Option<String>,
    /// False on self-hosted instances without Resend/SMTP — the pane
    /// hides/disables email affordances then (web parity).
    #[serde(default)]
    pub transport_configured: bool,
}

/// `notifications.emailPrefs` — query.
pub fn notifications_email_prefs(trpc: &TrpcClient) -> Result<EmailPrefs, ApiError> {
    trpc.query("notifications.emailPrefs")
}

/// `notifications.updateEmailPrefs` input — every field optional (send only
/// what changed).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmailPrefsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_prefs: Option<HashMap<String, bool>>,
    /// A digest cadence (`off` = hourly digest / `daily` — the server
    /// `digestValues` in `lib/notification-email-policy.ts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// `notifications.updateEmailPrefs` — mutation; returns the updated prefs.
pub fn notifications_update_email_prefs(
    trpc: &TrpcClient,
    input: &UpdateEmailPrefsInput,
) -> Result<EmailPrefs, ApiError> {
    trpc.mutation("notifications.updateEmailPrefs", input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn mark_read_posts_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":31}}}"#);
        let out = notifications_mark_read(&client(&base), "n-1").unwrap();
        assert_eq!(out.tx_id, Some(31));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/notifications.markRead HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"n-1"}"#));
    }

    #[test]
    fn mark_all_read_posts_no_body() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":32}}}"#);
        let out = notifications_mark_all_read(&client(&base)).unwrap();
        assert_eq!(out.tx_id, Some(32));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/notifications.markAllRead HTTP/1.1"));
    }

    #[test]
    fn email_prefs_query_decodes_type_map() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"emailEnabled":true,"typePrefs":{"pr_merged":false},"digest":"daily","transportConfigured":true}}}"#,
        );
        let prefs = notifications_email_prefs(&client(&base)).unwrap();
        assert!(prefs.email_enabled);
        assert_eq!(prefs.type_prefs.get("pr_merged"), Some(&false));
        assert_eq!(prefs.digest.as_deref(), Some("daily"));
        assert!(prefs.transport_configured);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // Reads are GET (a POST to a .query 405s).
        assert!(request.starts_with("GET /api/trpc/notifications.emailPrefs HTTP/1.1"));
    }

    #[test]
    fn update_email_prefs_sends_only_changed_fields() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"emailEnabled":false,"typePrefs":{},"digest":null,"transportConfigured":true}}}"#,
        );
        let input = UpdateEmailPrefsInput {
            email_enabled: Some(false),
            ..Default::default()
        };
        let prefs = notifications_update_email_prefs(&client(&base), &input).unwrap();
        assert!(!prefs.email_enabled);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"emailEnabled":false}"#));
    }
}
