//! Share a Local desktop as a team (#EDE-57): the uploader of the team
//! import API (#EXP-1713, `fixtures/team-import.json`). The desktop's rows
//! go up in the fixture's table order, cut into chunks that respect both
//! caps (`chunk_rows`, the fixture's `chunking` cases), the attachments'
//! bytes through the signed upload each chunk answers, then one commit:
//! the account's new team, which the desktop then becomes (the caller
//! archives it, `Desktops::archive`).

use std::sync::LazyLock;

use api::{ApiError, TrpcClient};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::desktops::LocalDesktop;

pub const TEAM_IMPORT_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/team-import.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkCaps {
    pub max_rows: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestSpec {
    version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub tables: Vec<String>,
    pub chunk: ChunkCaps,
    manifest: ManifestSpec,
}

pub static SPEC: LazyLock<Spec> = LazyLock::new(|| serde_json::from_str(TEAM_IMPORT_JSON).expect("team-import.json parses"));

/// One staged row as `imports.chunk` takes it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ImportRow {
    pub table: String,
    pub id: String,
    pub row: Map<String, Value>,
}

/// The tables whose rows carry no `id` of their own (a composite key).
fn composite(table: &str) -> bool {
    table == "issue_labels"
}

/// The fixture's `validateChunk`: the refusal, or `None` when it may go up.
pub fn validate_chunk(rows: &[ImportRow]) -> Option<String> {
    let caps = &SPEC.chunk;
    if rows.is_empty() {
        return Some("A chunk needs at least one row".into());
    }
    if rows.len() > caps.max_rows {
        return Some(format!("A chunk holds at most {} rows", caps.max_rows));
    }
    if serde_json::to_vec(rows).map(|v| v.len()).unwrap_or(usize::MAX) > caps.max_bytes {
        return Some(format!("A chunk holds at most {} MB of rows", (caps.max_bytes as f64 / (1024.0 * 1024.0)).round() as usize));
    }
    for entry in rows {
        if !SPEC.tables.iter().any(|t| t == &entry.table) {
            return Some(format!("Unknown table: {}", entry.table));
        }
        if entry.id.is_empty() {
            return Some(format!("{}: a row needs an id", entry.table));
        }
        if !composite(&entry.table) && entry.row.get("id").and_then(Value::as_str) != Some(entry.id.as_str()) {
            return Some(format!("{} {}: row.id must equal id", entry.table, entry.id));
        }
    }
    None
}

