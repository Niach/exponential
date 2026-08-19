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
//! re-render. The composer keeps its draft until the mutation SUCCEEDS
//! (EXP-554 — an optimistic clear would eat the draft when an attachment
//! upload fails halfway; the web keeps the draft on failure too).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, TextareaState},
    notification::Notification,
    v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::rows::{Comment, IssueEvent, Label, Board, User};

use crate::comment_attachments::{self, MAX_COMMENT_ATTACHMENTS};
use crate::comments::{self, CommentRowProps};
use crate::icons::{registry, ExpIcon};
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
    input: Entity<TextareaState>,
    /// The §4.6 mention-capable wrapper around `input` (rendered in the row).
    mention: Entity<MentionInput>,
    saving: bool,
    /// EXP-554: already-linked attachments the user staged for removal. They
    /// vanish from the edit strip immediately and are dropped from the id set
    /// the save sends — the server hard-deletes the rows it no longer sees.
    removed_attachment_ids: HashSet<String>,
    /// Attachments picked during THIS edit, uploaded on save.
    pending: Vec<PendingCommentAttachment>,
}

/// One file picked for a comment, from the pick until its upload lands
/// (EXP-554). Modeled on `issue_detail.rs::PendingFileUpload`, with the
/// classification and the path kept so the upload can happen on SEND (never
/// on pick — an abandoned composer must not leave orphan rows).
#[derive(Clone)]
pub(crate) struct PendingCommentAttachment {
    /// Process-local row key (element ids + the stamp-back lookup).
    pub(crate) key: u64,
    pub(crate) path: PathBuf,
    pub(crate) filename: String,
    /// What `read_any_file` WILL send (derived from the extension).
    pub(crate) content_type: String,
    /// One of the five inline-image types → the `/images` endpoint.
    pub(crate) is_image: bool,
    /// Set once the upload answered — a retry after a mid-batch failure
    /// never re-uploads it.
    pub(crate) uploaded_id: Option<String>,
    /// Last failure for this item (rendered on its chip).
    pub(crate) error: Option<SharedString>,
}

/// Which composer a pending list belongs to — the create composer at the
/// bottom of the thread, or the inline editor of the comment being edited.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingScope {
    Composer,
    Edit,
}

impl PendingScope {
    /// Element-id namespace (the two strips can be on screen at once).
    pub(crate) fn id_prefix(self) -> &'static str {
        match self {
            PendingScope::Composer => "composer",
            PendingScope::Edit => "edit",
        }
    }
}

pub struct IssueTimeline {
    issue_id: Option<String>,
    /// The composer's scoping team (drives autocomplete + pill
    /// resolution); re-resolved as the issue/board chain syncs.
    team_id: Option<String>,
    composer: Entity<TextareaState>,
    /// Mention-capable wrapper around the composer (§4.2: "the lightweight
    /// `@`-autocomplete textarea from §4.6, not the heavy block editor").
    composer_mention: Entity<MentionInput>,
    /// Attachment bytes for images referenced in comment bodies (auth-gated
    /// fetch through the account transport).
    images: Entity<ImageCache>,
    submitting: bool,
    editing: Option<EditState>,
    /// EXP-551: the emoji picker both composers share, plus which of them
    /// currently has it open (`None` = closed). One picker is enough — only
    /// one popover can be up at a time.
    emoji_picker: Entity<crate::emoji_picker::EmojiPicker>,
    emoji_open: Option<PendingScope>,
    /// EXP-554: files picked for the NEXT comment, uploaded on send.
    pending_attachments: Vec<PendingCommentAttachment>,
    next_pending_key: u64,
    _subscriptions: Vec<Subscription>,
}

