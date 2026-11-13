//! #FEED-237 (message-delivery.json): THE OUTBOX of a session composer.
//! A Send clears the field at the press and puts an entry here (a fresh
//! clientId = the `sessions.send` `clientMessageId`); the transcript draws
//! it as the viewer's own bubble at its bottom with its delivery mark
//! ([`domain::message_delivery`]) until the synced row takes its place
//! ([`reconcile`], `outboxMerge`). The send runs on the app, never on the
//! view (closing the screen cancels nothing); a failure is the entry's red
//! mark (no toast), its menu's Retry sends again with the SAME clientId.
//!
//! Persisted per session in the data dir's `outbox.json`
//! (`{sessionId: [entry…]}`), so an app restart keeps every entry; on start
//! every `sending` entry without a messageId is SENT AGAIN with its
//! clientMessageId (F21: the server returns the row it already stored).
//!
//! FILES: a send with files is an entry too: its files persist with it
//! (their bytes copied under the data dir's `outbox-files/<clientId>/`, the
//! landed ids on the entry as each lands); the send uploads the remaining
//! ones, then `sessions.send`s the message; a file that is gone fails the
//! entry (Retry / Delete).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use domain::message_delivery::{outbox_merge, sort_entries, LocalState, OutboxEntry, OutboxFile, OutboxRow};
use domain::rows::MessageRow;
use gpui::{App, Global};

/// The file under the data dir.
pub(crate) const FILE: &str = "outbox.json";
/// FILES: the directory under the data dir holding the entries' files.
pub(crate) const FILES_DIR: &str = "outbox-files";

#[derive(Default)]
pub(crate) struct SessionOutbox {
    sessions: BTreeMap<String, Vec<OutboxEntry>>,
    /// Where it persists (`None` = memory only: tests, no data dir).
    path: Option<PathBuf>,
    loaded: bool,
    /// clientIds whose send is in flight in this process.
    in_flight: HashSet<String>,
    /// The full input each press sent (its Teams / MCP picks), for a Retry
    /// in this process; after a restart a retry sends the plain message.
    inputs: HashMap<String, api::sessions::SendInput>,
    /// FILES: the staged bytes of this process's presses (by clientId),
    /// so the upload never waits for their copy on disk.
    bytes: HashMap<String, Vec<(u64, crate::composer_images::PreviewBytes)>>,
    /// Bumped on every change (the views fold it into their rows).
    revision: u64,
}

impl Global for SessionOutbox {}

/// Load `path` (`{sessionId: [entry…]}`); a missing or unreadable file = empty.
pub(crate) fn load_from(path: &Path) -> BTreeMap<String, Vec<OutboxEntry>> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str::<BTreeMap<String, Vec<OutboxEntry>>>(&json).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, entries)| !entries.is_empty())
        .collect()
}

/// Write `sessions` to `path` (sessions without entries left out).
pub(crate) fn write_to(path: &Path, sessions: &BTreeMap<String, Vec<OutboxEntry>>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let kept: BTreeMap<&String, &Vec<OutboxEntry>> = sessions.iter().filter(|(_, entries)| !entries.is_empty()).collect();
    let json = serde_json::to_string(&kept).unwrap_or_else(|_| "{}".to_string());
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// The entries to send again on start: `sending`, no messageId.
pub(crate) fn unacked(sessions: &BTreeMap<String, Vec<OutboxEntry>>) -> Vec<(String, OutboxEntry)> {
    sessions
        .iter()
        .flat_map(|(session_id, entries)| {
            entries
                .iter()
                .filter(|entry| entry.state == LocalState::Sending && entry.message_id.is_none())
                .map(move |entry| (session_id.clone(), entry.clone()))
        })
        .collect()
}

/// The synced rows as the merge reads them.
pub(crate) fn merge_rows(rows: &[MessageRow]) -> Vec<OutboxRow> {
    rows.iter()
        .filter(|row| row.card.is_none())
        .map(|row| OutboxRow {
            id: row.id.clone(),
            author_kind: row.author_kind.clone().unwrap_or_default(),
            author_id: row.author_id.clone(),
            body: row.body_text().trim().to_string(),
            created_at: row.created_at.clone().unwrap_or_default(),
        })
        .collect()
}

impl SessionOutbox {
    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if self.path.is_none() {
            self.path = crate::window_size::app_data_dir().map(|dir| dir.join(FILE));
        }
        if let Some(path) = &self.path {
            self.sessions = load_from(path);
        }
    }

    fn persist(&mut self) {
        self.revision += 1;
        let Some(path) = self.path.clone() else { return };
        let snapshot = self.sessions.clone();
        // Small, best effort, off the UI thread.
        std::thread::spawn(move || {
            if let Err(err) = write_to(&path, &snapshot) {
                log::warn!("[outbox] write {} failed: {err}", path.display());
            }
        });
    }

    fn entry_mut(&mut self, session_id: &str, client_id: &str) -> Option<&mut OutboxEntry> {
        self.sessions.get_mut(session_id)?.iter_mut().find(|entry| entry.client_id == client_id)
    }

    /// Drop the entries `drop` names; whether any went.
    fn drop_entries(&mut self, session_id: &str, drop: &[String]) -> bool {
        let Some(entries) = self.sessions.get_mut(session_id) else { return false };
        let before = entries.len();
        // FILES: the dropped entries' copies go with them.
        let copies: Vec<PathBuf> = entries
            .iter()
            .filter(|entry| drop.contains(&entry.client_id))
            .filter_map(copies_dir)
            .collect();
        entries.retain(|entry| !drop.contains(&entry.client_id));
        for id in drop {
            self.inputs.remove(id);
            self.bytes.remove(id);
        }
        if !copies.is_empty() {
            std::thread::spawn(move || {
                for dir in copies {
                    let _ = std::fs::remove_dir_all(dir);
                }
            });
        }
        let changed = entries.len() != before;
        if entries.is_empty() {
            self.sessions.remove(session_id);
        }
        changed
    }
}

