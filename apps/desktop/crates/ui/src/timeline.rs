//! The issue-detail activity timeline (masterplan-v3 §4.2; web parity target:
//! `apps/web/src/components/issue-timeline.tsx` + `comment-rows/event.tsx`).
//!
//! `issue_events` and `comments` are **interleaved and sorted by
//! `created_at`** — one merged list under an "Activity (N)" header, followed
//! by the comment composer. Event rows use the web `EventRow` phrasing
//! verbatim ("changed status to …", "assigned …", "added label …", "opened a
//! pull request", …); unknown event types render nothing (web returns null —
//! forward-compat with new server event types). Comment rows carry inline
//! Edit + Delete gated to author-or-admin (§4.2), via
//! [`crate::comments::comment_row`].
//!
//! Mutations (`comments.create` / `comments.update` / `comments.delete`) are
//! §4.1 un-gated: fire on a background thread, let the Electric echo
//! re-render. The composer clears optimistically on submit and restores the
//! draft on failure (the web keeps the draft only on failure too).

use std::collections::HashMap;

use gpui::{
    div, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::rows::{Comment, IssueEvent, Label, User};

use crate::comments::{self, CommentRowProps};
use crate::icons::ExpIcon;
use crate::markdown::{store_completion_source, ImageCache};
use crate::mention_input::MentionInput;
use crate::queries;

/// One merged timeline entry (web `TimelineItem`).
enum TimelineItem {
    Comment(Comment),
    Event(IssueEvent),
}

impl TimelineItem {
    fn at(&self) -> i64 {
        let created = match self {
            TimelineItem::Comment(comment) => comment.created_at.as_deref(),
            TimelineItem::Event(event) => event.created_at.as_deref(),
        };
        created.and_then(comments::parse_epoch).unwrap_or(0)
    }
}

/// The comment being edited (web `editingCommentId` + the draft field).
struct EditState {
    comment_id: String,
    input: Entity<InputState>,
    /// The §4.6 mention-capable wrapper around `input` (rendered in the row).
    mention: Entity<MentionInput>,
    saving: bool,
}

pub struct IssueTimeline {
    issue_id: Option<String>,
    /// The composer's scoping workspace (drives autocomplete + pill
    /// resolution); re-resolved as the issue/project chain syncs.
    workspace_id: Option<String>,
    composer: Entity<InputState>,
    /// Mention-capable wrapper around the composer (§4.2: "the lightweight
    /// `@`-autocomplete textarea from §4.6, not the heavy block editor").
    composer_mention: Entity<MentionInput>,
    /// Attachment bytes for images referenced in comment bodies (auth-gated
    /// fetch through the account transport).
    images: Entity<ImageCache>,
    submitting: bool,
    editing: Option<EditState>,
    _subscriptions: Vec<Subscription>,
}

impl IssueTimeline {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Leave a reply…")
        });
        let composer_mention = cx.new(|cx| MentionInput::new(composer.clone(), cx));
        let images = cx.new(|_| ImageCache::new(None));

        let mut subscriptions = Vec::new();
        // Cmd/Ctrl+Enter submits (web `onKeyDown` metaKey/ctrlKey gate).
        subscriptions.push(cx.subscribe_in(
            &composer,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary: true, .. } => {
                    this.submit_comment(window, cx);
                }
                // Re-render so the Send button's disabled state tracks the draft.
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        // §4.1 reactivity: exactly the collections this view reads.
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe(&collections.comments, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.issue_events, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.labels, |_, _, cx| cx.notify()));
        // The autocomplete scope depends on the issue → project → workspace
        // chain; re-resolve as those shapes land.
        subscriptions.push(cx.observe(&collections.issues, |this, _, cx| {
            this.refresh_scope(cx);
        }));
        subscriptions.push(cx.observe(&collections.projects, |this, _, cx| {
            this.refresh_scope(cx);
        }));

        Self {
            issue_id: None,
            workspace_id: None,
            composer,
            composer_mention,
            images,
            submitting: false,
            editing: None,
            _subscriptions: subscriptions,
        }
    }

    /// Point the timeline at another issue. Draft + edit state reset (they are
    /// per-issue local state, like the web component's).
    pub fn set_issue(
        &mut self,
        issue_id: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.workspace_id = None;
        self.editing = None;
        self.submitting = false;
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh_scope(cx);
        cx.notify();
    }

    /// Re-point the §4.6 completion source + the image transport at the
    /// issue's workspace once resolvable (idempotent; runs on issue change
    /// and as the issues/projects shapes sync).
    fn refresh_scope(&mut self, cx: &mut gpui::Context<Self>) {
        let workspace_id = self
            .issue_id
            .as_deref()
            .and_then(|issue_id| queries::issue_workspace_id(cx, issue_id));
        if workspace_id == self.workspace_id {
            return;
        }
        self.workspace_id = workspace_id.clone();
        let source = workspace_id.map(store_completion_source);
        self.composer_mention.update(cx, |mention, _| {
            mention.set_source(source.clone());
        });
        if let Some(editing) = self.editing.as_ref() {
            let source = source.clone();
            editing.mention.update(cx, |mention, _| {
                mention.set_source(source);
            });
        }
        let transport = queries::attachment_transport(cx);
        self.images.update(cx, |images, _| {
            images.set_transport(transport);
        });
    }

    // -- mutations ------------------------------------------------------------

    pub(crate) fn submit_comment(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let draft = self.composer.read(cx).value().trim().to_string();
        if draft.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] comments.create skipped: no signed-in account");
            return;
        };

        self.submitting = true;
        // Optimistic clear; restored on failure below.
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();

        let body = draft.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::comments::comments_create(&trpc, &issue_id, &body) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.submitting = false;
                if let Err(err) = result {
                    log::warn!("[ui] comments.create failed: {err}");
                    // Give the draft back (web keeps it on failure).
                    this.composer
                        .update(cx, |input, cx| input.set_value(draft, window, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Open the inline editor for one comment (web `setEditingCommentId`).
    pub(crate) fn begin_edit(
        &mut self,
        comment_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let body = Store::global(cx)
            .collections()
            .comments
            .read(cx)
            .get(comment_id)
            .and_then(|comment| comment.body.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).auto_grow(2, 12);
            state.set_value(body, window, cx);
            state
        });
        let mention = cx.new(|cx| {
            let mut mention = MentionInput::new(input.clone(), cx);
            mention.set_source(self.workspace_id.clone().map(store_completion_source));
            mention
        });
        self.editing = Some(EditState {
            comment_id: comment_id.to_string(),
            input,
            mention,
            saving: false,
        });
        cx.notify();
    }

    pub(crate) fn cancel_edit(&mut self, cx: &mut gpui::Context<Self>) {
        self.editing = None;
        cx.notify();
    }

    pub(crate) fn save_edit(
        &mut self,
        comment_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(editing) = self.editing.as_mut() else {
            return;
        };
        if editing.comment_id != comment_id || editing.saving {
            return;
        }
        let next = editing.input.read(cx).value().trim().to_string();
        let previous = Store::global(cx)
            .collections()
            .comments
            .read(cx)
            .get(comment_id)
            .and_then(|comment| comment.body.clone())
            .unwrap_or_default();
        // Web parity: empty or unchanged → just close the editor.
        if next.is_empty() || next == previous.trim() {
            self.editing = None;
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        editing.saving = true;
        cx.notify();

        let id = comment_id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::comments::comments_update(&trpc, &id, &next) })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Ok(_) => this.editing = None,
                    Err(err) => {
                        log::warn!("[ui] comments.update failed: {err}");
                        if let Some(editing) = this.editing.as_mut() {
                            editing.saving = false;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn delete_comment(&mut self, comment_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let id = comment_id.to_string();
        cx.background_executor()
            .spawn(async move {
                if let Err(err) = api::comments::comments_delete(&trpc, &id) {
                    log::warn!("[ui] comments.delete({id}) failed: {err}");
                }
            })
            .detach();
    }

    // -- reads ----------------------------------------------------------------

    fn merged_items(&self, cx: &App) -> Vec<TimelineItem> {
        let Some(issue_id) = self.issue_id.as_deref() else {
            return Vec::new();
        };
        let collections = Store::global(cx).collections();
        let mut items: Vec<TimelineItem> = collections
            .comments
            .read(cx)
            .iter()
            .filter(|comment| comment.issue_id == issue_id)
            .cloned()
            .map(TimelineItem::Comment)
            .collect();
        items.extend(
            collections
                .issue_events
                .read(cx)
                .iter()
                .filter(|event| event.issue_id == issue_id)
                .cloned()
                .map(TimelineItem::Event),
        );
        items.sort_by_key(TimelineItem::at);
        items
    }
}

impl Render for IssueTimeline {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let items = self.merged_items(cx);
        let collections = Store::global(cx).collections();
        let user_map: HashMap<String, User> = collections
            .users
            .read(cx)
            .iter()
            .map(|user| (user.id.clone(), user.clone()))
            .collect();
        let label_map: HashMap<String, Label> = collections
            .labels
            .read(cx)
            .iter()
            .map(|label| (label.id.clone(), label.clone()))
            .collect();

        let account = queries::active_account(cx);
        let current_user_id = account.as_ref().map(|a| a.user_id.clone());
        let is_admin = account.as_ref().map(|a| a.is_admin).unwrap_or(false);
        let now_epoch = now_epoch();

        let header_label = if items.is_empty() {
            "Activity".to_string()
        } else {
            format!("Activity ({})", items.len())
        };

        let mut body = v_flex()
            .w_full()
            .border_t_1()
            .border_color(cx.theme().border)
            .px_4()
            .py_3()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .mb_2()
                    .child(SharedString::from(header_label)),
            );

        if items.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .py_1()
                    .child("No activity yet. Be the first to add a comment."),
            );
        }

        for item in &items {
            match item {
                TimelineItem::Event(event) => {
                    if let Some(row) = event_row(event, &user_map, &label_map, cx) {
                        body = body.child(row);
                    }
                }
                TimelineItem::Comment(comment) => {
                    let author = comment
                        .author_id
                        .as_deref()
                        .and_then(|id| user_map.get(id));
                    let can_modify = is_admin
                        || (current_user_id.is_some()
                            && comment.author_id == current_user_id);
                    let (editing, saving) = match self.editing.as_ref() {
                        Some(edit) if edit.comment_id == comment.id => {
                            (Some(&edit.mention), edit.saving)
                        }
                        _ => (None, false),
                    };
                    let row = comments::comment_row(
                        CommentRowProps {
                            comment,
                            author,
                            can_modify,
                            editing,
                            saving,
                            now_epoch,
                            workspace_id: self.workspace_id.as_deref(),
                            images: &self.images,
                        },
                        cx,
                    );
                    body = body.child(row);
                }
            }
        }

        let has_draft = !self.composer.read(cx).value().trim().is_empty();
        let composer =
            comments::composer_row(&self.composer_mention, self.submitting, has_draft, cx);
        body.child(composer)
    }
}

