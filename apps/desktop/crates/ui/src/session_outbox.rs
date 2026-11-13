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

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use domain::message_delivery::{outbox_merge, sort_entries, LocalState, OutboxEntry, OutboxRow};
use domain::rows::MessageRow;
use gpui::{App, Global};

/// The file under the data dir.
pub(crate) const FILE: &str = "outbox.json";

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
        entries.retain(|entry| !drop.contains(&entry.client_id));
        for id in drop {
            self.inputs.remove(id);
        }
        let changed = entries.len() != before;
        if entries.is_empty() {
            self.sessions.remove(session_id);
        }
        changed
    }
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
/// sent with `input` (its `clientMessageId` set here). Returns the clientId.
pub(crate) fn send(session_id: &str, text: &str, mut input: api::sessions::SendInput, cx: &mut App) -> String {
    let client_id = uuid::Uuid::new_v4().to_string();
    input.client_message_id = Some(client_id.clone());
    let entry = OutboxEntry {
        client_id: client_id.clone(),
        text: text.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        state: LocalState::Sending,
        message_id: None,
    };
    let outbox = outbox(cx);
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
    let merged = outbox_merge(viewer_id, &current, &merge_rows(rows));
    if !merged.drop.is_empty() {
        let outbox = outbox(cx);
        if outbox.drop_entries(session_id, &merged.drop) {
            outbox.persist();
        }
    }
    current.into_iter().filter(|entry| merged.keep.contains(&entry.client_id)).collect()
}

/// Send entry `client_id` (once at a time per entry), off the UI thread.
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
    let (Some(trpc), true) = (crate::queries::trpc_client(cx), sync::Store::try_global(cx).is_some()) else {
        settle(session_id, client_id, Err(()), cx);
        return;
    };
    let plan = crate::sealing::plan_send(&input, cx);
    let (session_id, client_id) = (session_id.to_string(), client_id.to_string());
    cx.spawn(async move |cx| {
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

/// One of the viewer's own bubbles at the transcript's bottom: a synced row
/// the device has not consumed yet, or an outbox entry.
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

/// message-delivery.json ORDER: the viewer's own synced rows the device has
/// not consumed (oldest first, a card never), then the kept outbox entries
/// (`kept`, already merged). Each wears `deliveryState`.
pub(crate) fn own_tail(viewer_id: &str, rows: &[MessageRow], kept: &[OutboxEntry]) -> Vec<OwnBubble> {
    use domain::message_delivery::{delivery_state, DeliveryInput};
    let mut pending: Vec<&MessageRow> = rows
        .iter()
        .filter(|row| row.card.is_none() && row.is_queued() && row.author_id.as_deref() == Some(viewer_id))
        .filter(|row| !row.body_text().trim().is_empty() || !row.attachment_ids.is_empty())
        .collect();
    let at = |row: &MessageRow| row.created_at.as_deref().and_then(domain::instant::parse_instant).unwrap_or(0);
    pending.sort_by(|a, b| at(a).cmp(&at(b)).then_with(|| a.id.cmp(&b.id)));
    let mut out: Vec<OwnBubble> = pending
        .into_iter()
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
            text: entry.text.clone(),
            created_at: entry.created_at.clone(),
            state: delivery_state(&DeliveryInput { local: Some(entry.state), stored, ..DeliveryInput::default() }),
            stored,
            message_id: entry.message_id.clone(),
            client_id: Some(entry.client_id.clone()),
        }
    }));
    out
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
        let tail = own_tail("u1", &rows, &kept);
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

    fn entry(id: &str, state: LocalState, message_id: Option<&str>) -> OutboxEntry {
        OutboxEntry {
            client_id: id.into(),
            text: format!("text {id}"),
            created_at: "2026-11-13T10:00:00.000Z".into(),
            state,
            message_id: message_id.map(str::to_string),
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