/// FILES: where an entry's file copies live (`outbox-files/<clientId>`),
/// read off its files' paths; `None` = nothing on disk.
fn copies_dir(entry: &OutboxEntry) -> Option<PathBuf> {
    let path = entry.files.iter().find(|file| !file.path.is_empty())?;
    let dir = Path::new(&path.path).parent()?;
    // Only ever a directory this outbox made.
    (dir.file_name().and_then(|name| name.to_str()) == Some(entry.client_id.as_str())
        && dir.parent().and_then(Path::file_name).and_then(|name| name.to_str()) == Some(FILES_DIR))
    .then(|| dir.to_path_buf())
}

/// FILES: an entry's file as it waits under `files_dir` (the data dir's
/// `outbox-files`): `<clientId>/<key>-<filename>`.
pub(crate) fn copy_path(files_dir: &Path, client_id: &str, key: u64, filename: &str) -> PathBuf {
    let name: String = filename.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '_' } else { c }).collect();
    files_dir.join(client_id).join(format!("{key}-{name}"))
}

/// FILES: a waiting file's bytes; a file that is gone (deleted,
/// unreadable) is the entry's failure.
pub(crate) fn read_copy(file: &OutboxFile) -> Result<Vec<u8>, String> {
    if file.path.is_empty() {
        return Err(format!("{} is gone", file.filename));
    }
    std::fs::read(&file.path).map_err(|err| format!("{} is gone: {err}", file.filename))
}

/// FILES: the files `jobs` staged, as the entry persists them (paths under
/// `files_dir`; `None` = memory only).
pub(crate) fn entry_files(
    client_id: &str,
    jobs: &[crate::composer_images::UploadJob],
    files_dir: Option<&Path>,
) -> Vec<OutboxFile> {
    jobs.iter()
        .map(|job| OutboxFile {
            key: job.key,
            filename: job.filename.clone(),
            content_type: job.content_type.clone(),
            path: files_dir
                .map(|dir| copy_path(dir, client_id, job.key, &job.filename).to_string_lossy().into_owned())
                .unwrap_or_default(),
            uploaded_id: job.uploaded_id.clone(),
            uploaded_name: job.uploaded_name.clone(),
        })
        .collect()
}

fn outbox(cx: &mut App) -> &mut SessionOutbox {
    let outbox = cx.default_global::<SessionOutbox>();
    outbox.ensure_loaded();
    outbox
}

/// The change counter (views fold it into their rows' fingerprints).
pub(crate) fn revision(cx: &App) -> u64 {
    cx.try_global::<SessionOutbox>().map_or(0, |outbox| outbox.revision)
}

/// The session's entries, oldest first (ties by clientId).
pub(crate) fn entries(session_id: &str, cx: &App) -> Vec<OutboxEntry> {
    let mut entries = cx
        .try_global::<SessionOutbox>()
        .and_then(|outbox| outbox.sessions.get(session_id).cloned())
        .unwrap_or_default();
    sort_entries(&mut entries);
    entries
}

