//! Typed `sidebarLayout.*` tRPC helpers (#EXP-1832, sidebar-folders.json).
//! Verified against `apps/web/src/lib/trpc/sidebar-layout.ts`:
//!
//! - `sidebarLayout.save({teamId, section, upserts, deletes})` → `{txId}`:
//!   one move's rows, the output of `domain::sidebar_folders::sidebar_move` /
//!   `sidebar_ungroup` ([`SidebarWrites`]).
//! - `sidebarLayout.rename({id, name})` → `{txId, name}`: the saved name
//!   (trimmed, at most 80 characters, empty = `New folder`).
//!
//! The rows re-stream over the per-user `sidebar_layout` shape.

use domain::sidebar_folders::{SidebarEntryWrite, SidebarSection, SidebarWrites};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `sidebarLayout.save` input (camelCase on the wire).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarLayoutSaveInput {
    pub team_id: String,
    pub section: SidebarSection,
    pub upserts: Vec<SidebarEntryWrite>,
    pub deletes: Vec<String>,
}

impl SidebarLayoutSaveInput {
    /// One move's writes for a team's section.
    pub fn new(team_id: &str, section: SidebarSection, writes: &SidebarWrites) -> Self {
        Self { team_id: team_id.to_string(), section, upserts: writes.upserts.clone(), deletes: writes.deletes.clone() }
    }
}

/// `sidebarLayout.save` output.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarLayoutSaveOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `sidebarLayout.rename` output: the name as the server saved it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarLayoutRenameOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub name: String,
}

/// `sidebarLayout.save` — mutation.
pub fn sidebar_layout_save(trpc: &TrpcClient, input: &SidebarLayoutSaveInput) -> Result<SidebarLayoutSaveOutput, ApiError> {
    trpc.mutation("sidebarLayout.save", input)
}

/// `sidebarLayout.rename` — mutation.
pub fn sidebar_layout_rename(trpc: &TrpcClient, id: &str, name: &str) -> Result<SidebarLayoutRenameOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        name: &'a str,
    }
    trpc.mutation("sidebarLayout.rename", &Input { id, name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use domain::sidebar_folders::SidebarEntryKind;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn save_posts_the_writes_camel_case() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7}}}"#);
        let writes = SidebarWrites {
            upserts: vec![SidebarEntryWrite {
                id: None,
                kind: SidebarEntryKind::Item,
                target_id: Some("b-1".into()),
                parent_id: Some("f-1".into()),
                name: None,
                sort_order: 2.0,
            }],
            deletes: vec!["f-0".into()],
            created_folder_id: None,
        };
        let out = sidebar_layout_save(&client(&base), &SidebarLayoutSaveInput::new("t-1", SidebarSection::Board, &writes)).unwrap();
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sidebarLayout.save HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"teamId":"t-1","section":"board","upserts":[{"id":null,"kind":"item","targetId":"b-1","parentId":"f-1","name":null,"sortOrder":2}],"deletes":["f-0"]}"#
        ));
    }

    #[test]
    fn rename_posts_id_and_name_and_reads_the_saved_name() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":9,"name":"Ops"}}}"#);
        let out = sidebar_layout_rename(&client(&base), "f-1", "  Ops ").unwrap();
        assert_eq!((out.tx_id, out.name.as_str()), (Some(9), "Ops"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/sidebarLayout.rename HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"f-1","name":"  Ops "}"#));
    }
}