/// Unix seconds now (for relative times).
fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Event rows (comment-rows/event.tsx)
// ---------------------------------------------------------------------------

/// Web `statusLabel`: wire value with `_` → space (`in_progress` →
/// `in progress`).
fn status_label(wire: &str) -> String {
    wire.replace('_', " ")
}

/// The phrase of one event, mirroring the web `EventRow` switch. Returns
/// `None` for unknown event types (web returns null).
fn event_phrase(
    event: &IssueEvent,
    user_map: &HashMap<String, User>,
    label_map: &HashMap<String, Label>,
) -> Option<(ExpIcon, String)> {
    let payload = event.payload.as_ref();
    let payload_str = |key: &str| -> Option<String> {
        payload
            .and_then(|value| value.get(key))
            .and_then(|value| match value {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
    };

    match event.kind.as_deref()? {
        "status_changed" => {
            let to = payload_str("to").unwrap_or_default();
            Some((
                ExpIcon::CircleDot,
                format!("changed status to {}", status_label(&to)),
            ))
        }
        "assignee_changed" => match payload_str("to") {
            // `payload.to` can reference a user the viewer can't see (the
            // users shape only exposes co-members) — still an assignment,
            // not a removal (web comment, verbatim).
            Some(to_id) => {
                let name = user_map
                    .get(&to_id)
                    .map(|user| comments::author_label(Some(user)))
                    .unwrap_or_else(|| "someone".to_string());
                Some((ExpIcon::UserPlus, format!("assigned {name}")))
            }
            None => Some((ExpIcon::UserPlus, "removed the assignee".to_string())),
        },
        kind @ ("label_added" | "label_removed") => {
            let label_name = payload_str("labelId")
                .and_then(|id| label_map.get(&id).map(|label| label.name.clone()))
                .unwrap_or_else(|| "a label".to_string());
            let verb = if kind == "label_added" { "added" } else { "removed" };
            Some((ExpIcon::Tag, format!("{verb} label {label_name}")))
        }
        "pr_opened" => Some((
            ExpIcon::GitPullRequest,
            "opened a pull request".to_string(),
        )),
        "pr_merged" => Some((ExpIcon::GitMerge, "merged the pull request".to_string())),
        _ => None,
    }
}

/// One compact single-line activity entry (web `EventRow`): icon + "Actor
/// did-something" in muted text with the actor emphasized.
fn event_row(
    event: &IssueEvent,
    user_map: &HashMap<String, User>,
    label_map: &HashMap<String, Label>,
    cx: &App,
) -> Option<impl IntoElement> {
    let (icon, phrase) = event_phrase(event, user_map, label_map)?;
    let actor_name = match event.actor_user_id.as_deref() {
        Some(id) => comments::user_label(id, user_map.get(id)),
        None => "Someone".to_string(),
    };

    Some(
        h_flex()
            .py_1()
            .pl_1()
            .gap_2()
            .items_center()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                Icon::from(icon)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground)
                    .flex_shrink_0(),
            )
            .child(
                h_flex()
                    .gap_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .whitespace_nowrap()
                            .child(SharedString::from(actor_name)),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(SharedString::from(phrase)),
                    ),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> IssueEvent {
        serde_json::from_value(json!({
            "id": "e-1",
            "issue_id": "i-1",
            "type": kind,
            "payload": payload,
            "created_at": "2026-07-03T10:00:00Z"
        }))
        .unwrap()
    }

    fn users_with(name: &str) -> HashMap<String, User> {
        let user: User =
            serde_json::from_value(json!({ "id": "u-1", "name": name })).unwrap();
        HashMap::from([("u-1".to_string(), user)])
    }

    #[test]
    fn event_phrases_mirror_web_event_row() {
        let users = users_with("Ada");
        let labels: HashMap<String, Label> = HashMap::from([(
            "l-1".to_string(),
            serde_json::from_value(json!({ "id": "l-1", "workspace_id": "w", "name": "bug" }))
                .unwrap(),
        )]);

        let (_, phrase) =
            event_phrase(&event("status_changed", json!({ "to": "in_progress" })), &users, &labels)
                .unwrap();
        assert_eq!(phrase, "changed status to in progress");

        let (_, phrase) =
            event_phrase(&event("assignee_changed", json!({ "to": "u-1" })), &users, &labels)
                .unwrap();
        assert_eq!(phrase, "assigned Ada");

        // Assigning an invisible user is still an assignment (web comment).
        let (_, phrase) =
            event_phrase(&event("assignee_changed", json!({ "to": "u-ghost" })), &users, &labels)
                .unwrap();
        assert_eq!(phrase, "assigned someone");

        let (_, phrase) =
            event_phrase(&event("assignee_changed", json!({})), &users, &labels).unwrap();
        assert_eq!(phrase, "removed the assignee");

        let (_, phrase) =
            event_phrase(&event("label_added", json!({ "labelId": "l-1" })), &users, &labels)
                .unwrap();
        assert_eq!(phrase, "added label bug");

        let (_, phrase) =
            event_phrase(&event("label_removed", json!({ "labelId": "gone" })), &users, &labels)
                .unwrap();
        assert_eq!(phrase, "removed label a label");

        let (_, phrase) =
            event_phrase(&event("pr_opened", json!({})), &users, &labels).unwrap();
        assert_eq!(phrase, "opened a pull request");

        let (_, phrase) =
            event_phrase(&event("pr_merged", json!({})), &users, &labels).unwrap();
        assert_eq!(phrase, "merged the pull request");

        // Unknown event type renders nothing (web returns null).
        assert!(event_phrase(&event("something_new", json!({})), &users, &labels).is_none());
    }

    #[test]
    fn timeline_items_sort_by_created_at_across_kinds() {
        let comment: Comment = serde_json::from_value(json!({
            "id": "c-1", "issue_id": "i-1", "created_at": "2026-07-03T12:00:00Z"
        }))
        .unwrap();
        let earlier = event("pr_opened", json!({}));
        let mut items = [
            TimelineItem::Comment(comment),
            TimelineItem::Event(earlier),
        ];
        items.sort_by_key(TimelineItem::at);
        assert!(matches!(items[0], TimelineItem::Event(_)));
        assert!(matches!(items[1], TimelineItem::Comment(_)));
    }
}