/// The app start: load the file and send every unacked entry again once a
/// signed-in client exists (polled; gives up after a few minutes).
pub(crate) fn init(cx: &mut App) {
    let pending = unacked(&outbox(cx).sessions);
    if pending.is_empty() {
        return;
    }
    cx.spawn(async move |cx| {
        for _ in 0..90 {
            let ready = cx.update(|cx| crate::queries::trpc_client(cx).is_some() && sync::Store::try_global(cx).is_some());
            if ready {
                cx.update(|cx| {
                    for (session_id, entry) in pending {
                        dispatch(&session_id, &entry.client_id, cx);
                    }
                });
                return;
            }
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
        }
    })
    .detach();
}

/// THE press: an entry for `text` (trimmed) at the bottom of `session_id`,
/// sent with `input` (its `clientMessageId` set here) after the staged
/// `jobs` upload (FILES: they persist with the entry). Returns the clientId.
pub(crate) fn send(
    session_id: &str,
    text: &str,
    mut input: api::sessions::SendInput,
    jobs: Vec<crate::composer_images::UploadJob>,
    cx: &mut App,
) -> String {
    let client_id = uuid::Uuid::new_v4().to_string();
    input.client_message_id = Some(client_id.clone());
    let outbox = outbox(cx);
    let files_dir = outbox.path.as_deref().and_then(Path::parent).map(|dir| dir.join(FILES_DIR));
    let entry = OutboxEntry {
        client_id: client_id.clone(),
        text: text.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        state: LocalState::Sending,
        message_id: None,
        files: entry_files(&client_id, &jobs, files_dir.as_deref()),
        body: None,
    };
    if !jobs.is_empty() {
        outbox.bytes.insert(client_id.clone(), jobs.into_iter().map(|job| (job.key, job.bytes)).collect());
    }
    outbox.sessions.entry(session_id.to_string()).or_default().push(entry);
    outbox.inputs.insert(client_id.clone(), input);
    outbox.persist();
    dispatch(session_id, &client_id, cx);
    client_id
}

/// The bubble menu's Retry: the SAME clientId, back to `sending`.
pub(crate) fn retry(session_id: &str, client_id: &str, cx: &mut App) {
    let outbox = outbox(cx);
    let Some(entry) = outbox.entry_mut(session_id, client_id) else { return };
    entry.state = LocalState::Sending;
    outbox.persist();
    dispatch(session_id, client_id, cx);
}

/// The bubble menu's Delete: the entry and its text are gone.
pub(crate) fn delete(session_id: &str, client_id: &str, cx: &mut App) {
    let outbox = outbox(cx);
    if outbox.drop_entries(session_id, &[client_id.to_string()]) {
        outbox.persist();
    }
}

/// `outboxMerge`: the entries a synced row replaced leave the outbox.
/// Returns the kept entries, oldest first.
pub(crate) fn reconcile(session_id: &str, viewer_id: &str, rows: &[MessageRow], cx: &mut App) -> Vec<OutboxEntry> {
    let current = entries(session_id, cx);
    if current.is_empty() {
        return current;
    }
    // FILES: an entry whose files landed matches its row by the message
    // that went out (the prose, the embeds, the file lines).
    let matching: Vec<OutboxEntry> = current
        .iter()
        .map(|entry| match &entry.body {
            Some(body) => OutboxEntry { text: body.trim().to_string(), ..entry.clone() },
            None => entry.clone(),
        })
        .collect();
    let merged = outbox_merge(viewer_id, &matching, &merge_rows(rows));
    if !merged.drop.is_empty() {
        let outbox = outbox(cx);
        if outbox.drop_entries(session_id, &merged.drop) {
            outbox.persist();
        }
    }
    current.into_iter().filter(|entry| merged.keep.contains(&entry.client_id)).collect()
}

