//! NEW-2: the typed `prompts.*` client (web `apps/web/src/lib/trpc/prompts.ts`).
//!
//! Prompts replaced actions: a name, a curated icon and a markdown body,
//! nothing else (no inputs, triggers or repository). Running one is
//! `sessions.send({promptId, text})` ([`crate::sessions::SendInput`]): a
//! root whose first message is the body plus what was typed. Listing rides
//! the synced `prompts` shape (body EXCLUDED); [`get`] is the only body
//! path. Reads are member-gated, writes owner-only.
//!
//! Plain blocking calls: run them on a background executor only.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The server's `MAX_PROMPT_NAME`.
pub const MAX_PROMPT_NAME_CHARS: usize = 255;
/// The server's `MAX_PROMPT_BODY` (bytes).
pub const MAX_PROMPT_BODY_BYTES: usize = 64 * 1024;

/// One prompt as the wire carries it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub name: String,
    /// A curated registry icon (the `boards.icon` set); `None` = the generic
    /// glyph.
    #[serde(default)]
    pub icon: Option<String>,
    /// The markdown body: EMPTY on shape-synced rows ([`from_row`]); [`get`]
    /// returns the real one.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub sort_order: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct ListResponse {
    prompts: Vec<Prompt>,
}

#[derive(Deserialize)]
struct PromptResponse {
    prompt: Prompt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TeamIdInput<'a> {
    team_id: &'a str,
}

#[derive(Serialize)]
struct IdInput<'a> {
    id: &'a str,
}

/// `prompts.list`: ordered by `sortOrder` then `name` server-side.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<Prompt>, ApiError> {
    let response: ListResponse = trpc.query_with_input("prompts.list", &TeamIdInput { team_id })?;
    Ok(response.prompts)
}

/// `prompts.get`: the row with its body (member read).
pub fn get(trpc: &TrpcClient, id: &str) -> Result<Prompt, ApiError> {
    let response: PromptResponse = trpc.query_with_input("prompts.get", &IdInput { id })?;
    Ok(response.prompt)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateInput<'a> {
    team_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'a str>,
    body: &'a str,
}

/// `prompts.create` (owner only): appended to the end of the sort order.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    name: &str,
    icon: Option<&str>,
    body: &str,
) -> Result<Prompt, ApiError> {
    let response: PromptResponse =
        trpc.mutation("prompts.create", &CreateInput { team_id, name, icon, body })?;
    Ok(response.prompt)
}

/// `prompts.update` input (strict server-side: only these keys). Omitted
/// fields stay unchanged; `icon: Some(None)` clears the glyph.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptUpdate {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
}

impl PromptUpdate {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Self::default() }
    }
}

/// `prompts.update` (owner only).
pub fn update(trpc: &TrpcClient, input: &PromptUpdate) -> Result<Prompt, ApiError> {
    let response: PromptResponse = trpc.mutation("prompts.update", input)?;
    Ok(response.prompt)
}

/// `prompts.delete` (owner only).
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct Ok_ {
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok_ = trpc.mutation("prompts.delete", &IdInput { id })?;
    Ok(())
}

/// Hydrate the list projection from a synced `prompts` shape row. `body`
/// is deliberately empty: the shape excludes it; [`get`] is the body path.
pub fn from_row(row: &domain::rows::PromptRow) -> Prompt {
    Prompt {
        id: row.id.clone(),
        team_id: row.team_id.clone().unwrap_or_default(),
        name: row.name.clone().unwrap_or_default(),
        icon: row.icon.clone(),
        body: String::new(),
        sort_order: row.sort_order.unwrap_or_default(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
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

    fn request(captured: &flume::Receiver<String>) -> String {
        captured.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn list_and_get_unwrap_their_envelopes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"prompts":[{"id":"p-1","teamId":"t-1","name":"Review PR","icon":null,"sortOrder":1}]}}}"#,
        );
        let prompts = list(&client(&base), "t-1").unwrap();
        assert_eq!(prompts[0].name, "Review PR");
        assert!(prompts[0].body.is_empty());
        assert!(request(&captured).starts_with("GET /api/trpc/prompts.list?input="));

        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"prompt":{"id":"p-1","teamId":"t-1","name":"Review PR","body":"# Review"}}}}"##,
        );
        assert_eq!(get(&client(&base), "p-1").unwrap().body, "# Review");
        assert!(request(&captured).starts_with("GET /api/trpc/prompts.get?input="));
    }

    #[test]
    fn writes_post_only_the_prompt_keys() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"prompt":{"id":"p-1","teamId":"t-1","name":"Groom","body":"do it"},"txId":1}}}"#,
        );
        create(&client(&base), "t-1", "Groom", None, "do it").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/prompts.create HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"teamId":"t-1","name":"Groom","body":"do it"}"#), "{text}");

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"prompt":{"id":"p-1","teamId":"t-1","name":"Renamed","body":"x"},"txId":2}}}"#,
        );
        let input = PromptUpdate {
            name: Some("Renamed".into()),
            icon: Some(Some("sparkles".into())),
            ..PromptUpdate::new("p-1")
        };
        assert_eq!(update(&client(&base), &input).unwrap().name, "Renamed");
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/prompts.update HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"p-1","name":"Renamed","icon":"sparkles"}"#), "{text}");

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"ok":true,"id":"p-1","txId":3}}}"#);
        delete(&client(&base), "p-1").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/prompts.delete HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"id":"p-1"}"#), "{text}");
    }

    #[test]
    fn from_row_hydrates_the_synced_projection() {
        let row: domain::rows::PromptRow = serde_json::from_value(serde_json::json!({
            "id": "p-1",
            "team_id": "t-1",
            "name": "Review PR",
            "icon": "sparkles",
            "sort_order": "1.5"
        }))
        .unwrap();
        let prompt = from_row(&row);
        assert_eq!(prompt.team_id, "t-1");
        assert_eq!(prompt.icon.as_deref(), Some("sparkles"));
        assert!(prompt.body.is_empty());
        assert_eq!(prompt.sort_order, 1.5);
    }
}
