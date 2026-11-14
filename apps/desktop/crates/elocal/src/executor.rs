//! The local executor: `api::Transport` over a Local desktop's store. Every
//! procedure local-parity.json lists is answered here with the output shape
//! the server's router gives (what the typed wrappers in `api` decode); any
//! other procedure is refused with the fixture's words as HTTP 501 (a plain
//! refusal in the UI, never a sign-in). The rows the parity module derives
//! (issues, comments, sessions, the assistant) are written as they come;
//! the rest keeps the server's column names on the synced-shape tables.
//! Attachment bytes are served from the blob store as `/api/attachments/{id}`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::{ApiError, Blob, CallKind, Transport};
use serde_json::{json, Value};

use crate::blobs::FsBlobStore;
use crate::desktops::DesktopMeta;
use crate::identity::LocalIdentity;
use crate::parity::{self, Row, StatusRow};
use crate::store::LocalStore;
use crate::time;

/// The cap the server holds on started statuses (contract `issueStatusCategory.startedMax`).
const STARTED_MAX: usize = 4;

/// The server's `COMPOSER_DRAFT_TEXT_MAX` (apps/web lib/trpc/composer-drafts.ts).
const COMPOSER_DRAFT_TEXT_MAX: usize = 100_000;
/// The longest key the server takes (`COMPOSER_DRAFT_KEY_MAX`, the column's varchar(80)).
const COMPOSER_DRAFT_KEY_MAX: usize = 80;

pub struct LocalExecutor {
    store: Arc<LocalStore>,
    blobs: FsBlobStore,
    meta: Mutex<DesktopMeta>,
    meta_path: PathBuf,
    team_id: String,
    base_url: String,
    txid: AtomicI64,
}

/// A refusal: the HTTP status the seam reports and its message.
#[derive(Debug)]
struct Refusal(u16, String);

type Out = Result<Value, Refusal>;

fn bad(message: impl Into<String>) -> Refusal {
    Refusal(400, message.into())
}

fn not_found(what: &str) -> Refusal {
    Refusal(404, format!("{what} not found"))
}

fn internal(message: String) -> Refusal {
    Refusal(500, message)
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn text<'a>(input: &'a Row, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

/// The branch `HEAD` names in a working copy (`ref: refs/heads/<branch>`).
fn head_branch(repo: &std::path::Path) -> Option<String> {
    let head = std::fs::read_to_string(repo.join(".git").join("HEAD")).ok()?;
    head.trim().strip_prefix("ref: refs/heads/").map(str::to_string)
}

fn need<'a>(input: &'a Row, key: &str) -> Result<&'a str, Refusal> {
    text(input, key).filter(|s| !s.is_empty()).ok_or_else(|| bad(format!("{key} is required")))
}