/// Send entry `client_id` (once at a time per entry), off the UI thread:
/// FILES first (the remaining uploads, each landed id kept as it lands),
/// then `sessions.send`.
fn dispatch(session_id: &str, client_id: &str, cx: &mut App) {
    let outbox = outbox(cx);
    let Some(entry) = outbox.entry_mut(session_id, client_id).cloned() else { return };
    if !outbox.in_flight.insert(client_id.to_string()) {
        return;
    }
    let input = outbox.inputs.get(client_id).cloned().unwrap_or_else(|| {
        let mut input = api::sessions::SendInput::message(session_id, &entry.text, Vec::new());
        input.client_message_id = Some(client_id.to_string());
        input
    });
    let memory = outbox.bytes.get(client_id).cloned().unwrap_or_default();
    let (Some(trpc), true) = (crate::queries::trpc_client(cx), sync::Store::try_global(cx).is_some()) else {
        settle(session_id, client_id, Err(()), cx);
        return;
    };
    let transport = if entry.files.is_empty() { None } else { crate::queries::attachment_transport(cx) };
    if !entry.files.is_empty() && transport.is_none() {
        settle(session_id, client_id, Err(()), cx);
        return;
    }
    // The sealing facts are the prose's (the embeds name no one).
    let plan = crate::sealing::plan_send(&input, cx);
    let (session_id, client_id) = (session_id.to_string(), client_id.to_string());
    cx.spawn(async move |cx| {
        let mut input = input;
        if let Some(transport) = transport {
            match upload_files(&session_id, &client_id, &entry, memory, transport, cx).await {
                Ok(message) => {
                    let ids: Vec<String> = message.1;
                    input.text = Some(message.0.clone()).filter(|text| !text.trim().is_empty());
                    input.attachment_ids = Some(ids).filter(|ids| !ids.is_empty());
                    cx.update(|cx| set_body(&session_id, &client_id, message.0, cx));
                }
                Err(err) => {
                    log::warn!("[outbox] upload for {client_id} failed: {err}");
                    cx.update(|cx| settle(&session_id, &client_id, Err(()), cx));
                    return;
                }
            }
        }
        let result = cx
            .background_executor()
            .spawn(async move { crate::sealing::send_blocking(&trpc, &input, &plan) })
            .await;
        let result = match result {
            Ok(output) => Ok(output.message_id),
            Err(err) => {
                log::warn!("[outbox] send of {client_id} failed: {err}");
                Err(())
            }
        };
        cx.update(|cx| settle(&session_id, &client_id, result, cx));
    })
    .detach();
}

/// FILES: write the press's in-memory bytes to their copies (a restart
/// resumes from them), upload every file not landed yet in strip order
/// (the composer's idempotent [`crate::composer_images::upload_all`], one
/// at a time so each landed id is kept on the entry as it lands) and
/// return the message to send (`uploaded_message`) with its attachment
/// ids. A file that is gone is an error.
async fn upload_files(
    session_id: &str,
    client_id: &str,
    entry: &OutboxEntry,
    memory: Vec<(u64, crate::composer_images::PreviewBytes)>,
    transport: std::sync::Arc<dyn crate::markdown::AttachmentTransport>,
    cx: &mut gpui::AsyncApp,
) -> Result<(String, Vec<String>), String> {
    use crate::composer_images::{is_inline_image, uploaded_message};
    let copies: Vec<(PathBuf, crate::composer_images::PreviewBytes)> = entry
        .remaining_files()
        .filter(|file| !file.path.is_empty())
        .filter_map(|file| {
            let bytes = memory.iter().find(|(key, _)| *key == file.key)?.1.clone();
            Some((PathBuf::from(&file.path), bytes))
        })
        .collect();
    if !copies.is_empty() {
        cx.background_executor()
            .spawn(async move {
                for (path, bytes) in copies {
                    if path.exists() {
                        continue;
                    }
                    let written = path
                        .parent()
                        .map_or(Ok(()), std::fs::create_dir_all)
                        .and_then(|()| std::fs::write(&path, bytes.as_ref()));
                    if let Err(err) = written {
                        log::warn!("[outbox] copy {} failed: {err}", path.display());
                    }
                }
            })
            .await;
    }
    let mut files = entry.files.clone();
    for index in 0..files.len() {
        if files[index].uploaded_id.is_some() {
            continue;
        }
        let file = files[index].clone();
        let bytes = memory.iter().find(|(key, _)| *key == file.key).map(|(_, bytes)| bytes.clone());
        let (transport, session) = (transport.clone(), session_id.to_string());
        let landed = cx
            .background_executor()
            .spawn(async move {
                let upload = |filename: &str, content_type: &str, bytes: &[u8]| {
                    transport.upload_session(&session, filename, content_type, bytes)
                };
                match bytes {
                    Some(bytes) => upload_one(&file, bytes, upload),
                    None => upload_one(&file, read_copy(&file)?, upload),
                }
            })
            .await?;
        let (_, id, name) = landed;
        files[index].uploaded_id = Some(id.clone());
        files[index].uploaded_name = Some(name.clone());
        let key = files[index].key;
        cx.update(|cx| note_landed(session_id, client_id, key, id, name, cx));
    }
    let file_keys: Vec<u64> =
        files.iter().filter(|file| !is_inline_image(&file.content_type)).map(|file| file.key).collect();
    let resolved: Vec<crate::composer_images::Uploaded> = files
        .iter()
        .filter_map(|file| {
            let id = file.uploaded_id.clone()?;
            Some((file.key, id, file.uploaded_name.clone().unwrap_or_else(|| file.filename.clone())))
        })
        .collect();
    let ids = resolved.iter().map(|(_, id, _)| id.clone()).collect();
    Ok((uploaded_message(&entry.text, &file_keys, &resolved), ids))
}