impl IssueTimeline {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Leave a reply…")
        });
        let composer_mention = cx.new(|cx| MentionInput::new(composer.clone(), cx));
        let images = cx.new(|_| ImageCache::new(None));
        // EXP-551: the picker hands the glyph back here so it lands in
        // whichever composer opened it.
        let emoji_host = cx.weak_entity();
        let emoji_picker = cx.new(|cx| {
            crate::emoji_picker::EmojiPicker::new(
                std::rc::Rc::new(
                    move |unicode: &str, window: &mut Window, cx: &mut gpui::App| {
                        let unicode = unicode.to_string();
                        emoji_host
                            .update(cx, |this, cx| this.insert_emoji(&unicode, window, cx))
                            .ok();
                    },
                ),
                window,
                cx,
            )
        });

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
        // EXP-554: comment attachment strips read the `attachments`
        // collection — without this observer a just-posted (or remotely
        // added/removed) attachment would only appear on the next unrelated
        // re-render.
        subscriptions.push(cx.observe(&collections.attachments, |_, _, cx| cx.notify()));
        // The autocomplete scope depends on the issue → board → team
        // chain; re-resolve as those shapes land. Boards also notify —
        // board names in board_moved rows (EXP-57).
        subscriptions.push(cx.observe(&collections.issues, |this, _, cx| {
            this.refresh_scope(cx);
        }));
        subscriptions.push(cx.observe(&collections.boards, |this, _, cx| {
            this.refresh_scope(cx);
            cx.notify();
        }));

        Self {
            issue_id: None,
            team_id: None,
            composer,
            composer_mention,
            images,
            submitting: false,
            editing: None,
            emoji_picker,
            emoji_open: None,
            pending_attachments: Vec::new(),
            next_pending_key: 0,
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
        self.team_id = None;
        self.editing = None;
        self.submitting = false;
        // Pending picks are per-issue local state, like the draft: an upload
        // that never happened must not follow the user to another issue.
        self.pending_attachments.clear();
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh_scope(cx);
        cx.notify();
    }

    /// Re-point the §4.6 completion source + the image transport at the
    /// issue's team once resolvable (idempotent; runs on issue change
    /// and as the issues/boards shapes sync).
    fn refresh_scope(&mut self, cx: &mut gpui::Context<Self>) {
        let team_id = self
            .issue_id
            .as_deref()
            .and_then(|issue_id| queries::issue_team_id(cx, issue_id));
        if team_id == self.team_id {
            return;
        }
        self.team_id = team_id.clone();
        // EXP-551: with no team resolved yet the `:` emoji typeahead still
        // works — only `@`/`#` need synced data.
        let source = Some(match team_id {
            Some(team_id) => store_completion_source(team_id),
            None => crate::markdown::emoji_completion_source(),
        });
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

    // -- emoji (EXP-551) ------------------------------------------------------

    pub(crate) fn emoji_open(&self, scope: PendingScope) -> bool {
        self.emoji_open == Some(scope)
    }

    pub(crate) fn set_emoji_open(
        &mut self,
        scope: PendingScope,
        open: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let next = open.then_some(scope);
        if self.emoji_open != next {
            self.emoji_open = next;
            cx.notify();
        }
    }

    /// Insert a picked glyph into whichever composer opened the picker, then
    /// close it. `TextareaState::insert` is undo-atomic — one ⌘Z removes it.
    fn insert_emoji(&mut self, unicode: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let target = match self.emoji_open {
            Some(PendingScope::Edit) => self.editing.as_ref().map(|editing| editing.mention.clone()),
            _ => Some(self.composer_mention.clone()),
        };
        if let Some(mention) = target {
            mention.update(cx, |mention, cx| mention.insert_text(unicode, window, cx));
        }
        self.emoji_open = None;
        cx.notify();
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
        let pending = self.pending_attachments.clone();
        // EXP-554: an attachment-only comment is legal (the server allows an
        // empty body when the id list is non-empty).
        if draft.is_empty() && pending.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] comments.create skipped: no signed-in account");
            return;
        };
        let transport = queries::attachment_transport(cx);
        if !pending.is_empty() && transport.is_none() {
            log::warn!("[ui] comments.create skipped: no attachment transport");
            return;
        }

        self.submitting = true;
        // NO optimistic clear: the draft (and the picks) survive a failed
        // upload, and only a successful create empties the composer.
        cx.notify();

        let body = draft;
        cx.spawn_in(window, async move |this, cx| {
            let upload_issue = issue_id.clone();
            let (stamped, result) = cx
                .background_executor()
                .spawn(async move {
                    let (stamped, upload_error) = match transport {
                        Some(transport) => comment_attachments::upload_pending_attachments(
                            transport.as_ref(),
                            &upload_issue,
                            &pending,
                        ),
                        None => (Vec::new(), None),
                    };
                    if let Some(error) = upload_error {
                        return (stamped, Err(error));
                    }
                    let ids: Vec<String> =
                        stamped.iter().map(|(_, id)| id.clone()).collect();
                    let result = api::comments::comments_create(
                        &trpc,
                        &upload_issue,
                        &body,
                        (!ids.is_empty()).then_some(ids.as_slice()),
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                    (stamped, result)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.submitting = false;
                this.stamp_uploaded(PendingScope::Composer, &stamped);
                match result {
                    Ok(()) => {
                        this.composer
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        this.pending_attachments.clear();
                    }
                    Err(error) => {
                        log::warn!("[ui] comments.create failed: {error}");
                        this.note_attachment_failure(PendingScope::Composer, &error);
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "Could not post comment: {error}"
                            ))),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- EXP-554 attachment state ---------------------------------------------

    /// "Attach files" — the native multi-select picker (the same shape as the
    /// Files rail's), capped at [`MAX_COMMENT_ATTACHMENTS`] per comment.
    /// Nothing uploads here: a pick only stages a path.
    pub(crate) fn pick_comment_attachments(
        &mut self,
        scope: PendingScope,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.stage_attachments(scope, paths, cx);
            });
        })
        .detach();
    }

    fn stage_attachments(
        &mut self,
        scope: PendingScope,
        paths: Vec<PathBuf>,
        cx: &mut gpui::Context<Self>,
    ) {
        // The cap counts what the comment will END UP with: already-linked
        // rows (edit mode, minus the staged removals) plus every pick.
        let linked = match scope {
            PendingScope::Composer => 0,
            PendingScope::Edit => self
                .editing
                .as_ref()
                .map(|editing| {
                    comment_attachments::comment_attachments(&editing.comment_id, cx)
                        .into_iter()
                        .filter(|row| !editing.removed_attachment_ids.contains(&row.id))
                        .count()
                })
                .unwrap_or_default(),
        };
        let mut staged = Vec::new();
        for path in paths {
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_string();
            let content_type = crate::markdown::image_paste::content_type_for_path(&path);
            staged.push(PendingCommentAttachment {
                key: self.next_pending_key,
                path,
                filename,
                content_type: content_type.to_string(),
                is_image: crate::issue_files::is_inline_image(Some(content_type)),
                uploaded_id: None,
                error: None,
            });
            self.next_pending_key += 1;
        }
        let Some(pending) = self.pending_mut(scope) else {
            return;
        };
        let room = MAX_COMMENT_ATTACHMENTS.saturating_sub(linked + pending.len());
        let dropped = staged.len().saturating_sub(room);
        staged.truncate(room);
        pending.extend(staged);
        if dropped > 0 {
            log::warn!(
                "[ui] comment attachments capped at {MAX_COMMENT_ATTACHMENTS}: dropped {dropped}"
            );
        }
        cx.notify();
    }

    pub(crate) fn remove_pending_attachment(
        &mut self,
        scope: PendingScope,
        key: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(pending) = self.pending_mut(scope) {
            pending.retain(|item| item.key != key);
        }
        cx.notify();
    }

    /// Stage one already-linked attachment for removal (edit mode). The row
    /// is hard-deleted server-side when the save lands, never before.
    pub(crate) fn stage_attachment_removal(
        &mut self,
        attachment_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(editing) = self.editing.as_mut() {
            editing.removed_attachment_ids.insert(attachment_id);
        }
        cx.notify();
    }

    fn pending_mut(&mut self, scope: PendingScope) -> Option<&mut Vec<PendingCommentAttachment>> {
        match scope {
            PendingScope::Composer => Some(&mut self.pending_attachments),
            PendingScope::Edit => self.editing.as_mut().map(|editing| &mut editing.pending),
        }
    }

    /// Record the ids the upload leg resolved so a retry resumes instead of
    /// re-uploading (and so the abandoned rows stay findable).
    fn stamp_uploaded(&mut self, scope: PendingScope, stamped: &[(u64, String)]) {
        let Some(pending) = self.pending_mut(scope) else {
            return;
        };
        for (key, id) in stamped {
            if let Some(item) = pending.iter_mut().find(|item| item.key == *key) {
                item.uploaded_id = Some(id.clone());
                item.error = None;
            }
        }
    }

    /// Put the failure on the first item that still has no `uploaded_id` —
    /// uploads are sequential, so that IS the one that failed (a mutation
    /// failure after a clean upload run leaves the chips untouched).
    fn note_attachment_failure(&mut self, scope: PendingScope, error: &str) {
        let Some(pending) = self.pending_mut(scope) else {
            return;
        };
        if let Some(item) = pending
            .iter_mut()
            .find(|item| item.uploaded_id.is_none())
        {
            item.error = Some(SharedString::from(error.to_string()));
        }
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
            let mut state = TextareaState::new(window, cx).auto_grow(2, 12);
            state.set_value(body, window, cx);
            state
        });
        let mention = cx.new(|cx| {
            let mut mention = MentionInput::new(input.clone(), cx);
            mention.set_source(Some(match self.team_id.clone() {
                Some(team_id) => store_completion_source(team_id),
                None => crate::markdown::emoji_completion_source(),
            }));
            mention
        });
        self.editing = Some(EditState {
            comment_id: comment_id.to_string(),
            input,
            mention,
            saving: false,
            removed_attachment_ids: HashSet::new(),
            pending: Vec::new(),
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
        let Some(editing) = self.editing.as_ref() else {
            return;
        };
        if editing.comment_id != comment_id || editing.saving {
            return;
        }
        let next = editing.input.read(cx).value().trim().to_string();
        let removed = editing.removed_attachment_ids.clone();
        let pending = editing.pending.clone();
        let previous = Store::global(cx)
            .collections()
            .comments
            .read(cx)
            .get(comment_id)
            .and_then(|comment| comment.body.clone())
            .unwrap_or_default();
        // The ids that survive the edit, in the strip's order. Sending an
        // array makes it the FULL desired set (rows missing from it are
        // hard-deleted server-side), so an untouched attachment list sends
        // NOTHING at all.
        let kept: Vec<String> = comment_attachments::comment_attachments(comment_id, cx)
            .into_iter()
            .filter(|row| !removed.contains(&row.id))
            .map(|row| row.id)
            .collect();
        let attachments_changed = !removed.is_empty() || !pending.is_empty();
        // Web parity: unchanged → just close the editor. EXP-554 relaxes the
        // empty-body guard — a body may be blank as long as the comment keeps
        // (or gains) an attachment; a truly empty comment still just closes.
        let unchanged = next == previous.trim() && !attachments_changed;
        let would_be_empty = next.is_empty() && kept.is_empty() && pending.is_empty();
        if unchanged || would_be_empty {
            self.editing = None;
            cx.notify();
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let transport = queries::attachment_transport(cx);
        if !pending.is_empty() && transport.is_none() {
            log::warn!("[ui] comments.update skipped: no attachment transport");
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        if let Some(editing) = self.editing.as_mut() {
            editing.saving = true;
        }
        cx.notify();

        let id = comment_id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let (stamped, result) = cx
                .background_executor()
                .spawn(async move {
                    let (stamped, upload_error) = match transport {
                        Some(transport) => comment_attachments::upload_pending_attachments(
                            transport.as_ref(),
                            &issue_id,
                            &pending,
                        ),
                        None => (Vec::new(), None),
                    };
                    if let Some(error) = upload_error {
                        return (stamped, Err(error));
                    }
                    let mut ids = kept;
                    ids.extend(stamped.iter().map(|(_, id)| id.clone()));
                    let attachment_ids =
                        (attachments_changed).then_some(ids.as_slice());
                    let result =
                        api::comments::comments_update(&trpc, &id, &next, attachment_ids)
                            .map(|_| ())
                            .map_err(|error| error.to_string());
                    (stamped, result)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.stamp_uploaded(PendingScope::Edit, &stamped);
                match result {
                    Ok(()) => this.editing = None,
                    Err(error) => {
                        log::warn!("[ui] comments.update failed: {error}");
                        this.note_attachment_failure(PendingScope::Edit, &error);
                        if let Some(editing) = this.editing.as_mut() {
                            editing.saving = false;
                        }
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "Could not save comment: {error}"
                            ))),
                            cx,
                        );
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
                // EXP-530: `created` rows exist for automations but the
                // synthesized creation line already covers them — drop them
                // here so they don't inflate the "Activity (N)" count.
                .filter(|event| event.kind.as_deref() != Some("created"))
                .cloned()
                .map(TimelineItem::Event),
        );
        items.sort_by_key(TimelineItem::at);
        items
    }

    /// EXP-417: the synthesized first activity row — "{who} created the issue
    /// · {time}" (mobile has had it since EXP-161; this is the desktop half).
    /// It is NOT an `issue_events` row, so it never counts toward "Activity
    /// (N)". `None` until the issue itself has synced.
    fn creation_row(
        &self,
        user_map: &HashMap<String, User>,
        now_epoch: i64,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        let issue_id = self.issue_id.as_deref()?;
        let issue = Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .cloned()?;
        // Widget/agent rows carry a NULL creator — the origin is the author
        // signal.
        let who = if issue.source.as_deref() == Some(domain::contract::ISSUE_SOURCE_WIDGET) {
            "Feedback widget".to_string()
        } else if issue.source.as_deref() == Some(domain::contract::ISSUE_SOURCE_AGENT) {
            "Agent".to_string()
        } else {
            match issue.creator_id.as_deref() {
                Some(id) => comments::user_label(id, user_map.get(id)),
                None => "Someone".to_string(),
            }
        };
        let time = issue
            .created_at
            .as_deref()
            .map(|at| comments::relative_time(at, now_epoch))
            .unwrap_or_default();

        Some(
            h_flex()
                .w_full()
                .py_1()
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    // Web parity (EXP-525): the synthesized created row leads
                    // with a small plain dot, not an event glyph.
                    div()
                        .size_3p5()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .size_1p5()
                                .rounded_full()
                                .bg(cx.theme().muted_foreground),
                        ),
                )
                .child(
                    // Same definite-width chain as `event_row` (EXP-175).
                    h_flex()
                        .gap_1()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .whitespace_nowrap()
                                .flex_shrink_0()
                                .child(SharedString::from(who)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(created_phrase(&time))),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// The creation row's phrase. `relative_time` returns `""` for an unparseable
/// or missing timestamp — the separator goes with it (iOS `time.isEmpty`
/// parity), never leaving a dangling " · ".
fn created_phrase(time: &str) -> String {
    if time.is_empty() {
        "created the issue".to_string()
    } else {
        format!("created the issue · {time}")
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
        // Board names in board_moved rows (EXP-57).
        let board_map: HashMap<String, Board> = collections
            .boards
            .read(cx)
            .iter()
            .map(|board| (board.id.clone(), board.clone()))
            .collect();

        let account = queries::active_account(cx);
        let current_user_id = account.as_ref().map(|a| a.user_id.clone());
        let now_epoch = now_epoch();

        let header_label = if items.is_empty() {
            "Activity".to_string()
        } else {
            format!("Activity ({})", items.len())
        };

        // Content re-centers to the detail column; the centering must ride
        // `centered_column` — `max_w` + `mx_auto` here made taffy size the
        // column fit-content, mis-measuring wrapped comment text (EXP-179).
        // `px_4` is the shared detail-body left edge (title / description /
        // activity all align on it — EXP-282).
        let mut body = v_flex()
            .px_4()
            .py_3()
            .child(
                // EXP-417: a real section title (was `text_xs` muted) — the
                // Linear reference reads it as a heading, not a caption.
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .mb_2()
                    .child(SharedString::from(header_label)),
            );

        // EXP-417: "X created the issue" goes FIRST, above every event.
        let creation_row = self.creation_row(&user_map, now_epoch, cx);
        let has_creation_row = creation_row.is_some();
        body = body.children(creation_row);

        // With the creation row present this is unreachable; it stays for the
        // race where the issue itself hasn't synced yet.
        if items.is_empty() && !has_creation_row {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .py_1()
                    .child("No activity yet. Be the first to add a comment."),
            );
        }

        // EXP-525: status-changed rows resolve their target status against
        // the team vocabulary (real icon + color).
        let statuses = self
            .team_id
            .as_deref()
            .map(|team_id| queries::team_statuses(cx, team_id))
            .unwrap_or_default();

        for item in &items {
            match item {
                TimelineItem::Event(event) => {
                    if let Some(row) =
                        event_row(event, &user_map, &label_map, &board_map, &statuses, cx)
                    {
                        body = body.child(row);
                    }
                }
                TimelineItem::Comment(comment) => {
                    let author = comment
                        .author_id
                        .as_deref()
                        .and_then(|id| user_map.get(id));
                    // Author-only, no global-admin bypass (EXP-398) — the
                    // server refuses the mutation for anyone else.
                    let can_modify =
                        current_user_id.is_some() && comment.author_id == current_user_id;
                    let edit = self
                        .editing
                        .as_ref()
                        .filter(|edit| edit.comment_id == comment.id);
                    let row = comments::comment_row(
                        CommentRowProps {
                            comment,
                            author,
                            can_modify,
                            editing: edit.map(|edit| &edit.mention),
                            saving: edit.is_some_and(|edit| edit.saving),
                            // EXP-554: the edit strip hides what the user
                            // staged for removal and grows a ✕ per tile.
                            edit_removed: edit.map(|edit| &edit.removed_attachment_ids),
                            edit_pending: edit
                                .map(|edit| edit.pending.as_slice())
                                .unwrap_or_default(),
                            now_epoch,
                            team_id: self.team_id.as_deref(),
                            images: &self.images,
                            emoji_picker: &self.emoji_picker,
                            emoji_open: self.emoji_open(PendingScope::Edit),
                        },
                        cx,
                    );
                    body = body.child(row);
                }
            }
        }

        // EXP-554: attachments alone are enough to send.
        let has_draft = !self.composer.read(cx).value().trim().is_empty()
            || !self.pending_attachments.is_empty();
        let composer = comments::composer_row(
            &self.composer_mention,
            self.submitting,
            has_draft,
            &self.pending_attachments,
            &self.emoji_picker,
            self.emoji_open(PendingScope::Composer),
            cx,
        );
        // The hairline separating the activity section from the description
        // above stops at the READING column (EXP-422 — a deliberate reversal
        // of EXP-327's full-bleed rule; mobile keeps the full-bleed line).
        v_flex()
            .w_full()
            .child(crate::issue_detail::centered_column(
                v_flex()
                    .w_full()
                    // flex_shrink_0: a 1px main-axis child in a column is
                    // otherwise free to be squeezed to nothing (the same guard
                    // the rail divider uses).
                    .child(div().w_full().h_px().flex_shrink_0().bg(cx.theme().border))
                    .child(body.child(composer)),
            ))
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

/// Web `priorityLabel`: the wire value capitalized (`urgent` → `Urgent`);
/// an absent/empty side reads `None` (EXP-530 — a priority can be cleared).
fn priority_label(value: Option<String>) -> String {
    let raw = value.unwrap_or_default();
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "None".to_string(),
    }
}

/// The trailing digits of a PR URL (`…/pull/42` → `42`) — `pr_merged`
/// payloads carry only `prUrl` (see `pr-sync.ts`), so the number is derived.
fn pr_number_from_url(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit('/').next()?;
    (!last.is_empty() && last.bytes().all(|b| b.is_ascii_digit())).then(|| last.to_string())
}

/// An event row's leading glyph: a plain registry icon, or — for status
/// changes (EXP-525) — the target status's REAL resolved icon in its color.
enum EventGlyph {
    Plain(ExpIcon),
    Status(domain::statuses::ResolvedStatus),
}

impl EventGlyph {
    fn render(&self, cx: &App) -> Icon {
        match self {
            EventGlyph::Plain(icon) => Icon::from(icon.clone())
                .xsmall()
                .text_color(cx.theme().muted_foreground),
            EventGlyph::Status(status) => crate::icons::resolved_status_icon(status, cx).xsmall(),
        }
    }
}

/// The phrase of one event, mirroring the web `EventRow` switch (plus the
/// richer payload details — EXP-33: from→to status, PR number + link).
/// The third element is a click-through URL (PR events). Returns `None` for
/// unknown event types (web returns null).
fn event_phrase(
    event: &IssueEvent,
    user_map: &HashMap<String, User>,
    label_map: &HashMap<String, Label>,
    board_map: &HashMap<String, Board>,
    statuses: &[domain::rows::IssueStatusRow],
) -> Option<(EventGlyph, String, Option<String>)> {
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
            // EXP-314: the payload now carries the human NAMES of the status
            // rows (`fromName`/`toName`) alongside the legacy enum anchors.
            // Prefer them — a custom status has no enum value to munge — and
            // fall back to the anchor munge for rows written before EXP-314.
            let to = payload_str("toName")
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| status_label(&payload_str("to").unwrap_or_default()));
            let from = payload_str("fromName")
                .filter(|name| !name.is_empty())
                .or_else(|| payload_str("from").map(|from| status_label(&from)));
            let phrase = match from {
                Some(from) if !from.is_empty() && !to.is_empty() => {
                    format!("changed status from {from} to {to}")
                }
                _ => format!("changed status to {to}"),
            };
            // EXP-525: lead with the TARGET status's real colored icon —
            // resolved from the payload's `toStatusId` (or the legacy anchor)
            // against the team vocabulary, like every status surface.
            let anchor = domain::IssueStatus::from_wire(&payload_str("to").unwrap_or_default());
            let status = crate::queries::resolve_status_ref(
                payload_str("toStatusId").as_deref(),
                anchor,
                statuses,
            );
            Some((EventGlyph::Status(status), phrase, None))
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
                Some((EventGlyph::Plain(registry::EVENT_ASSIGNEE_CHANGED), format!("assigned {name}"), None))
            }
            None => Some((
                EventGlyph::Plain(registry::EVENT_ASSIGNEE_CHANGED),
                "removed the assignee".to_string(),
                None,
            )),
        },
        kind @ ("label_added" | "label_removed") => {
            let label_name = payload_str("labelId")
                .and_then(|id| label_map.get(&id).map(|label| label.name.clone()))
                .unwrap_or_else(|| "a label".to_string());
            let verb = if kind == "label_added" { "added" } else { "removed" };
            Some((EventGlyph::Plain(registry::EVENT_LABEL_ADDED), format!("{verb} label {label_name}"), None))
        }
        "board_moved" => {
            // EXP-57 (web `EventRow` parity): from/to board names resolve
            // from the synced boards; a deleted source degrades generically
            // and the payload's fromIdentifier keeps the row useful.
            let board_name = |key: &str| {
                payload_str(key).and_then(|id| board_map.get(&id).map(|p| p.name.clone()))
            };
            let from =
                board_name("fromBoardId").unwrap_or_else(|| "another board".to_string());
            let to = board_name("toBoardId").unwrap_or_else(|| "this board".to_string());
            let phrase = match payload_str("fromIdentifier") {
                Some(identifier) => format!("moved this from {from} ({identifier}) to {to}"),
                None => format!("moved this from {from} to {to}"),
            };
            Some((EventGlyph::Plain(registry::EVENT_BOARD_MOVED), phrase, None))
        }
        "pr_opened" => {
            let url = payload_str("prUrl");
            let phrase = match payload_str("prNumber") {
                Some(number) => format!("opened pull request #{number}"),
                None => "opened a pull request".to_string(),
            };
            Some((EventGlyph::Plain(registry::PR_OPEN), phrase, url))
        }
        "pr_merged" => {
            let url = payload_str("prUrl");
            let number = payload_str("prNumber")
                .or_else(|| url.as_deref().and_then(pr_number_from_url));
            let phrase = match number {
                Some(number) => format!("merged pull request #{number}"),
                None => "merged the pull request".to_string(),
            };
            Some((EventGlyph::Plain(registry::PR_MERGED), phrase, url))
        }
        // EXP-530: the from→to pair the server writes in
        // `finalizeIssueUpdateInTx` — both sides capitalized, a cleared
        // priority reads "None" (web `priorityLabel` parity).
        "priority_changed" => {
            let from = priority_label(payload_str("from"));
            let to = priority_label(payload_str("to"));
            Some((
                EventGlyph::Plain(registry::EVENT_PRIORITY_CHANGED),
                format!("changed priority from {from} to {to}"),
                None,
            ))
        }
        // EXP-530: `created` rows exist as the automation-trigger substrate —
        // the issue header already shows creation, so every client suppresses
        // them here (web `EventRow` returns null on the same arm).
        "created" => None,
        _ => None,
    }
}