fn ids_of(input: &Row, key: &str) -> Vec<String> {
    input.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn obj(v: Value) -> Row {
    v.as_object().cloned().unwrap_or_default()
}

impl LocalExecutor {
    pub fn new(store: Arc<LocalStore>, blobs: FsBlobStore, meta: DesktopMeta, meta_path: PathBuf, desktop_id: String) -> Self {
        Self { store, blobs, meta: Mutex::new(meta), meta_path, base_url: format!("local://{desktop_id}"), team_id: desktop_id, txid: AtomicI64::new(1) }
    }

    pub fn identity(&self) -> LocalIdentity {
        self.meta.lock().expect("meta").identity.clone()
    }

    pub fn device_id(&self) -> String {
        self.meta.lock().expect("meta").device_id.clone()
    }

    pub fn team_id(&self) -> &str {
        &self.team_id
    }

    /// The sidecar as it stands (the identity, the device, the bodies).
    pub fn meta(&self) -> DesktopMeta {
        self.meta.lock().expect("meta").clone()
    }

    pub fn store(&self) -> &Arc<LocalStore> {
        &self.store
    }

    pub fn blobs(&self) -> &FsBlobStore {
        &self.blobs
    }

    fn with_meta<T>(&self, f: impl FnOnce(&mut DesktopMeta) -> T) -> T {
        let mut meta = self.meta.lock().expect("meta");
        let out = f(&mut meta);
        if let Ok(text) = serde_json::to_string_pretty(&*meta) {
            if let Err(e) = std::fs::write(&self.meta_path, text) {
                log::warn!("desktop.json: {e}");
            }
        }
        out
    }

    /// #EXP-1727: the linked folder `id` names.
    pub fn repository(&self, id: &str) -> Option<crate::desktops::LocalRepository> {
        self.meta.lock().expect("meta").repositories.iter().find(|r| r.id == id).cloned()
    }

    /// The repository of `board_id`'s row, if it links one.
    pub fn board_repository(&self, board_id: &str) -> Option<crate::desktops::LocalRepository> {
        let board = self.row("boards", board_id).ok().flatten()?;
        self.repository(text(&board, "repository_id")?)
    }

    fn txid(&self) -> i64 {
        self.txid.fetch_add(1, Ordering::Relaxed)
    }

    // -- the store, with refusals ----------------------------------------------

    fn rows(&self, table: &str) -> Result<Vec<Row>, Refusal> {
        self.store.rows(table).map_err(internal)
    }

    fn rows_where(&self, table: &str, column: &str, value: &str) -> Result<Vec<Row>, Refusal> {
        self.store.rows_where(table, column, value).map_err(internal)
    }

    fn row(&self, table: &str, id: &str) -> Result<Option<Row>, Refusal> {
        self.store.row(table, id).map_err(internal)
    }

    fn row_or(&self, table: &str, id: &str, what: &str) -> Result<Row, Refusal> {
        self.row(table, id)?.ok_or_else(|| not_found(what))
    }

    fn write(&self, table: &str, row: Row) -> Result<(), Refusal> {
        self.store.write(table, row).map_err(internal)
    }

    fn write_many(&self, table: &str, rows: Vec<Row>) -> Result<(), Refusal> {
        self.store.write_many(table, rows).map_err(internal)
    }

    fn delete(&self, table: &str, key: &str) -> Result<(), Refusal> {
        self.store.delete(table, key).map_err(internal)
    }

    fn delete_many(&self, table: &str, keys: &[String]) -> Result<(), Refusal> {
        self.store.delete_many(table, keys).map_err(internal)
    }

    fn statuses(&self) -> Result<Vec<StatusRow>, Refusal> {
        Ok(self.rows("issue_statuses")?.iter().map(StatusRow::from_row).collect())
    }

    /// Patches a row's columns from `patch` and stamps it.
    fn patch_row(&self, table: &str, id: &str, what: &str, patch: Row) -> Result<Row, Refusal> {
        let patched = self
            .store
            .modify(table, id, |row| {
                let mut row = row.clone();
                for (k, v) in patch {
                    row.insert(k, v);
                }
                row.insert("updated_at".into(), json!(time::now()));
                Some(row)
            })
            .map_err(internal)?;
        patched.ok_or_else(|| not_found(what))
    }

    // -- dispatch ----------------------------------------------------------------

    fn dispatch(&self, path: &str, input: Row, session_id: Option<&str>) -> Out {
        if !parity::is_local_procedure(path) {
            return Err(Refusal(501, parity::refusal(path)));
        }
        match path {
            "public.ping" => Ok(json!(true)),
            "teams.getDefault" => Ok(json!({"team": self.team_out()?})),
            "teams.update" => self.teams_update(input),
            "teams.getAgentPrompt" => Ok(json!({"agentPrompt": self.meta.lock().expect("meta").agent_prompt, "updatedAt": null})),
            "boards.create" => self.boards_create(input),
            "boards.update" => self.boards_update(input),
            "boards.delete" | "boards.archive" => self.boards_remove(input),
            "boards.unarchive" | "boards.restore" => Ok(json!({"ok": true, "txid": self.txid()})),
            "boards.listArchived" | "boards.listDeleted" => Ok(json!([])),
            "statuses.create" => self.statuses_create(input),
            "statuses.update" => self.statuses_update(input),
            "statuses.move" => self.statuses_move(input),
            "statuses.referencingCount" => self.statuses_referencing_count(input),
            "statuses.delete" => self.statuses_delete(input),
            "labels.create" => self.labels_create(input),
            "labels.update" => self.labels_update(input),
            "labels.delete" => self.labels_delete(input),
            "issueLabels.add" => self.issue_labels_set(&[need(&input, "issueId")?.to_string()], need(&input, "labelId")?, true),
            "issueLabels.remove" => self.issue_labels_set(&[need(&input, "issueId")?.to_string()], need(&input, "labelId")?, false),
            "issueLabels.bulkAdd" => self.issue_labels_set(&ids_of(&input, "issueIds"), need(&input, "labelId")?, true),
            "issueLabels.bulkRemove" => self.issue_labels_set(&ids_of(&input, "issueIds"), need(&input, "labelId")?, false),
            "issues.create" => self.issues_create(input),
            "issues.update" => self.issues_update(input),
            "issues.delete" => self.issues_delete(input, session_id),
            "issues.move" => self.issues_move(input),
            "issues.get" => self.issues_get(input),
            "issues.search" => self.issues_search(input),
            "issues.bulkUpdate" => self.issues_bulk_update(input),
            "issues.bulkDelete" => self.issues_bulk_delete(input, session_id),
            "issues.listDeleted" => self.issues_list_deleted(),
            "issues.restore" => self.issues_restore(input, session_id),
            "issues.purge" => self.issues_purge(input, session_id),
            "issueDrafts.upsert" => self.drafts_upsert(input),
            "issueDrafts.delete" => self.drafts_delete(input),
            // A Local desktop's files ride an issue, never a draft: none to list.
            "issueDrafts.listAttachments" => Ok(json!([])),
            "composerDrafts.upsert" => self.composer_drafts_upsert(input),
            "composerDrafts.delete" => self.composer_drafts_delete(input),
            "sidebarLayout.save" => self.sidebar_layout_save(input),
            "sidebarLayout.rename" => self.sidebar_layout_rename(input),
            "comments.create" => self.comments_create(input),
            "comments.update" => self.comments_update(input),
            "comments.delete" => self.comments_delete(input),
            "attachments.delete" => self.attachments_delete(input),
            "sessions.send" => self.sessions_send(input),
            "sessions.end" => self.sessions_end(input),
            "sessions.delete" => self.sessions_delete(input),
            "sessions.stop" => Ok(json!({"txid": self.txid(), "delivered": true})),
            "sessions.configure" => self.sessions_configure(input),
            "sessions.linkIssues" => self.sessions_link(input, true),
            "sessions.unlinkIssue" => self.sessions_link(input, false),
            "sessions.markRead" => self.sessions_mark_read(input),
            "sessions.setRead" => self.sessions_set_flag(input, "read_at", "read"),
            "sessions.setArchived" => self.sessions_set_flag(input, "archived_at", "archived"),
            "agents.list" => Ok(json!({"agents": self.agents_out()?})),
            "agents.get" => self.agents_get(input),
            "agents.ensureAssistant" => self.agents_ensure_assistant(),
            "agents.create" => self.agents_create(input),
            "agents.update" => self.agents_update(input),
            "agents.delete" => self.agents_delete(input),
            "prompts.list" => Ok(json!({"prompts": self.prompts_out()?})),
            "prompts.get" => self.prompts_get(input),
            "prompts.create" => self.prompts_create(input),
            "prompts.update" => self.prompts_update(input),
            "prompts.delete" => self.prompts_delete(input),
            "apps.records.upsert" => self.records_upsert(input),
            "apps.records.delete" => self.records_delete(input),
            "apps.records.batch" => self.records_batch(input),
            "apps.records.history" => Ok(json!({"events": []})),
            "repositories.linkFolder" => self.repositories_link_folder(input),
            "repositories.listLocal" => Ok(json!({"repositories": self.meta.lock().expect("meta").repositories.iter().map(Self::repository_out).collect::<Vec<_>>()})),
            "repositories.forIssue" => self.repositories_for_issue(input),
            other => Err(Refusal(501, parity::refusal(other))),
        }
    }

    // -- teams -------------------------------------------------------------------

    fn team_out(&self) -> Result<Value, Refusal> {
        let team = self.rows("teams")?.into_iter().next().ok_or_else(|| not_found("Team"))?;
        Ok(json!({"id": team["id"], "name": team["name"], "slug": team["slug"]}))
    }

    fn teams_update(&self, input: Row) -> Out {
        let mut patch = Row::new();
        for (key, column) in [("name", "name"), ("estimationType", "estimation_type"), ("mergeMode", "merge_mode"), ("yoloMode", "yolo_mode")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        for (key, column) in [("iconUrl", "icon_url"), ("timeZone", "time_zone")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        if let Some(prompt) = text(&input, "agentPrompt") {
            self.with_meta(|m| m.agent_prompt = prompt.to_string());
        }
        self.patch_row("teams", &self.team_id, "Team", patch)?;
        Ok(json!({"team": self.team_out()?, "txid": self.txid()}))
    }

    // -- boards ------------------------------------------------------------------

    fn board_out(row: &Row) -> Value {
        json!({"id": row["id"], "teamId": row["team_id"], "name": row["name"], "slug": row["slug"], "prefix": row["prefix"], "color": row["color"], "repositoryId": null, "defaultBranch": null})
    }

    fn boards_create(&self, input: Row) -> Out {
        let name = need(&input, "name")?.trim().to_string();
        let prefix = need(&input, "prefix")?.trim().to_uppercase();
        if prefix.is_empty() || prefix.len() > 10 || !prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(bad("A prefix is 1 to 10 letters or digits"));
        }
        let boards = self.rows("boards")?;
        if boards.iter().any(|b| text(b, "prefix") == Some(prefix.as_str())) {
            return Err(Refusal(409, format!("The prefix {prefix} is taken")));
        }
        let mut slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect::<String>().trim_matches('-').to_string();
        if slug.is_empty() {
            slug = "board".into();
        }
        while boards.iter().any(|b| text(b, "slug") == Some(slug.as_str())) {
            slug.push_str("-2");
        }
        let now = time::now();
        let sort_order = boards.iter().filter_map(|b| b.get("sort_order").and_then(Value::as_f64)).fold(0.0, f64::max) + 1.0;
        let row = obj(json!({"id": new_id(), "team_id": self.team_id, "name": name, "slug": slug, "prefix": prefix, "color": text(&input, "color").unwrap_or("#6366f1"), "icon": input.get("icon").cloned().unwrap_or(Value::Null), "repository_id": null, "default_branch": null, "sort_order": sort_order, "created_at": now, "updated_at": now}));
        self.write("boards", row.clone())?;
        Ok(json!({"board": Self::board_out(&row), "txid": self.txid()}))
    }

    fn boards_update(&self, input: Row) -> Out {
        let id = need(&input, "boardId")?;
        let mut patch = Row::new();
        if let Some(repo) = input.get("repositoryId") {
            if let Some(repo) = repo.as_str().filter(|r| self.repository(r).is_none()) {
                return Err(not_found(&format!("Repository {repo}")));
            }
        }
        for (key, column) in [("name", "name"), ("color", "color"), ("icon", "icon"), ("repositoryId", "repository_id")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        let row = self.patch_row("boards", id, "Board", patch)?;
        Ok(json!({"board": Self::board_out(&row), "txid": self.txid()}))
    }

    // -- repositories (#EXP-1727) ------------------------------------------

    fn repository_out(repo: &crate::desktops::LocalRepository) -> Value {
        json!({"id": repo.id, "fullName": repo.name, "defaultBranch": repo.default_branch, "path": repo.path})
    }

    /// Links a git working copy on this machine; `boardId` points that board at it.
    fn repositories_link_folder(&self, input: Row) -> Out {
        let path = std::path::PathBuf::from(need(&input, "path")?);
        if !path.join(".git").exists() {
            return Err(bad("That folder is not a git repository"));
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("repository").to_string();
        let default_branch = head_branch(&path).unwrap_or_else(|| "main".into());
        let repo = self.with_meta(|meta| match meta.repositories.iter().find(|r| r.path == path) {
            Some(existing) => existing.clone(),
            None => {
                let repo = crate::desktops::LocalRepository { id: uuid::Uuid::new_v4().to_string(), name, path: path.clone(), default_branch };
                meta.repositories.push(repo.clone());
                repo
            }
        });
        if let Some(board) = text(&input, "boardId") {
            let mut patch = Row::new();
            patch.insert("repository_id".into(), json!(repo.id));
            self.patch_row("boards", board, "Board", patch)?;
        }
        Ok(json!({"repository": Self::repository_out(&repo), "txid": self.txid()}))
    }

    fn repositories_for_issue(&self, input: Row) -> Out {
        let issue = self.row("issues", need(&input, "issueId")?)?.ok_or_else(|| not_found("Issue"))?;
        let repo = text(&issue, "board_id").and_then(|b| self.board_repository(b));
        Ok(repo.map(|r| json!({"repositoryId": r.id, "fullName": r.name, "defaultBranch": r.default_branch})).unwrap_or(Value::Null))
    }

    /// A trashed or archived board leaves the local store with its issues
    /// (the synced shapes hold neither; nothing to restore from).
    fn boards_remove(&self, input: Row) -> Out {
        let id = need(&input, "boardId")?;
        self.row_or("boards", id, "Board")?;
        let issues: Vec<String> = self.rows_where("issues", "board_id", id)?.iter().filter_map(|i| text(i, "id")).map(str::to_string).collect();
        self.issues_remove(&issues)?;
        // #EXP-1799: the board goes for good here, its trashed issues with it.
        let trashed: Vec<String> = self.meta.lock().expect("meta").issue_trash.iter().filter(|t| text(&t.issue, "board_id") == Some(id)).map(|t| t.id().to_string()).collect();
        for issue in trashed {
            self.purge_issue(&issue, Some(Actor::of(self, None)))?;
        }
        self.delete("boards", id)?;
        Ok(json!({"ok": true, "txid": self.txid()}))
    }

    // -- statuses ----------------------------------------------------------------

    fn status_out(row: &Row) -> Value {
        json!({"id": row["id"], "teamId": row["team_id"], "category": row["category"], "name": row["name"], "color": row["color"], "builtinKey": row.get("builtin_key").cloned().unwrap_or(Value::Null)})
    }

    fn statuses_create(&self, input: Row) -> Out {
        let category = need(&input, "category")?.to_string();
        let name = need(&input, "name")?.trim().to_string();
        let rows = self.rows("issue_statuses")?;
        if rows.iter().any(|s| text(s, "name").is_some_and(|n| n.eq_ignore_ascii_case(&name))) {
            return Err(Refusal(409, format!("A status named {name} exists")));
        }
        if category == "started" && rows.iter().filter(|s| text(s, "category") == Some("started")).count() >= STARTED_MAX {
            return Err(bad(format!("A team can have at most {STARTED_MAX} started statuses")));
        }
        let sort_order = rows.iter().filter(|s| text(s, "category") == Some(category.as_str())).filter_map(|s| s.get("sort_order").and_then(Value::as_f64)).fold(0.0, f64::max) + 1.0;
        let now = time::now();
        let row = obj(json!({"id": new_id(), "team_id": self.team_id, "category": category, "name": name, "color": text(&input, "color").unwrap_or("#A1A1AA"), "sort_order": sort_order, "builtin_key": null, "created_at": now, "updated_at": now}));
        self.write("issue_statuses", row.clone())?;
        Ok(json!({"status": Self::status_out(&row), "txid": self.txid()}))
    }

    fn statuses_update(&self, input: Row) -> Out {
        let id = need(&input, "statusId")?;
        let mut patch = Row::new();
        for key in ["name", "color"] {
            if let Some(v) = input.get(key) {
                patch.insert(key.into(), v.clone());
            }
        }
        self.patch_row("issue_statuses", id, "Status", patch)?;
        Ok(json!({"txid": self.txid()}))
    }

    fn statuses_move(&self, input: Row) -> Out {
        let id = need(&input, "statusId")?;
        let up = text(&input, "direction") == Some("up");
        let row = self.row_or("issue_statuses", id, "Status")?;
        let category = text(&row, "category").unwrap_or_default().to_string();
        let mut siblings: Vec<Row> = self.rows_where("issue_statuses", "category", &category)?;
        siblings.sort_by(|a, b| a.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0).partial_cmp(&b.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
        let Some(i) = siblings.iter().position(|s| text(s, "id") == Some(id)) else { return Ok(json!({"txid": self.txid()})) };
        let j = if up { i.checked_sub(1) } else { (i + 1 < siblings.len()).then_some(i + 1) };
        if let Some(j) = j {
            siblings.swap(i, j);
            let now = time::now();
            let rows: Vec<Row> = siblings.into_iter().enumerate().map(|(n, mut s)| { s.insert("sort_order".into(), json!((n + 1) as f64)); s.insert("updated_at".into(), json!(now)); s }).collect();
            self.write_many("issue_statuses", rows)?;
        }
        Ok(json!({"txid": self.txid()}))
    }

    fn statuses_referencing_count(&self, input: Row) -> Out {
        let id = need(&input, "statusId")?;
        Ok(json!({"count": self.rows_where("issues", "status_id", id)?.len()}))
    }

    fn statuses_delete(&self, input: Row) -> Out {
        let id = need(&input, "statusId")?;
        let row = self.row_or("issue_statuses", id, "Status")?;
        if row.get("builtin_key").is_some_and(|k| !k.is_null()) {
            return Err(bad("A builtin status cannot be deleted"));
        }
        let statuses = self.statuses()?;
        let target = text(&input, "reassignToId").filter(|t| statuses.iter().any(|s| s.id == *t)).map(str::to_string).or_else(|| statuses.iter().find(|s| s.builtin_key.as_deref() == Some("backlog")).map(|s| s.id.clone()));
        let referencing = self.rows_where("issues", "status_id", id)?;
        let reassigned = referencing.len();
        let now = time::now();
        let moved: Vec<Row> = referencing.into_iter().map(|issue| parity::apply_issue_patch(&issue, &obj(json!({"statusId": target})), &statuses, &now)).collect();
        if !moved.is_empty() {
            self.write_many("issues", moved)?;
        }
        self.delete("issue_statuses", id)?;
        Ok(json!({"reassigned": reassigned, "reassignedToId": target, "txid": self.txid()}))
    }

    // -- labels ------------------------------------------------------------------

    fn labels_create(&self, input: Row) -> Out {
        let name = need(&input, "name")?.trim().to_string();
        if self.rows("labels")?.iter().any(|l| text(l, "name").is_some_and(|n| n.eq_ignore_ascii_case(&name))) {
            return Err(Refusal(409, format!("A label named {name} exists")));
        }
        let now = time::now();
        let row = obj(json!({"id": new_id(), "team_id": self.team_id, "name": name, "color": text(&input, "color").unwrap_or("#6366f1"), "sort_order": 0, "created_at": now, "updated_at": now}));
        self.write("labels", row.clone())?;
        Ok(json!({"label": {"id": row["id"], "teamId": row["team_id"], "name": row["name"], "color": row["color"]}, "txid": self.txid()}))
    }

    fn labels_update(&self, input: Row) -> Out {
        let id = need(&input, "labelId")?;
        let mut patch = Row::new();
        for key in ["name", "color"] {
            if let Some(v) = input.get(key) {
                patch.insert(key.into(), v.clone());
            }
        }
        self.patch_row("labels", id, "Label", patch)?;
        Ok(json!({"txid": self.txid()}))
    }

    fn labels_delete(&self, input: Row) -> Out {
        let id = need(&input, "labelId")?;
        self.row_or("labels", id, "Label")?;
        let pairs: Vec<String> = self.rows_where("issue_labels", "label_id", id)?.iter().filter_map(|p| text(p, "issue_id").map(|i| format!("{i}/{id}"))).collect();
        self.delete_many("issue_labels", &pairs)?;
        self.delete("labels", id)?;
        Ok(json!({"txid": self.txid()}))
    }

    fn issue_labels_set(&self, issue_ids: &[String], label_id: &str, on: bool) -> Out {
        self.row_or("labels", label_id, "Label")?;
        if on {
            let now = time::now();
            let rows: Vec<Row> = issue_ids.iter().map(|issue| obj(json!({"issue_id": issue, "label_id": label_id, "team_id": self.team_id, "created_at": now}))).collect();
            self.write_many("issue_labels", rows)?;
        } else {
            let keys: Vec<String> = issue_ids.iter().map(|issue| format!("{issue}/{label_id}")).collect();
            self.delete_many("issue_labels", &keys)?;
        }
        Ok(json!({"txid": self.txid()}))
    }

    // -- issues ------------------------------------------------------------------

    fn issue_out(row: &Row) -> Value {
        json!({"id": row["id"], "identifier": row["identifier"], "boardId": row["board_id"], "title": row["title"], "status": row["status"], "statusId": row.get("status_id").cloned().unwrap_or(Value::Null), "priority": row["priority"], "assigneeId": row.get("assignee_id").cloned().unwrap_or(Value::Null), "assigneeAgentId": row.get("assignee_agent_id").cloned().unwrap_or(Value::Null), "dueDate": row.get("due_date").cloned().unwrap_or(Value::Null), "number": row.get("number").cloned().unwrap_or(Value::Null), "description": row.get("description").cloned().unwrap_or(Value::Null)})
    }

    fn board_numbers(&self, board_id: &str) -> Result<Vec<i64>, Refusal> {
        Ok(self.rows_where("issues", "board_id", board_id)?.iter().filter_map(|i| i.get("number").and_then(Value::as_i64)).collect())
    }

    fn issues_create(&self, input: Row) -> Out {
        let board_id = need(&input, "boardId")?;
        let board = self.row_or("boards", board_id, "Board")?;
        if need(&input, "title")?.trim().is_empty() {
            return Err(bad("A title is required"));
        }
        let statuses = self.statuses()?;
        let numbers = self.board_numbers(board_id)?;
        let identity = self.identity();
        let id = new_id();
        let now = time::now();
        let ctx = parity::IssueContext { team_id: &self.team_id, board_prefix: text(&board, "prefix").unwrap_or("X"), numbers: &numbers, statuses: &statuses, identity: &identity, id: &id, now: &now };
        let row = parity::create_issue_row(&input, &ctx);
        self.write("issues", row.clone())?;
        // Filed from a draft: the draft goes (it IS this issue now).
        if let Some(draft) = text(&input, "draftId") {
            self.delete("issue_drafts", draft)?;
        }
        let labels = ids_of(&input, "labelIds");
        if !labels.is_empty() {
            let rows: Vec<Row> = labels.iter().map(|label| obj(json!({"issue_id": id, "label_id": label, "team_id": self.team_id, "created_at": now}))).collect();
            self.write_many("issue_labels", rows)?;
        }
        if let Some(session) = text(&input, "sessionId") {
            self.write("session_issues", obj(json!({"id": new_id(), "session_id": session, "issue_id": id, "team_id": self.team_id, "source": "user", "added_by": identity.user_id, "added_by_session_id": null, "sort_order": 0, "created_at": now, "updated_at": now})))?;
        }
        Ok(json!({"issue": Self::issue_out(&row), "txid": self.txid()}))
    }

    /// #EDE-65 J3b: the person's draft (local-parity.json `drafts`).
    fn drafts_upsert(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let board = self.row_or("boards", need(&input, "boardId")?, "Board")?;
        if text(&board, "team_id") != Some(need(&input, "teamId")?) {
            return Err(bad("The board must belong to this team"));
        }
        if let Some(filed) = self.rows_where("issues", "draft_id", id)?.first() {
            return Err(Refusal(409, parity::draft_consumed_refusal(text(filed, "identifier").unwrap_or(""))));
        }
        let existing = self.row("issue_drafts", id)?;
        let row = parity::draft_row(&input, &self.identity(), existing.as_ref(), &time::now());
        self.write("issue_drafts", row.clone())?;
        Ok(json!({"draft": Value::Object(row), "txId": self.txid()}))
    }

    fn drafts_delete(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let deleted = self.row("issue_drafts", id)?.is_some();
        if deleted {
            self.delete("issue_drafts", id)?;
        }
        Ok(json!({"deleted": deleted, "txId": self.txid()}))
    }

    /// #EXP-1800: the identity's composer draft for one key (local-parity.json
    /// `composerDrafts`, the server's `composerDrafts.upsert`): one row per
    /// key, an empty text refused (that is a delete).
    fn composer_drafts_upsert(&self, input: Row) -> Out {
        let key = Self::composer_draft_key(&input)?;
        if need(&input, "teamId")? != self.team_id {
            return Err(not_found("Team"));
        }
        let draft = text(&input, "text").ok_or_else(|| bad("text is required"))?;
        if draft.encode_utf16().count() > COMPOSER_DRAFT_TEXT_MAX {
            return Err(bad("The draft is too long"));
        }
        if draft.trim().is_empty() {
            return Err(bad(parity::composer_draft_empty_refusal()));
        }
        let existing = self.composer_draft(key)?;
        let row = parity::composer_draft_row(&input, &self.identity(), existing.as_ref(), &new_id(), &time::now());
        self.write("composer_drafts", row.clone())?;
        Ok(json!({"draft": Value::Object(row), "txId": self.txid()}))
    }

    /// #EXP-1800: drop the identity's composer draft for one key (a send, an
    /// emptied field, a discard); deleting nothing is fine.
    fn composer_drafts_delete(&self, input: Row) -> Out {
        let key = Self::composer_draft_key(&input)?;
        let existing = self.composer_draft(key)?;
        if let Some(id) = existing.as_ref().and_then(|row| text(row, "id")) {
            self.delete("composer_drafts", id)?;
        }
        Ok(json!({"deleted": existing.is_some(), "txId": self.txid()}))
    }

    /// The input's key, refused as the server's zod schema refuses it.
    fn composer_draft_key(input: &Row) -> Result<&str, Refusal> {
        let key = need(input, "key")?;
        if key.len() > COMPOSER_DRAFT_KEY_MAX || domain::composer_draft::parse_key(key).is_none() {
            return Err(bad("Unknown composer draft key"));
        }
        Ok(key)
    }

    /// The identity's row for `key` (one per user and key).
    fn composer_draft(&self, key: &str) -> Result<Option<Row>, Refusal> {
        let user = self.identity().user_id;
        Ok(self.rows_where("composer_drafts", "key", key)?.into_iter().find(|row| text(row, "user_id") == Some(user.as_str())))
    }

    // -- sidebar layout (#EXP-1832) ----------------------------------------------

    /// The identity's `sidebar_layout` rows.
    fn sidebar_rows(&self) -> Result<Vec<Row>, Refusal> {
        let user = self.identity().user_id;
        self.rows_where("sidebar_layout", "user_id", &user)
    }

    /// #EXP-1832: one move's rows (local-parity.json `sidebarLayout`, the
    /// server's `sidebarLayout.save`): the identity's own rows only, a folder
    /// at the top level under its id, an item upserted by (user, section,
    /// target) with its parent a folder of the section, only the section's
    /// folders deleted (their items fall to the top level).
    fn sidebar_layout_save(&self, input: Row) -> Out {
        if need(&input, "teamId")? != self.team_id {
            return Err(not_found("Team"));
        }
        let section = need(&input, "section")?;
        if domain::sidebar_folders::SidebarSection::parse(section).is_none() {
            return Err(bad("Unknown section"));
        }
        let upserts: Vec<Row> = input.get("upserts").and_then(Value::as_array).map(|a| a.iter().map(|w| obj(w.clone())).collect()).unwrap_or_default();
        let deletes = ids_of(&input, "deletes");
        let mine = self.sidebar_rows()?;
        let of_section = |row: &Row| text(row, "team_id") == Some(self.team_id.as_str()) && text(row, "section") == Some(section);
        let is_folder = |row: &Row| text(row, "kind") == Some("folder");
        let mut folder_ids: std::collections::HashSet<String> =
            mine.iter().filter(|row| is_folder(row) && of_section(row)).filter_map(|row| text(row, "id")).filter(|id| !deletes.iter().any(|d| d == id)).map(str::to_string).collect();
        for write in &upserts {
            match text(write, "kind") {
                Some("folder") => {
                    let id = text(write, "id").filter(|id| !id.is_empty());
                    let (Some(id), None, None) = (id, text(write, "targetId"), text(write, "parentId")) else {
                        return Err(bad("A folder has an id and no target or parent; an item has a target"));
                    };
                    match mine.iter().find(|row| text(row, "id") == Some(id)) {
                        Some(row) if !(is_folder(row) && of_section(row)) => return Err(bad("Not a folder of this section")),
                        Some(_) => {}
                        None if self.row("sidebar_layout", id)?.is_some() => return Err(bad("Folder id taken")),
                        None => {}
                    }
                    folder_ids.insert(id.to_string());
                }
                Some("item") if text(write, "targetId").is_some_and(|t| !t.is_empty()) => {}
                _ => return Err(bad("A folder has an id and no target or parent; an item has a target")),
            }
            if !write.get("sortOrder").is_some_and(Value::is_number) {
                return Err(bad("sortOrder is required"));
            }
        }
        for write in &upserts {
            if text(write, "kind") == Some("item") {
                if let Some(parent) = text(write, "parentId") {
                    if !folder_ids.contains(parent) {
                        return Err(bad("An item's parent must be a folder of this section"));
                    }
                }
            }
        }
        for id in &deletes {
            if let Some(row) = mine.iter().find(|row| text(row, "id") == Some(id.as_str())) {
                if !(is_folder(row) && of_section(row)) {
                    return Err(bad("Only this section's folders are deleted"));
                }
            }
        }

        let user = self.identity().user_id;
        let now = time::now();
        let mut rows = Vec::with_capacity(upserts.len());
        for write in &upserts {
            let sort_order = write["sortOrder"].clone();
            if text(write, "kind") == Some("folder") {
                let id = text(write, "id").unwrap_or_default();
                let name = domain::sidebar_folders::sidebar_folder_name(text(write, "name").unwrap_or(""));
                let existing = mine.iter().find(|row| text(row, "id") == Some(id));
                let created = existing.and_then(|row| row.get("created_at").cloned()).unwrap_or_else(|| json!(now));
                rows.push(obj(json!({"id": id, "user_id": user, "team_id": self.team_id, "section": section, "kind": "folder", "target_id": null, "parent_id": null, "name": name, "sort_order": sort_order, "created_at": created, "updated_at": now})));
            } else {
                let target = text(write, "targetId").unwrap_or_default();
                let existing = mine.iter().find(|row| text(row, "section") == Some(section) && text(row, "target_id") == Some(target));
                let id = existing.and_then(|row| text(row, "id")).map(str::to_string).unwrap_or_else(new_id);
                let created = existing.and_then(|row| row.get("created_at").cloned()).unwrap_or_else(|| json!(now));
                let name = existing.and_then(|row| row.get("name").cloned()).unwrap_or(Value::Null);
                rows.push(obj(json!({"id": id, "user_id": user, "team_id": self.team_id, "section": section, "kind": "item", "target_id": target, "parent_id": text(write, "parentId"), "name": name, "sort_order": sort_order, "created_at": created, "updated_at": now})));
            }
        }
        if !rows.is_empty() {
            self.write_many("sidebar_layout", rows)?;
        }
        let gone: Vec<String> = deletes.into_iter().filter(|id| mine.iter().any(|row| text(row, "id") == Some(id.as_str()) && is_folder(row))).collect();
        if !gone.is_empty() {
            self.delete_many("sidebar_layout", &gone)?;
        }
        Ok(json!({"txId": self.txid()}))
    }

    /// #EXP-1832: rename one of the identity's folders (empty = the default).
    fn sidebar_layout_rename(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let name = domain::sidebar_folders::sidebar_folder_name(text(&input, "name").ok_or_else(|| bad("name is required"))?);
        let mine = self.sidebar_rows()?;
        if !mine.iter().any(|row| text(row, "id") == Some(id) && text(row, "kind") == Some("folder")) {
            return Err(not_found("Folder"));
        }
        self.patch_row("sidebar_layout", id, "Folder", obj(json!({"name": name})))?;
        Ok(json!({"txId": self.txid(), "name": name}))
    }

    fn issues_update(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let row = self.row_or("issues", id, "Issue")?;
        let updated = parity::apply_issue_patch(&row, &input, &self.statuses()?, &time::now());
        self.write("issues", updated.clone())?;
        Ok(json!({"issue": Self::issue_out(&updated), "txid": self.txid()}))
    }

    /// Removes issues with what hangs off them.
    fn issues_remove(&self, ids: &[String]) -> Result<(), Refusal> {
        for id in ids {
            let comments: Vec<String> = self.rows_where("comments", "issue_id", id)?.iter().filter_map(|c| text(c, "id")).map(str::to_string).collect();
            self.delete_many("comments", &comments)?;
            let pairs: Vec<String> = self.rows_where("issue_labels", "issue_id", id)?.iter().filter_map(|p| text(p, "label_id").map(|l| format!("{id}/{l}"))).collect();
            self.delete_many("issue_labels", &pairs)?;
            for attachment in self.rows_where("attachments", "issue_id", id)? {
                if let Some(a) = text(&attachment, "id") {
                    self.blobs.remove(a);
                    self.delete("attachments", a)?;
                }
            }
            let links: Vec<String> = self.rows_where("session_issues", "issue_id", id)?.iter().filter_map(|l| text(l, "id")).map(str::to_string).collect();
            self.delete_many("session_issues", &links)?;
        }
        self.delete_many("issues", ids)
    }

    /// #EXP-1799: a delete is the trash's (issue-lifecycle.json `trash`).
    fn issues_delete(&self, input: Row, session_id: Option<&str>) -> Out {
        let id = need(&input, "id")?.to_string();
        self.row_or("issues", &id, "Issue")?;
        self.trash_issues(&[id.clone()], session_id)?;
        Ok(json!({"id": id, "txid": self.txid()}))
    }

    fn issues_move(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let board_id = need(&input, "boardId")?;
        let board = self.row_or("boards", board_id, "Board")?;
        let mut row = self.row_or("issues", id, "Issue")?;
        if text(&row, "board_id") != Some(board_id) {
            let number = parity::next_issue_number(&self.board_numbers(board_id)?);
            row.insert("board_id".into(), json!(board_id));
            row.insert("number".into(), json!(number));
            row.insert("identifier".into(), json!(parity::issue_identifier(text(&board, "prefix").unwrap_or("X"), number)));
            row.insert("updated_at".into(), json!(time::now()));
            self.write("issues", row.clone())?;
        }
        Ok(json!({"issue": Self::issue_out(&row), "txid": self.txid(), "boardSlug": board["slug"]}))
    }

    fn issues_get(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let row = self.rows("issues")?.into_iter().find(|i| text(i, "id") == Some(id) || text(i, "identifier") == Some(id)).ok_or_else(|| not_found("Issue"))?;
        Ok(json!({"issue": {"id": row["id"], "identifier": row["identifier"], "title": row["title"], "description": row.get("description").cloned().unwrap_or(Value::Null), "boardId": row["board_id"], "status": row["status"], "branch": null, "prUrl": null, "prState": null}, "teamId": self.team_id}))
    }

    fn issues_search(&self, input: Row) -> Out {
        let query = text(&input, "query").unwrap_or("").trim().to_lowercase();
        let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
        let mut hits: Vec<Row> = self.rows("issues")?.into_iter().filter(|i| query.is_empty() || text(i, "title").is_some_and(|t| t.to_lowercase().contains(&query)) || text(i, "identifier").is_some_and(|t| t.to_lowercase().contains(&query))).collect();
        hits.sort_by(|a, b| text(b, "updated_at").cmp(&text(a, "updated_at")));
        hits.truncate(limit);
        Ok(Value::Array(hits.iter().map(|i| json!({"id": i["id"], "identifier": i["identifier"], "title": i["title"], "boardId": i["board_id"], "status": i["status"], "statusId": i.get("status_id").cloned().unwrap_or(Value::Null), "priority": i["priority"]})).collect()))
    }

    fn issues_bulk_update(&self, input: Row) -> Out {
        let ids = ids_of(&input, "ids");
        let statuses = self.statuses()?;
        let now = time::now();
        let patch: Row = input.get("patch").and_then(Value::as_object).cloned().unwrap_or_else(|| input.iter().filter(|(k, _)| k.as_str() != "ids").map(|(k, v)| (k.clone(), v.clone())).collect());
        let mut rows = Vec::new();
        for id in &ids {
            if let Some(row) = self.row("issues", id)? {
                rows.push(parity::apply_issue_patch(&row, &patch, &statuses, &now));
            }
        }
        let updated = rows.len();
        self.write_many("issues", rows)?;
        Ok(json!({"updated": updated, "txid": self.txid()}))
    }

    fn issues_bulk_delete(&self, input: Row, session_id: Option<&str>) -> Out {
        // The server's input is `issueIds` (what the api wrapper sends).
        let ids = if input.contains_key("issueIds") { ids_of(&input, "issueIds") } else { ids_of(&input, "ids") };
        let trashed = self.trash_issues(&ids, session_id)?;
        Ok(json!({"deleted": trashed, "txid": self.txid()}))
    }

    // -- comments ----------------------------------------------------------------

    fn comment_out(row: &Row) -> Value {
        json!({"id": row["id"], "issueId": row["issue_id"], "authorId": row.get("author_id").cloned().unwrap_or(Value::Null), "parentId": row.get("parent_id").cloned().unwrap_or(Value::Null), "source": row["source"], "audience": row["audience"], "body": row["body"], "editedAt": row.get("edited_at").cloned().unwrap_or(Value::Null), "createdAt": row["created_at"]})
    }

    fn comments_create(&self, input: Row) -> Out {
        let issue_id = need(&input, "issueId")?;
        self.row_or("issues", issue_id, "Issue")?;
        if need(&input, "body")?.trim().is_empty() {
            return Err(bad("A body is required"));
        }
        let existing: Vec<(String, Option<String>)> = self.rows_where("comments", "issue_id", issue_id)?.iter().map(|c| (text(c, "id").unwrap_or_default().to_string(), text(c, "parent_id").map(str::to_string))).collect();
        let row = parity::create_comment_row(&input, &self.team_id, &self.identity(), &existing, &new_id(), &time::now());
        self.write("comments", row.clone())?;
        let now = time::now();
        for attachment in ids_of(&input, "attachmentIds") {
            if let Some(mut a) = self.row("attachments", &attachment)? {
                a.insert("comment_id".into(), row["id"].clone());
                a.insert("issue_id".into(), json!(issue_id));
                a.insert("updated_at".into(), json!(now));
                self.write("attachments", a)?;
            }
        }
        Ok(json!({"comment": Self::comment_out(&row), "mentionedUserIds": [], "txid": self.txid(), "reporterEmailed": null}))
    }

    fn comments_update(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let body = need(&input, "body")?;
        let row = self.patch_row("comments", id, "Comment", obj(json!({"body": body, "edited_at": time::now()})))?;
        Ok(json!({"comment": Self::comment_out(&row), "txid": self.txid()}))
    }

    fn comments_delete(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        self.row_or("comments", id, "Comment")?;
        let replies: Vec<String> = self.rows_where("comments", "parent_id", id)?.iter().filter_map(|c| text(c, "id")).map(str::to_string).collect();
        self.delete_many("comments", &replies)?;
        self.delete("comments", id)?;
        Ok(json!({"txid": self.txid()}))
    }

    // -- attachments -------------------------------------------------------------

    fn attachments_delete(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        self.row_or("attachments", id, "Attachment")?;
        self.blobs.remove(id);
        self.delete("attachments", id)?;
        Ok(json!({"txid": self.txid()}))
    }

    /// An attachment the host stored: its row (and the bytes in the blob store).
    pub fn attachment_create(&self, filename: &str, content_type: &str, bytes: &[u8], issue_id: Option<&str>) -> Result<Row, String> {
        let id = new_id();
        self.blobs.put(&id, bytes, content_type).map_err(|e| e.to_string())?;
        let now = time::now();
        let board_id = match issue_id {
            Some(issue) => self.store.row("issues", issue)?.and_then(|i| text(&i, "board_id").map(str::to_string)),
            None => None,
        };
        let row = obj(json!({"id": id, "team_id": self.team_id, "issue_id": issue_id, "board_id": board_id, "comment_id": null, "uploader_id": self.identity().user_id, "filename": filename, "content_type": content_type, "size_bytes": bytes.len(), "storage_key": format!("local/{id}"), "url": format!("/api/attachments/{id}"), "width": null, "height": null, "duration_ms": null, "poster_storage_key": null, "as_file": false, "created_at": now, "updated_at": now}));
        self.store.write("attachments", row.clone())?;
        Ok(row)
    }

    // -- sessions ----------------------------------------------------------------

    fn sessions_send(&self, input: Row) -> Out {
        let existing = match text(&input, "sessionId") {
            Some(session_id) => {
                let session = self.row_or("sessions", session_id, "Session")?;
                let conversation = self.rows_where("conversations", "session_id", session_id)?.into_iter().next().ok_or_else(|| not_found("Conversation"))?;
                Some((session, conversation))
            }
            None => None,
        };
        if existing.is_none() && text(&input, "text").map(str::trim).unwrap_or("").is_empty() && text(&input, "name").is_none() {
            return Err(bad("A message needs text"));
        }
        // F21: a key this desktop already stored answers that message again.
        let client_message_id = text(&input, "clientMessageId").filter(|k| !k.is_empty()).map(str::to_string);
        if let Some(sent) = client_message_id.as_deref().and_then(|key| self.client_message(key)) {
            return Ok(sent);
        }
        let identity = self.identity();
        let device_id = self.device_id();
        let now = time::now();
        // The send's attachments as stored (their lines go into the body).
        let attachments = match input.get("attachmentIds").and_then(Value::as_array) {
            Some(ids) => ids.iter().filter_map(Value::as_str).filter_map(|id| self.store.row("attachments", id).ok().flatten()).collect(),
            None => Vec::new(),
        };
        let ctx = parity::SendContext { team_id: &self.team_id, identity: &identity, device_id: &device_id, existing, attachments, now: &now };
        let (rows, (session_id, message_id)) = parity::send_rows(&input, &ctx, &mut new_id);
        for (table, row) in rows {
            self.write(table, row)?;
        }
        if let Some(key) = client_message_id {
            self.with_meta(|meta| {
                meta.client_messages.retain(|c| c.client_message_id != key);
                meta.client_messages.push(crate::desktops::ClientMessage { client_message_id: key, message_id: message_id.clone() });
                let over = meta.client_messages.len().saturating_sub(crate::desktops::CLIENT_MESSAGES_KEPT);
                meta.client_messages.drain(..over);
            });
        }
        Ok(json!({"sessionId": session_id, "messageId": message_id, "txid": self.txid(), "deliverTo": null}))
    }

    /// F21: the result of the send already stored under `key`, `None` when
    /// none is (or its message is gone: the send runs anew).
    fn client_message(&self, key: &str) -> Option<Value> {
        let message_id = self.meta.lock().expect("meta").client_messages.iter().find(|c| c.client_message_id == key)?.message_id.clone();
        let message = self.store.row("messages", &message_id).ok().flatten()?;
        let session_id = text(&message, "session_id").or_else(|| text(&message, "root_id"))?.to_string();
        Some(json!({"sessionId": session_id, "messageId": message_id, "txid": self.txid(), "deliverTo": null}))
    }

    fn sessions_end(&self, input: Row) -> Out {
        let id = need(&input, "sessionId")?;
        let now = time::now();
        self.patch_row("sessions", id, "Session", obj(json!({"status": "ended", "busy": false, "needs_input": false, "ended_at": now})))?;
        Ok(json!({"txid": self.txid()}))
    }

    fn sessions_delete(&self, input: Row) -> Out {
        let id = need(&input, "sessionId")?;
        self.row_or("sessions", id, "Session")?;
        let tree: Vec<String> = self.rows_where("sessions", "root_id", id)?.iter().filter_map(|s| text(s, "id")).map(str::to_string).collect();
        for session in tree.iter().chain(std::iter::once(&id.to_string())) {
            // #EXP-1730: a message lives in its session (`session_id`).
            let messages: Vec<String> = self.rows_where("messages", "session_id", session)?.iter().filter_map(|m| text(m, "id")).map(str::to_string).collect();
            self.delete_many("messages", &messages)?;
            for conversation in self.rows_where("conversations", "session_id", session)? {
                if let Some(c) = text(&conversation, "id") {
                    self.delete("conversations", c)?;
                }
            }
            let links: Vec<String> = self.rows_where("session_issues", "session_id", session)?.iter().filter_map(|l| text(l, "id")).map(str::to_string).collect();
            self.delete_many("session_issues", &links)?;
            let reads: Vec<String> = self.rows_where("session_reads", "session_id", session)?.iter().filter_map(|r| text(r, "id")).map(str::to_string).collect();
            self.delete_many("session_reads", &reads)?;
        }
        self.delete_many("sessions", &tree)?;
        self.delete("sessions", id)?;
        Ok(json!({"txid": self.txid()}))
    }

    fn sessions_configure(&self, input: Row) -> Out {
        let id = need(&input, "sessionId")?;
        let mut patch = Row::new();
        for (key, column) in [("model", "model"), ("account", "account"), ("effort", "effort"), ("computerUse", "computer_use"), ("ultracode", "ultracode"), ("cli", "cli")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        self.patch_row("sessions", id, "Session", patch)?;
        Ok(json!({"txid": self.txid()}))
    }

    fn sessions_link(&self, input: Row, on: bool) -> Out {
        let session_id = need(&input, "sessionId")?;
        self.row_or("sessions", session_id, "Session")?;
        let links = self.rows_where("session_issues", "session_id", session_id)?;
        if on {
            let now = time::now();
            let identity = self.identity();
            let mut sort = links.len();
            let mut rows = Vec::new();
            for issue in ids_of(&input, "issueIds") {
                if links.iter().any(|l| text(l, "issue_id") == Some(issue.as_str())) {
                    continue;
                }
                rows.push(obj(json!({"id": new_id(), "session_id": session_id, "issue_id": issue, "team_id": self.team_id, "source": "user", "added_by": identity.user_id, "added_by_session_id": null, "sort_order": sort, "created_at": now, "updated_at": now})));
                sort += 1;
            }
            self.write_many("session_issues", rows)?;
        } else {
            let issue = need(&input, "issueId")?;
            let keys: Vec<String> = links.iter().filter(|l| text(l, "issue_id") == Some(issue)).filter_map(|l| text(l, "id")).map(str::to_string).collect();
            self.delete_many("session_issues", &keys)?;
        }
        Ok(json!({"txid": self.txid()}))
    }

    fn read_row(&self, session_id: &str) -> Result<Row, Refusal> {
        let identity = self.identity();
        let existing = self.rows_where("session_reads", "session_id", session_id)?.into_iter().find(|r| text(r, "user_id") == Some(identity.user_id.as_str()));
        let now = time::now();
        Ok(existing.unwrap_or_else(|| obj(json!({"id": new_id(), "user_id": identity.user_id, "session_id": session_id, "read_at": null, "archived_at": null, "story_seen_at": null, "created_at": now, "updated_at": now}))))
    }

    fn sessions_mark_read(&self, input: Row) -> Out {
        let session_id = need(&input, "sessionId")?;
        let mut row = self.read_row(session_id)?;
        let now = time::now();
        row.insert("read_at".into(), json!(now));
        row.insert("updated_at".into(), json!(now));
        self.write("session_reads", row)?;
        Ok(json!({"ok": true}))
    }

    fn sessions_set_flag(&self, input: Row, column: &str, key: &str) -> Out {
        let on = input.get(key).and_then(Value::as_bool).unwrap_or(true);
        let now = time::now();
        for session_id in ids_of(&input, "sessionIds") {
            let mut row = self.read_row(&session_id)?;
            row.insert(column.into(), if on { json!(now) } else { Value::Null });
            row.insert("updated_at".into(), json!(now));
            self.write("session_reads", row)?;
        }
        Ok(json!({"txid": self.txid()}))
    }

    // -- agents ------------------------------------------------------------------

    fn agent_out(&self, row: &Row, with_instructions: bool) -> Value {
        let mut out = json!({"id": row["id"], "teamId": row["team_id"], "name": row["name"], "icon": row.get("icon").cloned().unwrap_or(Value::Null), "color": row.get("color").cloned().unwrap_or(Value::Null), "instructions": "", "triggers": row.get("triggers").cloned().filter(Value::is_array).unwrap_or_else(|| json!([])), "deviceId": row.get("device_id").cloned().unwrap_or(Value::Null), "cli": row.get("cli").cloned().unwrap_or(Value::Null), "account": row.get("account").cloned().unwrap_or(Value::Null), "model": row.get("model").cloned().unwrap_or(Value::Null), "skillIds": row.get("skill_ids").cloned().unwrap_or(Value::Null), "ownerId": row.get("owner_id").cloned().unwrap_or(Value::Null), "sortOrder": row.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0), "role": row.get("role").cloned().unwrap_or(Value::Null)});
        if with_instructions {
            let id = text(row, "id").unwrap_or_default();
            out["instructions"] = json!(self.meta.lock().expect("meta").agent_instructions.get(id).cloned().unwrap_or_default());
        }
        out
    }

    fn agents_out(&self) -> Result<Vec<Value>, Refusal> {
        let mut rows = self.rows("agents")?;
        rows.sort_by(|a, b| a.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0).partial_cmp(&b.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| text(a, "name").cmp(&text(b, "name"))));
        Ok(rows.iter().map(|r| self.agent_out(r, false)).collect())
    }

    fn agents_get(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let row = self.row_or("agents", id, "Agent")?;
        Ok(json!({"agent": self.agent_out(&row, true)}))
    }

    fn agents_ensure_assistant(&self) -> Out {
        let identity = self.identity();
        if let Some(row) = self.rows("agents")?.into_iter().find(|a| text(a, "role") == Some("assistant") && text(a, "owner_id") == Some(identity.user_id.as_str())) {
            return Ok(json!({"agent": self.agent_out(&row, true)}));
        }
        let row = parity::assistant_row(&self.team_id, &identity, &self.device_id(), &new_id(), &time::now());
        self.write("agents", row.clone())?;
        Ok(json!({"agent": self.agent_out(&row, true)}))
    }

    fn agents_create(&self, input: Row) -> Out {
        let name = need(&input, "name")?.trim().to_string();
        let now = time::now();
        let id = new_id();
        let sort_order = self.rows("agents")?.iter().filter_map(|a| a.get("sort_order").and_then(Value::as_f64)).fold(0.0, f64::max) + 1.0;
        let row = obj(json!({"id": id, "team_id": self.team_id, "name": name, "icon": input.get("icon").cloned().unwrap_or(Value::Null), "color": input.get("color").cloned().unwrap_or(Value::Null), "triggers": input.get("triggers").cloned().filter(Value::is_array).unwrap_or_else(|| json!([])), "device_id": text(&input, "deviceId").map(str::to_string).unwrap_or_else(|| self.device_id()), "cli": input.get("cli").cloned().unwrap_or(Value::Null), "account": input.get("account").cloned().unwrap_or(Value::Null), "model": input.get("model").cloned().unwrap_or(Value::Null), "skill_ids": [], "owner_id": self.identity().user_id, "sort_order": sort_order, "role": "agent", "instructions_updated_at": null, "created_at": now, "updated_at": now}));
        if let Some(instructions) = text(&input, "instructions") {
            self.with_meta(|m| { m.agent_instructions.insert(id.clone(), instructions.to_string()); });
        }
        self.write("agents", row.clone())?;
        Ok(json!({"agent": self.agent_out(&row, true)}))
    }

    fn agents_update(&self, input: Row) -> Out {
        let id = need(&input, "id")?.to_string();
        let mut patch = Row::new();
        for (key, column) in [("name", "name"), ("icon", "icon"), ("color", "color"), ("triggers", "triggers"), ("deviceId", "device_id"), ("cli", "cli"), ("account", "account"), ("model", "model"), ("skillIds", "skill_ids"), ("sortOrder", "sort_order")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        if let Some(instructions) = text(&input, "instructions") {
            patch.insert("instructions_updated_at".into(), json!(time::now()));
            self.with_meta(|m| { m.agent_instructions.insert(id.clone(), instructions.to_string()); });
        }
        let row = self.patch_row("agents", &id, "Agent", patch)?;
        Ok(json!({"agent": self.agent_out(&row, true)}))
    }

    fn agents_delete(&self, input: Row) -> Out {
        let id = need(&input, "id")?.to_string();
        self.row_or("agents", &id, "Agent")?;
        for session in self.rows_where("sessions", "agent_id", &id)? {
            if let Some(s) = text(&session, "id") {
                self.sessions_delete(obj(json!({"sessionId": s})))?;
            }
        }
        self.with_meta(|m| { m.agent_instructions.remove(&id); });
        self.delete("agents", &id)?;
        Ok(json!({"txid": self.txid()}))
    }

    // -- prompts -----------------------------------------------------------------

    fn prompt_out(&self, row: &Row, with_body: bool) -> Value {
        let id = text(row, "id").unwrap_or_default();
        let body = if with_body { self.meta.lock().expect("meta").prompt_bodies.get(id).cloned().unwrap_or_default() } else { String::new() };
        json!({"id": row["id"], "teamId": row["team_id"], "name": row["name"], "icon": row.get("icon").cloned().unwrap_or(Value::Null), "body": body, "sortOrder": row.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0), "createdAt": row.get("created_at").cloned().unwrap_or(Value::Null), "updatedAt": row.get("updated_at").cloned().unwrap_or(Value::Null)})
    }

    fn prompts_out(&self) -> Result<Vec<Value>, Refusal> {
        let mut rows = self.rows("prompts")?;
        rows.sort_by(|a, b| a.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0).partial_cmp(&b.get("sort_order").and_then(Value::as_f64).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal).then_with(|| text(a, "name").cmp(&text(b, "name"))));
        Ok(rows.iter().map(|r| self.prompt_out(r, false)).collect())
    }

    fn prompts_get(&self, input: Row) -> Out {
        let id = need(&input, "id")?;
        let row = self.row_or("prompts", id, "Prompt")?;
        Ok(json!({"prompt": self.prompt_out(&row, true)}))
    }

    fn prompts_create(&self, input: Row) -> Out {
        let name = need(&input, "name")?.trim().to_string();
        let body = need(&input, "body")?.to_string();
        if self.rows("prompts")?.iter().any(|p| text(p, "name") == Some(name.as_str())) {
            return Err(Refusal(409, format!("A prompt named {name} exists")));
        }
        let now = time::now();
        let id = new_id();
        let sort_order = self.rows("prompts")?.iter().filter_map(|p| p.get("sort_order").and_then(Value::as_f64)).fold(0.0, f64::max) + 1.0;
        let row = obj(json!({"id": id, "team_id": self.team_id, "name": name, "icon": input.get("icon").cloned().unwrap_or(Value::Null), "sort_order": sort_order, "created_at": now, "updated_at": now}));
        self.with_meta(|m| { m.prompt_bodies.insert(id.clone(), body); });
        self.write("prompts", row.clone())?;
        Ok(json!({"prompt": self.prompt_out(&row, true)}))
    }

    fn prompts_update(&self, input: Row) -> Out {
        let id = need(&input, "id")?.to_string();
        let mut patch = Row::new();
        for (key, column) in [("name", "name"), ("icon", "icon"), ("sortOrder", "sort_order")] {
            if let Some(v) = input.get(key) {
                patch.insert(column.into(), v.clone());
            }
        }
        if let Some(body) = text(&input, "body") {
            self.with_meta(|m| { m.prompt_bodies.insert(id.clone(), body.to_string()); });
        }
        let row = self.patch_row("prompts", &id, "Prompt", patch)?;
        Ok(json!({"prompt": self.prompt_out(&row, true)}))
    }

    fn prompts_delete(&self, input: Row) -> Out {
        let id = need(&input, "id")?.to_string();
        self.row_or("prompts", &id, "Prompt")?;
        self.with_meta(|m| { m.prompt_bodies.remove(&id); });
        self.delete("prompts", &id)?;
        Ok(json!({"ok": true}))
    }

    // -- app records -------------------------------------------------------------

    fn record_write(&self, app_id: &str, write: &Row, now: &str) -> Result<(String, bool), Refusal> {
        let identity = self.identity();
        let op = text(write, "op").unwrap_or("upsert");
        if op == "delete" {
            let id = need(write, "id")?;
            self.delete("app_records", id)?;
            return Ok((id.to_string(), false));
        }
        let kind = need(write, "kind")?;
        let data = write.get("data").cloned().filter(Value::is_object).ok_or_else(|| bad("data must be an object"))?;
        let existing = text(write, "id").or_else(|| text(write, "recordId")).and_then(|id| self.row("app_records", id).ok().flatten());
        match existing {
            Some(mut row) => {
                let mut merged = row.get("data").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
                for (k, v) in data.as_object().into_iter().flatten() {
                    merged[k] = v.clone();
                }
                row.insert("data".into(), merged);
                row.insert("updated_by".into(), json!(identity.user_id));
                row.insert("updated_at".into(), json!(now));
                let id = text(&row, "id").unwrap_or_default().to_string();
                self.write("app_records", row)?;
                Ok((id, false))
            }
            None => {
                let id = text(write, "newId").or_else(|| text(write, "id")).map(str::to_string).unwrap_or_else(new_id);
                let author = text(write, "author").unwrap_or(identity.user_id.as_str()).to_string();
                let row = obj(json!({"id": id, "app_id": app_id, "team_id": self.team_id, "kind": kind, "data": data, "created_by": identity.user_id, "author_id": author, "updated_by": identity.user_id, "via_app_id": null, "share_name": null, "created_at": now, "updated_at": now}));
                self.write("app_records", row)?;
                Ok((id, true))
            }
        }
    }

    fn records_upsert(&self, input: Row) -> Out {
        let app_id = need(&input, "id")?;
        let mut write = input.clone();
        write.remove("id");
        if let Some(record) = text(&input, "recordId") {
            write.insert("id".into(), json!(record));
        }
        let (id, created) = self.record_write(app_id, &write, &time::now())?;
        Ok(json!({"txid": self.txid(), "id": id, "created": created, "eventId": null}))
    }

    fn records_delete(&self, input: Row) -> Out {
        let record = need(&input, "recordId")?;
        self.row_or("app_records", record, "Record")?;
        self.delete("app_records", record)?;
        Ok(json!({"txid": self.txid(), "eventId": null}))
    }

    fn records_batch(&self, input: Row) -> Out {
        let app_id = need(&input, "id")?;
        let now = time::now();
        let mut ids = Vec::new();
        for write in input.get("writes").and_then(Value::as_array).into_iter().flatten() {
            let (id, _) = self.record_write(app_id, &obj(write.clone()), &now)?;
            ids.push(id);
        }
        Ok(json!({"txid": self.txid(), "ids": ids, "eventIds": []}))
    }
}


// -- the issue trash (#EXP-1799) -------------------------------------------------
//
// issue-lifecycle.json `trash`, the server's semantics (apps/web
// lib/issue-trash.ts) over a store with no server-side predicates: a trashed
// issue's row and its children leave the synced tables for the sidecar's
// `issue_trash`, so every read hides them and nothing is lost; restore puts
// them back, purge (an owner's Delete forever, the sweep past the retention)
// drops them and their bytes.

/// The tables whose rows are an issue's children (the server's
/// `propagate_issue_deleted_at` set); `issue_relations` matches either side.
const TRASH_CHILDREN: [&str; 9] = [
    "comments",
    "attachments",
    "issue_labels",
    "issue_subscribers",
    "issue_events",
    "notifications",
    "session_issues",
    "pull_request_issues",
    "issue_relations",
];

/// The columns the trash stamps on the issue row.
const TRASH_COLUMNS: [&str; 3] = ["deleted_at", "deleted_by", "deleted_by_agent_id"];

/// Who trashed, restored or purged; `None` at a call site = the sweep.
struct Actor {
    user_id: String,
    agent_id: Option<String>,
    session_id: Option<String>,
}

impl Actor {
    /// The identity, acting through `session_id` when the call names one
    /// (the local agent tools do): that session's agent is the acting agent.
    fn of(executor: &LocalExecutor, session_id: Option<&str>) -> Self {
        let agent_id = session_id.and_then(|s| executor.row("sessions", s).ok().flatten()).and_then(|s| text(&s, "agent_id").filter(|a| !a.is_empty()).map(str::to_string));
        Self { user_id: executor.identity().user_id, agent_id, session_id: session_id.map(str::to_string) }
    }
}

/// The key a child row is deleted by.
fn child_key(table: &str, row: &Row) -> Option<String> {
    if table == "issue_labels" {
        Some(format!("{}/{}", text(row, "issue_id")?, text(row, "label_id")?))
    } else {
        text(row, "id").map(str::to_string)
    }
}

/// The trash `eventPayload`, in order.
fn trash_payload(issue: &Row, actor: Option<&Actor>) -> Value {
    json!({
        "issueId": issue.get("id").cloned().unwrap_or(Value::Null),
        "identifier": issue.get("identifier").cloned().unwrap_or(Value::Null),
        "title": issue.get("title").cloned().unwrap_or(Value::Null),
        "boardId": issue.get("board_id").cloned().unwrap_or(Value::Null),
        "actorUserId": actor.map(|a| a.user_id.clone()),
        "actorAgentId": actor.and_then(|a| a.agent_id.clone()),
        "sessionId": actor.and_then(|a| a.session_id.clone()),
        "viaMcp": actor.is_some_and(|a| a.session_id.is_some()),
    })
}

impl LocalExecutor {
    /// The team stream's row for a trash event (`events`, as emitEventInTx writes it).
    fn emit_trash_event(&self, name: &str, issue: &Row, actor: Option<&Actor>) -> Result<(), Refusal> {
        let (emitted_by, session_id, user_id) = match actor {
            None => ("server", None, None),
            Some(Actor { session_id: Some(s), .. }) => ("session", Some(s.clone()), None),
            Some(a) => ("user", None, Some(a.user_id.clone())),
        };
        self.write("events", obj(json!({"id": new_id(), "team_id": self.team_id, "name": name, "payload": trash_payload(issue, actor), "emitted_by": emitted_by, "session_id": session_id, "user_id": user_id, "created_at": time::now()})))
    }

    /// The child rows of `issue_id` in `table`.
    fn children_of(&self, table: &str, issue_id: &str) -> Result<Vec<Row>, Refusal> {
        if table == "issue_relations" {
            Ok(self.rows(table)?.into_iter().filter(|r| text(r, "issue_id") == Some(issue_id) || text(r, "related_issue_id") == Some(issue_id)).collect())
        } else {
            self.rows_where(table, "issue_id", issue_id)
        }
    }

    /// Moves the live issues among `ids` to the trash (an id not in the
    /// store is skipped); returns how many went.
    fn trash_issues(&self, ids: &[String], session_id: Option<&str>) -> Result<usize, Refusal> {
        let actor = Actor::of(self, session_id);
        let now = time::now();
        let mut trashed = 0;
        for id in ids {
            let Some(mut issue) = self.row("issues", id)? else { continue };
            let mut children = std::collections::BTreeMap::new();
            for table in TRASH_CHILDREN {
                let rows = self.children_of(table, id)?;
                if !rows.is_empty() {
                    children.insert(table.to_string(), rows);
                }
            }
            issue.insert("deleted_at".into(), json!(now));
            issue.insert("deleted_by".into(), json!(actor.user_id));
            issue.insert("deleted_by_agent_id".into(), json!(actor.agent_id));
            // The sidecar first: a crash between the two keeps the rows.
            let entry = crate::desktops::TrashedIssue { issue: issue.clone(), children: children.clone() };
            self.with_meta(|m| {
                m.issue_trash.retain(|t| t.id() != id);
                m.issue_trash.push(entry);
            });
            for (table, rows) in &children {
                let keys: Vec<String> = rows.iter().filter_map(|r| child_key(table, r)).collect();
                self.delete_many(table, &keys)?;
            }
            self.delete("issues", id)?;
            self.emit_trash_event(domain::issue_trash::EVENT_DELETED, &issue, Some(&actor))?;
            trashed += 1;
        }
        Ok(trashed)
    }

    fn trashed(&self, id: &str) -> Option<crate::desktops::TrashedIssue> {
        self.meta.lock().expect("meta").issue_trash.iter().find(|t| t.id() == id).cloned()
    }

    fn trash_out(entry: &crate::desktops::TrashedIssue) -> Value {
        json!({"id": entry.id(), "identifier": entry.issue.get("identifier").cloned().unwrap_or(Value::Null), "txId": null})
    }

    /// `issues.listDeleted`: the trash, newest deleted first; a removed
    /// board's issues are not listed. Runs the sweep first.
    fn issues_list_deleted(&self) -> Out {
        self.purge_due_issues(jiff::Timestamp::now().as_millisecond());
        let boards: Vec<String> = self.rows("boards")?.iter().filter_map(|b| text(b, "id")).map(str::to_string).collect();
        let mut entries: Vec<crate::desktops::TrashedIssue> = self.meta.lock().expect("meta").issue_trash.iter().filter(|t| text(&t.issue, "board_id").is_some_and(|b| boards.iter().any(|x| x == b))).cloned().collect();
        entries.sort_by(|a, b| b.deleted_at().cmp(&a.deleted_at()));
        Ok(Value::Array(
            entries
                .iter()
                .map(|t| {
                    let purge_at = t.deleted_at().and_then(domain::instant::parse_instant).map(|ms| time::format_ms(ms + domain::issue_trash::RETENTION_DAYS * 86_400_000));
                    let column = |key: &str| t.issue.get(key).cloned().unwrap_or(Value::Null);
                    json!({"id": t.id(), "identifier": column("identifier"), "title": column("title"), "boardId": column("board_id"), "deletedAt": column("deleted_at"), "deletedBy": column("deleted_by"), "deletedByAgentId": column("deleted_by_agent_id"), "purgeAt": purge_at})
                })
                .collect(),
        ))
    }

    /// `issues.restore`: every row back (minus what went since), the three
    /// columns cleared.
    fn issues_restore(&self, input: Row, session_id: Option<&str>) -> Out {
        let id = need(&input, "id")?;
        let entry = self.trashed(id).ok_or_else(|| not_found("Issue"))?;
        let mut issue = entry.issue.clone();
        for column in TRASH_COLUMNS {
            issue.remove(column);
        }
        issue.insert("updated_at".into(), json!(time::now()));
        self.write("issues", issue)?;
        for (table, rows) in &entry.children {
            let mut back = Vec::new();
            for row in rows {
                match table.as_str() {
                    "issue_labels" if self.row("labels", text(row, "label_id").unwrap_or(""))?.is_none() => continue,
                    "session_issues" if self.row("sessions", text(row, "session_id").unwrap_or(""))?.is_none() => continue,
                    "issue_relations" => {
                        let other = if text(row, "issue_id") == Some(id) { text(row, "related_issue_id") } else { text(row, "issue_id") }.unwrap_or("");
                        if self.trashed(other).is_some() {
                            // Still hidden while the other side is trashed: it rides that issue.
                            let row = row.clone();
                            self.with_meta(|m| {
                                if let Some(t) = m.issue_trash.iter_mut().find(|t| t.id() == other) {
                                    t.children.entry("issue_relations".into()).or_default().push(row);
                                }
                            });
                            continue;
                        }
                        if self.row("issues", other)?.is_none() {
                            continue;
                        }
                    }
                    _ => {}
                }
                back.push(row.clone());
            }
            if !back.is_empty() {
                self.write_many(table, back)?;
            }
        }
        self.with_meta(|m| m.issue_trash.retain(|t| t.id() != id));
        self.emit_trash_event(domain::issue_trash::EVENT_RESTORED, &entry.issue, Some(&Actor::of(self, session_id)))?;
        Ok(Self::trash_out(&entry))
    }

    /// `issues.purge`: Delete forever, from the trash only (the identity is
    /// the desktop's owner, `access.purge`).
    fn issues_purge(&self, input: Row, session_id: Option<&str>) -> Out {
        let id = need(&input, "id")?;
        let entry = self.purge_issue(id, Some(Actor::of(self, session_id)))?.ok_or_else(|| not_found("Issue"))?;
        Ok(Self::trash_out(&entry))
    }

    /// Drops one trashed issue for good: its bytes, its held rows, and the
    /// `issue.purged` row. `None` = it was not in the trash.
    fn purge_issue(&self, id: &str, actor: Option<Actor>) -> Result<Option<crate::desktops::TrashedIssue>, Refusal> {
        let Some(entry) = self.with_meta(|m| {
            let at = m.issue_trash.iter().position(|t| t.id() == id)?;
            Some(m.issue_trash.remove(at))
        }) else {
            return Ok(None);
        };
        for attachment in entry.children.get("attachments").into_iter().flatten() {
            if let Some(a) = text(attachment, "id") {
                self.blobs.remove(a);
            }
        }
        self.emit_trash_event(domain::issue_trash::EVENT_PURGED, &entry.issue, actor.as_ref())?;
        Ok(Some(entry))
    }

    /// The sweep: purges every issue trashed past the retention (`now_ms` =
    /// UTC milliseconds); returns their ids.
    pub fn purge_due_issues(&self, now_ms: i64) -> Vec<String> {
        let due: Vec<String> = self.meta.lock().expect("meta").issue_trash.iter().filter(|t| domain::issue_trash::is_purge_due(t.deleted_at(), now_ms)).map(|t| t.id().to_string()).collect();
        due.into_iter()
            .filter(|id| match self.purge_issue(id, None) {
                Ok(purged) => purged.is_some(),
                Err(Refusal(_, e)) => {
                    log::warn!("issue trash sweep {id}: {e}");
                    false
                }
            })
            .collect()
    }
}

impl Transport for LocalExecutor {
    fn call(&self, _kind: CallKind, path: &str, input: Option<&str>, session_id: Option<&str>) -> Result<String, ApiError> {
        let input: Row = match input {
            Some(json) => match serde_json::from_str::<Value>(json) {
                Ok(Value::Object(map)) => map,
                Ok(Value::Null) => Row::new(),
                Ok(_) => return Err(ApiError::Http { status: 400, message: format!("{path}: the input is not an object") }),
                Err(e) => return Err(ApiError::Decode(format!("{path} input: {e}"))),
            },
            None => Row::new(),
        };
        match self.dispatch(path, input, session_id) {
            Ok(value) => serde_json::to_string(&value).map_err(|e| ApiError::Decode(format!("{path} output: {e}"))),
            Err(Refusal(status, message)) => Err(ApiError::Http { status, message }),
        }
    }

    fn get_blob(&self, path: &str, _timeout: Duration) -> Result<Blob, ApiError> {
        let id = path.strip_prefix("/api/attachments/").map(|rest| rest.split(['?', '/']).next().unwrap_or("")).unwrap_or("");
        match self.blobs.get(id) {
            Some((bytes, content_type)) => {
                let filename = self.store.row("attachments", id).ok().flatten().and_then(|a| text(&a, "filename").map(str::to_string));
                Ok(Blob { bytes, content_type, content_disposition: filename.map(|f| format!("attachment; filename=\"{f}\"")) })
            }
            None => Err(ApiError::Http { status: 404, message: "Attachment not found".into() }),
        }
    }

    fn put_bytes(&self, path: &str, _body: Vec<u8>, _content_type: &str) -> Result<(), ApiError> {
        Err(ApiError::Http { status: 501, message: parity::refusal(path) })
    }

    fn post_json(&self, path: &str, _body: &str, _accept: &str, _timeout: Duration) -> Result<String, ApiError> {
        Err(ApiError::Http { status: 501, message: parity::refusal(path) })
    }

    fn auth_headers(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}