/// The send answered: the stored row's id, or the red mark.
fn settle(session_id: &str, client_id: &str, result: Result<Option<String>, ()>, cx: &mut App) {
    let outbox = outbox(cx);
    outbox.in_flight.remove(client_id);
    let Some(entry) = outbox.entry_mut(session_id, client_id) else { return };
    match result {
        Ok(message_id) => {
            entry.message_id = message_id;
            entry.state = LocalState::Sending;
        }
        Err(()) => entry.state = LocalState::Failed,
    }
    outbox.persist();
    cx.refresh_windows();
}

/// FILES: one file through the composer's idempotent upload.
fn upload_one<B: AsRef<[u8]>>(
    file: &OutboxFile,
    bytes: B,
    upload: impl Fn(&str, &str, &[u8]) -> anyhow::Result<api::attachment_upload::UploadedImage>,
) -> Result<crate::composer_images::Uploaded, String> {
    let job = api::attachment_upload::UploadJob {
        key: file.key,
        uploaded_id: None,
        uploaded_name: None,
        filename: file.filename.clone(),
        content_type: file.content_type.clone(),
        bytes,
    };
    crate::composer_images::upload_all(vec![job], upload)
        .map_err(|(_, err)| err)?
        .into_iter()
        .next()
        .ok_or_else(|| "no upload".to_string())
}

/// FILES: a file landed: its id on the entry, persisted at once.
fn note_landed(session_id: &str, client_id: &str, key: u64, id: String, name: String, cx: &mut App) {
    let outbox = outbox(cx);
    let Some(entry) = outbox.entry_mut(session_id, client_id) else { return };
    let Some(file) = entry.files.iter_mut().find(|file| file.key == key) else { return };
    file.uploaded_id = Some(id);
    file.uploaded_name = Some(name);
    outbox.persist();
}

/// FILES: every file landed: the message as it goes out (its synced row's
/// body, what the merge matches before the send answers).
fn set_body(session_id: &str, client_id: &str, body: String, cx: &mut App) {
    let outbox = outbox(cx);
    let Some(entry) = outbox.entry_mut(session_id, client_id) else { return };
    entry.body = Some(body);
    outbox.persist();
}

/// HANDOFF (message-delivery.json `handoffCases`): the session's synced rows
/// as `ownHandoff` reads them.
pub(crate) fn handoff_rows(rows: &[MessageRow]) -> Vec<domain::message_delivery::HandoffRow> {
    rows.iter()
        .map(|row| domain::message_delivery::HandoffRow {
            id: row.id.clone(),
            author_kind: row.author_kind.clone().unwrap_or_default(),
            author_id: row.author_id.clone(),
            body: row.body_text().to_string(),
            created_at: row.created_at.clone().unwrap_or_default(),
            consumed_at: row.consumed_at.clone().filter(|at| !at.is_empty()),
            has_card: row.card.is_some(),
        })
        .collect()
}

/// Whether any own card-less row was consumed within `handoffMaxMs` (the
/// only rows the HANDOFF can hold; else the transcript need not be read).
pub(crate) fn may_hold(viewer_id: &str, now_ms: i64, rows: &[MessageRow]) -> bool {
    let since = now_ms - domain::message_delivery::HANDOFF_MAX_MS;
    rows.iter().any(|row| {
        row.card.is_none()
            && row.is_from_user()
            && row.author_id.as_deref() == Some(viewer_id)
            && row.consumed_at.as_deref().and_then(domain::instant::parse_instant).is_some_and(|at| at >= since)
    })
}

/// HANDOFF: the consumed own rows still waiting for the transcript's echo
/// (`ownHandoff`, oldest first) and when to look again on its own (the
/// newest held row's consumedAt + `handoffMaxMs`, epoch ms).
pub(crate) fn held_rows(
    viewer_id: &str,
    now_ms: i64,
    feed_empty: bool,
    rows: &[MessageRow],
    echoes: &[domain::message_delivery::HandoffEcho],
) -> (Vec<String>, Option<i64>) {
    let rows = handoff_rows(rows);
    let held = domain::message_delivery::own_handoff(viewer_id, now_ms, feed_empty, &rows, echoes);
    let expiry = domain::message_delivery::handoff_expiry(&rows, &held);
    (held, expiry)
}