/// The fixture's `chunkRows`: rows in `tables` order, each chunk within
/// both caps, a row never split (one wider than the byte cap stands alone).
pub fn chunk_rows(rows: &[ImportRow], caps: &ChunkCaps) -> Vec<Vec<ImportRow>> {
    let rank = |row: &ImportRow| SPEC.tables.iter().position(|t| t == &row.table).unwrap_or(SPEC.tables.len());
    let mut ordered: Vec<(usize, usize, &ImportRow)> = rows.iter().enumerate().map(|(i, r)| (rank(r), i, r)).collect();
    ordered.sort_by_key(|(rank, i, _)| (*rank, *i));
    let mut chunks: Vec<Vec<ImportRow>> = Vec::new();
    let mut current: Vec<ImportRow> = Vec::new();
    let mut bytes = 2usize;
    for (_, _, row) in ordered {
        let size = serde_json::to_vec(row).map(|v| v.len()).unwrap_or(0);
        let next = bytes + size + usize::from(!current.is_empty());
        if !current.is_empty() && (current.len() >= caps.max_rows || next > caps.max_bytes) {
            chunks.push(std::mem::take(&mut current));
            bytes = 2;
        }
        bytes += size + usize::from(!current.is_empty());
        current.push(row.clone());
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Every row of the desktop the import takes, in the fixture's order, the
/// sidecar's bodies and instructions folded in.
pub fn rows_of(desktop: &LocalDesktop) -> Result<Vec<ImportRow>, String> {
    let meta = desktop.executor.meta();
    let mut out = Vec::new();
    for table in &SPEC.tables {
        let Some(_) = crate::store::LocalStore::spec(table) else { continue };
        for mut row in desktop.store.rows(table)? {
            let id = if composite(table) {
                format!("{}/{}", row.get("issue_id").and_then(Value::as_str).unwrap_or(""), row.get("label_id").and_then(Value::as_str).unwrap_or(""))
            } else {
                row.get("id").and_then(Value::as_str).unwrap_or("").to_string()
            };
            if id.is_empty() {
                continue;
            }
            match table.as_str() {
                "prompts" => {
                    row.insert("body".into(), json!(meta.prompt_bodies.get(&id).cloned().unwrap_or_default()));
                }
                "agents" => {
                    row.insert("instructions".into(), json!(meta.agent_instructions.get(&id).cloned().unwrap_or_default()));
                }
                _ => {}
            }
            out.push(ImportRow { table: table.clone(), id, row });
        }
    }
    Ok(out)
}

/// `imports.create`'s manifest.
pub fn manifest(desktop: &LocalDesktop, name: &str, rows: &[ImportRow], device_id: Option<&str>) -> Value {
    let identity = desktop.executor.identity();
    let mut counts: Map<String, Value> = Map::new();
    for row in rows {
        let n = counts.get(&row.table).and_then(Value::as_u64).unwrap_or(0) + 1;
        counts.insert(row.table.clone(), json!(n));
    }
    let attachment_bytes: u64 = rows.iter().filter(|r| r.table == "attachments").filter_map(|r| r.row.get("size_bytes").and_then(Value::as_u64)).sum();
    let mut m = json!({
        "version": SPEC.manifest.version,
        "desktopId": desktop.entry.id,
        "name": name,
        "identity": {"userId": identity.user_id, "email": identity.email},
        "counts": counts,
        "attachmentBytes": attachment_bytes,
    });
    if let Some(device) = device_id {
        m["deviceId"] = json!(device);
    }
    m
}

/// What the uploader reports as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Created { team_id: String, slug: String },
    Chunk { sent: usize, total: usize },
    Attachment { sent: usize, total: usize },
    Committing,
}

/// The team the desktop became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shared {
    pub team_id: String,
    pub slug: String,
    pub name: String,
    pub remapped: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Created {
    import_id: String,
    team_id: String,
    slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Upload {
    id: String,
    upload_path: String,
}

#[derive(Deserialize)]
struct Chunked {
    #[serde(default)]
    uploads: Vec<Upload>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Committed {
    team: Value,
    #[serde(default)]
    remapped: u64,
}

/// Uploads the desktop as a new team of the account `client` talks to.
pub fn run(client: &TrpcClient, desktop: &LocalDesktop, name: &str, device_id: Option<&str>, on: &mut dyn FnMut(Progress)) -> Result<Shared, ApiError> {
    let rows = rows_of(desktop).map_err(ApiError::Decode)?;
    let manifest = manifest(desktop, name, &rows, device_id);
    let created: Created = client.mutation("imports.create", &json!({"name": name, "manifest": manifest}))?;
    on(Progress::Created { team_id: created.team_id.clone(), slug: created.slug.clone() });
    let chunks = chunk_rows(&rows, &SPEC.chunk);
    let total = chunks.len();
    let attachments_total = rows.iter().filter(|r| r.table == "attachments").count();
    let mut attachments_sent = 0;
    for (seq, chunk) in chunks.iter().enumerate() {
        if let Some(refusal) = validate_chunk(chunk) {
            return Err(ApiError::Decode(refusal));
        }
        let answer: Chunked = client.mutation("imports.chunk", &json!({"importId": created.import_id, "seq": seq, "rows": chunk}))?;
        on(Progress::Chunk { sent: seq + 1, total });
        for upload in answer.uploads {
            let Some(row) = chunk.iter().find(|r| r.table == "attachments" && r.id == upload.id) else { continue };
            let Some((bytes, content_type)) = desktop.blobs.get(&upload.id) else {
                return Err(ApiError::Decode(format!("attachment {}: no bytes on this desktop", upload.id)));
            };
            let content_type = row.row.get("content_type").and_then(Value::as_str).map(str::to_string).or(content_type).unwrap_or_else(|| "application/octet-stream".into());
            client.put_bytes(&upload.upload_path, bytes, &content_type)?;
            attachments_sent += 1;
            on(Progress::Attachment { sent: attachments_sent, total: attachments_total });
        }
    }
    on(Progress::Committing);
    let committed: Committed = client.mutation("imports.commit", &json!({"importId": created.import_id}))?;
    Ok(Shared {
        team_id: committed.team["id"].as_str().unwrap_or(&created.team_id).to_string(),
        slug: committed.team["slug"].as_str().unwrap_or(&created.slug).to_string(),
        name: committed.team["name"].as_str().unwrap_or(name).to_string(),
        remapped: committed.remapped,
    })
}