/// One compact single-line activity entry (web `EventRow`): icon + "Actor
/// did-something" in muted text with the actor emphasized. PR events carry a
/// link — the row opens it in the browser (EXP-33).
fn event_row(
    event: &IssueEvent,
    user_map: &HashMap<String, User>,
    label_map: &HashMap<String, Label>,
    board_map: &HashMap<String, Board>,
    statuses: &[domain::rows::IssueStatusRow],
    cx: &App,
) -> Option<gpui::AnyElement> {
    let (glyph, phrase, link) = event_phrase(event, user_map, label_map, board_map, statuses)?;
    let actor_name = match event.actor_user_id.as_deref() {
        Some(id) => comments::user_label(id, user_map.get(id)),
        None => "Someone".to_string(),
    };

    // The truncating phrase needs a DEFINITE width all the way down the
    // chain: gpui's ellipsis measures against available space, and any
    // auto-width link collapses it, truncating the text long before the row
    // edge. EXP-144's flex_1 wrapper wasn't enough — the phrase div inside
    // it was still auto-width and the row itself relied on implicit stretch
    // (EXP-175). So: w_full row → flex_1 wrapper → flex_1 phrase, every
    // level resolvable without measuring the text.
    // EXP-282: no extra `pl_1` — event rows start on the same left edge as
    // the section header, the comment avatars and the description above.
    let mut row = h_flex()
        .id(SharedString::from(format!("issue-event-{}", event.id)))
        .w_full()
        .py_1()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(glyph.render(cx).flex_shrink_0())
        .child(
            h_flex()
                .gap_1()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().foreground)
                        .whitespace_nowrap()
                        .flex_shrink_0()
                        .child(SharedString::from(actor_name)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .when(link.is_some(), |el| el.text_color(cx.theme().link))
                        .child(SharedString::from(phrase)),
                ),
        );

    if let Some(link) = link {
        row = row.cursor_pointer().on_click(move |_, _, _| {
            if let Err(error) = api::opener::open_in_browser(&link) {
                log::warn!("[ui] timeline: open PR link failed: {error}");
            }
        });
    }
    Some(row.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Desktop 0.14.16 aborted on EVERY issue open: the composer's emoji
    /// button read the timeline back through `cx.entity().read(cx)` while
    /// `render` held the entity's lease (gpui `double_lease_panic`). No synced
    /// issue is needed to reproduce — the composer row renders regardless —
    /// so this draws a bare timeline under an empty Store and must not panic.
    #[gpui::test]
    async fn a_bare_timeline_renders_without_a_double_lease(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            let store = Store::open(cx, None, None);
            cx.set_global(store);
        });
        let (view, cx) = cx.add_window_view(|window, cx| IssueTimeline::new(window, cx));
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        // Open the composer's picker and draw again: the Edit/Composer open
        // flags ride the same `&self` path.
        cx.update(|_window, cx| {
            view.update(cx, |view, cx| {
                view.set_emoji_open(PendingScope::Composer, true, cx);
            });
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        view.read_with(cx, |view, _| {
            assert!(view.emoji_open(PendingScope::Composer));
        });
    }

    /// The separator must vanish with the time — `relative_time` returns `""`
    /// for a missing/unparseable `created_at`, and a dangling " · " is what
    /// the iOS `time.isEmpty` branch exists to prevent.
    #[test]
    fn created_phrase_drops_the_separator_without_a_time() {
        assert_eq!(created_phrase(""), "created the issue");
        assert_eq!(
            created_phrase("2 hours ago"),
            "created the issue · 2 hours ago"
        );
    }

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
            serde_json::from_value(json!({ "id": "l-1", "team_id": "w", "name": "bug" }))
                .unwrap(),
        )]);
        let boards: HashMap<String, Board> = HashMap::new();

        let (_, phrase, _) = event_phrase(
            &event("status_changed", json!({ "to": "in_progress" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed status to in progress");

        // The server payload carries from+to (issues.ts) — show both (EXP-33).
        let (_, phrase, _) = event_phrase(
            &event("status_changed", json!({ "from": "todo", "to": "in_progress" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed status from todo to in progress");

        // EXP-314: a status-row payload renders the ROW NAMES (a custom status
        // has no enum value to munge), and a missing `fromName` still falls
        // back to the anchor munge.
        let (_, phrase, _) = event_phrase(
            &event(
                "status_changed",
                json!({
                    "from": "in_progress",
                    "to": "in_progress",
                    "fromStatusId": "s-1",
                    "toStatusId": "s-2",
                    "fromName": "Building",
                    "toName": "In QA"
                }),
            ),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed status from Building to In QA");

        let (_, phrase, _) = event_phrase(
            &event("status_changed", json!({ "to": "in_progress", "toName": "In QA" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed status to In QA");

        let (_, phrase, _) = event_phrase(
            &event("assignee_changed", json!({ "to": "u-1" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "assigned Ada");

        // Assigning an invisible user is still an assignment (web comment).
        let (_, phrase, _) = event_phrase(
            &event("assignee_changed", json!({ "to": "u-ghost" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "assigned someone");

        let (_, phrase, _) = event_phrase(
            &event("assignee_changed", json!({})),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "removed the assignee");

        let (_, phrase, _) = event_phrase(
            &event("label_added", json!({ "labelId": "l-1" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "added label bug");

        let (_, phrase, _) = event_phrase(
            &event("label_removed", json!({ "labelId": "gone" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "removed label a label");

        let (_, phrase, link) = event_phrase(
            &event("pr_opened", json!({})),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "opened a pull request");
        assert_eq!(link, None);

        let (_, phrase, link) = event_phrase(
            &event("pr_merged", json!({})),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "merged the pull request");
        assert_eq!(link, None);

        // EXP-530: both priority sides capitalize; a cleared side reads
        // "None" (web `priorityLabel`).
        let (_, phrase, _) = event_phrase(
            &event("priority_changed", json!({ "from": "low", "to": "urgent" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed priority from Low to Urgent");

        let (_, phrase, _) = event_phrase(
            &event("priority_changed", json!({ "to": "high" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed priority from None to High");

        // EXP-530: `created` is suppressed on every client — the issue header
        // already says who filed it.
        assert!(event_phrase(
            &event("created", json!({ "status": "todo", "source": "user" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .is_none());

        // Unknown event type renders nothing (web returns null).
        assert!(event_phrase(
            &event("something_new", json!({})),
            &users,
            &labels,
            &boards,
            &[],
        )
        .is_none());
    }

    #[test]
    fn pr_events_surface_number_and_link() {
        let users = HashMap::new();
        let labels = HashMap::new();
        let boards = HashMap::new();

        // pr_opened payload: { prUrl, prNumber, branch } (pr-sync.ts).
        let (_, phrase, link) = event_phrase(
            &event(
                "pr_opened",
                json!({
                    "prUrl": "https://github.com/o/r/pull/42",
                    "prNumber": 42,
                    "branch": "exp/EXP-33"
                }),
            ),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "opened pull request #42");
        assert_eq!(link.as_deref(), Some("https://github.com/o/r/pull/42"));

        // pr_merged payload carries only prUrl — the number is derived.
        let (_, phrase, link) = event_phrase(
            &event("pr_merged", json!({ "prUrl": "https://github.com/o/r/pull/42" })),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "merged pull request #42");
        assert_eq!(link.as_deref(), Some("https://github.com/o/r/pull/42"));

        assert_eq!(pr_number_from_url("https://x/pull/7/"), Some("7".into()));
        assert_eq!(pr_number_from_url("https://x/pull/abc"), None);
        assert_eq!(pr_number_from_url(""), None);
    }

    #[test]
    fn board_moved_resolves_names_and_degrades_gracefully() {
        let users = HashMap::new();
        let labels = HashMap::new();
        let alpha: Board = serde_json::from_value(json!({
            "id": "p-1", "team_id": "w-1", "name": "Alpha"
        }))
        .unwrap();
        let beta: Board = serde_json::from_value(json!({
            "id": "p-2", "team_id": "w-1", "name": "Beta"
        }))
        .unwrap();
        let boards = HashMap::from([("p-1".to_string(), alpha), ("p-2".to_string(), beta)]);

        // EXP-57 payload (issues.ts move): both boards synced → web parity.
        let (_, phrase, _) = event_phrase(
            &event(
                "board_moved",
                json!({
                    "fromBoardId": "p-1",
                    "toBoardId": "p-2",
                    "fromIdentifier": "EXP-42",
                    "toIdentifier": "ABC-17"
                }),
            ),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "moved this from Alpha (EXP-42) to Beta");

        // A deleted/unsynced source degrades generically; the retired
        // identifier keeps the row useful (web fallback).
        let (_, phrase, _) = event_phrase(
            &event(
                "board_moved",
                json!({
                    "fromBoardId": "gone",
                    "toBoardId": "p-2",
                    "fromIdentifier": "EXP-42"
                }),
            ),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "moved this from another board (EXP-42) to Beta");

        // Empty payload never panics.
        let (_, phrase, _) = event_phrase(
            &event("board_moved", json!({})),
            &users,
            &labels,
            &boards,
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "moved this from another board to this board");
    }

    /// EXP-33 regression: the store binds jsonb payloads as JSON TEXT
    /// (`bind_value`) and hydration rewraps TEXT as `Value::String`
    /// (`row_to_map`) — without the `tolerant_opt_json` re-parse the payload
    /// reaches the timeline as a STRING and "changed status to ‹blank›"
    /// renders. This pushes an event through the REAL SQLite store round-trip;
    /// constructing payloads directly (like the tests above) misses it.
    #[test]
    fn payload_survives_the_sqlite_store_round_trip() {
        use sync::protocol::{RowKey, ShapeMessage};

        let dir = std::env::temp_dir().join(format!(
            "exp-ui-timeline-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let store = sync::store::ShapeStore::open(&dir.join("sync.sqlite")).unwrap();
        let spec = sync::shapes::shape_by_name("issue_events").unwrap();

        let value = json!({
            "id": "e-rt",
            "issue_id": "i-1",
            "type": "status_changed",
            "payload": { "from": "todo", "to": "in_progress" },
            "created_at": "2026-07-03T10:00:00Z"
        });
        store
            .apply_batch(
                spec,
                &[ShapeMessage::Insert {
                    key: RowKey::Single("e-rt".into()),
                    value: value.as_object().cloned().unwrap(),
                }],
                None,
            )
            .unwrap();

        let map = store
            .read_by_key(spec, &RowKey::Single("e-rt".into()))
            .unwrap()
            .expect("row round-trips");
        let hydrated: IssueEvent =
            serde_json::from_value(serde_json::Value::Object(map)).unwrap();

        let payload = hydrated.payload.as_ref().expect("payload present");
        assert!(
            payload.is_object(),
            "payload must hydrate as a JSON object, got {payload:?}"
        );
        let (_, phrase, _) = event_phrase(
            &hydrated,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &[],
        )
        .unwrap();
        assert_eq!(phrase, "changed status from todo to in progress");

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
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