/// One of the viewer's own bubbles at the transcript's bottom: a consumed
/// row whose echo has not arrived (HANDOFF), a synced row the device has
/// not consumed yet, or an outbox entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnBubble {
    /// The row's id, else the entry's clientId (the element ids key on it).
    pub key: String,
    pub text: String,
    /// The row's createdAt, an entry's own until its row arrives.
    pub created_at: String,
    pub state: domain::message_delivery::DeliveryState,
    /// The server holds the row (a synced row, an acked entry).
    pub stored: bool,
    /// The stored row (Cancel sending's `sessions.unqueue`).
    pub message_id: Option<String>,
    /// The outbox entry (Retry / Delete), `None` = a synced row alone.
    pub client_id: Option<String>,
}

/// message-delivery.json ORDER: the consumed own rows the HANDOFF still
/// holds (`held`, [`held_rows`]: reading `read` until the transcript's echo
/// appears), then the viewer's own synced rows the device has not consumed
/// (oldest first, a card never), then the kept outbox entries (`kept`,
/// already merged). Each wears `deliveryState`; one bubble per row id.
pub(crate) fn own_tail(viewer_id: &str, rows: &[MessageRow], held: &[String], kept: &[OutboxEntry]) -> Vec<OwnBubble> {
    use domain::message_delivery::{delivery_state, DeliveryInput};
    let held_rows = held.iter().filter_map(|id| {
        rows.iter().find(|row| &row.id == id && !row.is_queued() && row.author_id.as_deref() == Some(viewer_id))
    });
    let mut pending: Vec<&MessageRow> = rows
        .iter()
        .filter(|row| row.card.is_none() && row.is_queued() && row.author_id.as_deref() == Some(viewer_id))
        .filter(|row| !row.body_text().trim().is_empty() || !row.attachment_ids.is_empty())
        .collect();
    let at = |row: &MessageRow| row.created_at.as_deref().and_then(domain::instant::parse_instant).unwrap_or(0);
    pending.sort_by(|a, b| at(a).cmp(&at(b)).then_with(|| a.id.cmp(&b.id)));
    let mut out: Vec<OwnBubble> = held_rows
        .chain(pending)
        .map(|row| OwnBubble {
            key: row.id.clone(),
            text: row.body_text().to_string(),
            created_at: row.created_at.clone().unwrap_or_default(),
            state: delivery_state(&DeliveryInput {
                local: None,
                stored: true,
                delivered_at: row.delivered_at.clone(),
                consumed_at: row.consumed_at.clone(),
                echoed: false,
            }),
            stored: true,
            message_id: Some(row.id.clone()),
            client_id: None,
        })
        .collect();
    out.extend(kept.iter().map(|entry| {
        let stored = entry.message_id.is_some();
        OwnBubble {
            key: entry.client_id.clone(),
            text: entry_text(entry),
            created_at: entry.created_at.clone(),
            state: delivery_state(&DeliveryInput { local: Some(entry.state), stored, ..DeliveryInput::default() }),
            stored,
            message_id: entry.message_id.clone(),
            client_id: Some(entry.client_id.clone()),
        }
    }));
    out
}

/// An entry's bubble text: its prose, else (files alone) their names.
fn entry_text(entry: &OutboxEntry) -> String {
    if !entry.text.trim().is_empty() || entry.files.is_empty() {
        return entry.text.clone();
    }
    entry.files.iter().map(|file| file.filename.as_str()).collect::<Vec<_>>().join("\n")
}

/// Cancel sending (`sessions.unqueue`): the row is deleted and the device
/// drops what it holds; a refusal (CONFLICT once consumed) toasts
/// `Couldn't remove the message`. The entry it came from leaves with it.
pub(crate) fn cancel(session_id: &str, bubble: &OwnBubble, cx: &mut App) {
    let Some(message_id) = bubble.message_id.clone() else { return };
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::toast::show_in_active_window(crate::toast::Toast::error(domain::message_delivery::CANCEL_FAILED_TOAST), cx);
        return;
    };
    let (session_id, client_id) = (session_id.to_string(), bubble.client_id.clone());
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::sessions::unqueue(&trpc, &message_id) }).await;
        cx.update(|cx| match result {
            Ok(_) => {
                if let Some(client_id) = client_id {
                    delete(&session_id, &client_id, cx);
                }
                cx.refresh_windows();
            }
            Err(err) => {
                log::info!("[outbox] unqueue refused: {err}");
                crate::toast::show_in_active_window(crate::toast::Toast::error(domain::message_delivery::CANCEL_FAILED_TOAST), cx);
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::message_delivery::DeliveryState;

    /// ORDER + STATE: the own unconsumed rows (oldest first; anyone
    /// else's, a consumed one and a card stay out), then the kept entries;
    /// the clock until delivered, one check once delivered, the red mark
    /// for a failed entry, an acked entry stored.
    #[test]
    fn the_own_tail_lists_pending_rows_then_entries_with_their_marks() {
        let row = |id: &str, author: &str, at: &str| MessageRow {
            id: id.into(),
            session_id: "s1".into(),
            author_kind: Some("user".into()),
            author_id: Some(author.into()),
            body: Some(format!("body {id}")),
            created_at: Some(at.into()),
            ..MessageRow::default()
        };
        let rows = vec![
            MessageRow { delivered_at: Some("2026-11-13T10:00:03.000Z".into()), ..row("m2", "u1", "2026-11-13T10:00:02.000Z") },
            row("m1", "u1", "2026-11-13T10:00:01.000Z"),
            row("x1", "u2", "2026-11-13T10:00:01.000Z"),
            MessageRow { consumed_at: Some("2026-11-13T10:00:04.000Z".into()), ..row("m0", "u1", "2026-11-13T10:00:00.000Z") },
            MessageRow { card: Some(serde_json::json!({"kind": "question"})), ..row("q1", "u1", "2026-11-13T10:00:00.000Z") },
        ];
        let kept = vec![entry("c1", LocalState::Failed, None), entry("c2", LocalState::Sending, Some("m9"))];
        let tail = own_tail("u1", &rows, &[], &kept);
        let keys: Vec<(&str, DeliveryState, bool)> = tail.iter().map(|b| (b.key.as_str(), b.state, b.stored)).collect();
        assert_eq!(
            keys,
            vec![
                ("m1", DeliveryState::Sending, true),
                ("m2", DeliveryState::Sent, true),
                ("c1", DeliveryState::Failed, false),
                ("c2", DeliveryState::Sending, true),
            ]
        );
        assert_eq!(tail[2].client_id.as_deref(), Some("c1"));
        assert_eq!(tail[3].message_id.as_deref(), Some("m9"));
    }

    /// HANDOFF: a consumed own row keeps its bubble (`read`, ahead of the
    /// unconsumed rows) until the transcript's echo appears; with the echo
    /// it goes (one bubble per message id); with no feed nothing is held
    /// (SENT INPUTS draws it); past `handoffMaxMs` it lets go, and the
    /// expiry names that moment.
    #[test]
    fn a_consumed_row_holds_its_bubble_until_the_echo_appears() {
        use domain::message_delivery::{HandoffEcho, HANDOFF_MAX_MS};
        let row = |id: &str, at: &str, consumed: Option<&str>| MessageRow {
            id: id.into(),
            session_id: "s1".into(),
            author_kind: Some("user".into()),
            author_id: Some("u1".into()),
            body: Some(format!("body {id}")),
            created_at: Some(at.into()),
            consumed_at: consumed.map(str::to_string),
            ..MessageRow::default()
        };
        let rows = vec![
            row("m1", "2026-11-13T10:00:01.000Z", None),
            row("m0", "2026-11-13T10:00:00.000Z", Some("2026-11-13T10:00:05.000Z")),
        ];
        let now = domain::instant::parse_instant("2026-11-13T10:00:10.000Z").unwrap();
        let consumed = domain::instant::parse_instant("2026-11-13T10:00:05.000Z").unwrap();
        let (held, expiry) = held_rows("u1", now, false, &rows, &[]);
        assert_eq!(held, vec!["m0"]);
        assert_eq!(expiry, Some(consumed + HANDOFF_MAX_MS));
        let tail = own_tail("u1", &rows, &held, &[]);
        let keys: Vec<(&str, DeliveryState)> = tail.iter().map(|b| (b.key.as_str(), b.state)).collect();
        assert_eq!(keys, vec![("m0", DeliveryState::Read), ("m1", DeliveryState::Sending)]);
        let echo = HandoffEcho { text: "body m0\n".into(), at: Some(consumed) };
        let (held, expiry) = held_rows("u1", now, false, &rows, std::slice::from_ref(&echo));
        assert!(held.is_empty() && expiry.is_none());
        assert_eq!(own_tail("u1", &rows, &held, &[]).len(), 1);
        assert!(held_rows("u1", now, true, &rows, &[]).0.is_empty());
        assert!(held_rows("u1", consumed + HANDOFF_MAX_MS + 1, false, &rows, &[]).0.is_empty());
    }

    fn entry(id: &str, state: LocalState, message_id: Option<&str>) -> OutboxEntry {
        OutboxEntry {
            client_id: id.into(),
            text: format!("text {id}"),
            created_at: "2026-11-13T10:00:00.000Z".into(),
            state,
            message_id: message_id.map(str::to_string),
            files: Vec::new(),
            body: None,
        }
    }

    /// The file round-trips per session; empty sessions stay out; on start
    /// only `sending` entries without a messageId go out again.
    #[test]
    fn the_outbox_persists_per_session_and_resends_only_unacked_entries() {
        let dir = std::env::temp_dir().join(format!("exp-outbox-{}", uuid::Uuid::new_v4()));
        let path = dir.join(FILE);
        let mut sessions = BTreeMap::new();
        sessions.insert(
            "s1".to_string(),
            vec![entry("c1", LocalState::Sending, None), entry("c2", LocalState::Sending, Some("m2")), entry("c3", LocalState::Failed, None)],
        );
        sessions.insert("s2".to_string(), Vec::new());
        write_to(&path, &sessions).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("\"s2\""), "{raw}");
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["s1"], sessions["s1"]);
        let again: Vec<String> = unacked(&loaded).into_iter().map(|(_, entry)| entry.client_id).collect();
        assert_eq!(again, vec!["c1"]);
        assert!(load_from(&dir.join("missing.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FILES: an entry with files persists them (paths under
    /// `outbox-files/<clientId>`, the landed ids) through `outbox.json`; on
    /// start it resends with only the unlanded files left to upload; a file
    /// that is gone reads as an error; its copies' directory is its own.
    #[test]
    fn an_entry_with_files_persists_them_and_resumes_the_rest() {
        let dir = std::env::temp_dir().join(format!("exp-outbox-{}", uuid::Uuid::new_v4()));
        let files_dir = dir.join(FILES_DIR);
        let path = dir.join(FILE);
        let image = |key: u64, name: &str, content_type: &str| crate::composer_images::UploadJob {
            key,
            uploaded_id: None,
            uploaded_name: None,
            filename: name.into(),
            content_type: content_type.into(),
            bytes: crate::composer_images::PreviewBytes(std::sync::Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                vec![1, 2, 3],
            ))),
        };
        let jobs = vec![image(0, "pasted.png", "image/png"), image(1, "notes/a.txt", "text/plain")];
        let mut with_files = entry("c1", LocalState::Sending, None);
        with_files.files = entry_files("c1", &jobs, Some(&files_dir));
        assert_eq!(with_files.files[1].path, files_dir.join("c1").join("1-notes_a.txt").to_string_lossy());
        assert_eq!(copies_dir(&with_files), Some(files_dir.join("c1")));
        // The first landed before the restart.
        with_files.files[0].uploaded_id = Some("a1".into());
        with_files.files[0].uploaded_name = Some("pasted.png".into());
        let mut sessions = BTreeMap::new();
        sessions.insert("s1".to_string(), vec![with_files.clone()]);
        write_to(&path, &sessions).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded["s1"], vec![with_files.clone()]);
        let again = unacked(&loaded);
        assert_eq!(again.len(), 1);
        let rest: Vec<&str> = again[0].1.remaining_files().map(|file| file.filename.as_str()).collect();
        assert_eq!(rest, vec!["notes/a.txt"]);
        // Its copy is gone: the entry fails; once there it reads.
        let remaining = again[0].1.remaining_files().next().unwrap().clone();
        assert!(read_copy(&remaining).is_err());
        std::fs::create_dir_all(files_dir.join("c1")).unwrap();
        std::fs::write(&remaining.path, b"hi").unwrap();
        assert_eq!(read_copy(&remaining).unwrap(), b"hi");
        // A memory-only file (no data dir) has no copy and no directory.
        let memory = OutboxEntry { files: entry_files("c2", &jobs, None), ..entry("c2", LocalState::Sending, None) };
        assert!(memory.files.iter().all(|file| file.path.is_empty()) && copies_dir(&memory).is_none());
        // Files alone: the bubble names them.
        let alone = OutboxEntry { text: String::new(), ..with_files };
        assert_eq!(entry_text(&alone), "pasted.png\nnotes/a.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The merge reads the synced rows' trimmed bodies (a card never
    /// matches) and drops the entry its row replaced.
    #[test]
    fn a_synced_row_replaces_its_entry() {
        let mut row = MessageRow {
            id: "m9".into(),
            session_id: "s1".into(),
            author_kind: Some("user".into()),
            author_id: Some("u1".into()),
            body: Some("text c1\n".into()),
            created_at: Some("2026-11-13 10:00:00.400+00".into()),
            ..MessageRow::default()
        };
        let outbox = vec![entry("c1", LocalState::Sending, None), entry("c2", LocalState::Failed, None)];
        let merged = outbox_merge("u1", &outbox, &merge_rows(std::slice::from_ref(&row)));
        assert_eq!(merged.drop, vec!["c1"]);
        assert_eq!(merged.keep, vec!["c2"]);
        row.card = Some(serde_json::json!({"kind": "plan"}));
        assert!(merge_rows(&[row]).is_empty());
    }
}
