//! EXP-1170 — the ISSUE DRAFT behind the New issue page
//! ([`crate::issue_draft_screen`]): its identity, its eager uploads, its
//! files and its AUTOSAVE.
//!
//! Lifted out of the retired create dialog (EXP-878, `issue_composer`), which
//! wrote its draft exactly once, on close. A page has no close, so the row is
//! kept current instead — ONE coalesced `issueDrafts.upsert` of the FULL row,
//! single-flight ([`WriteQueue`]: one request at a time, and while it runs
//! ONE queued snapshot that every later one REPLACES):
//!
//! * [`DraftEditor::schedule_save`] — [`AUTOSAVE_DEBOUNCE_MS`] after the last
//!   title/description edit;
//! * [`DraftEditor::save_now`] — at once (a chip or board pick, a blur);
//! * [`DraftEditor::leave`] — leaving the page (any navigation, a team switch,
//!   the view dropping): content saves, an emptied draft that EXISTS is
//!   deleted, an untouched blank page writes nothing ([`leave_action`]);
//! * [`DraftEditor::flush_for_create`] — before Create: the timer is dropped,
//!   in-flight writes and uploads settle, and the answer is whether the row
//!   exists (the create's `draftId`).
//!
//! Content is [`crate::drafts::has_content`] — a title, a description or an
//! attachment; chips alone never create a row. Uploads are EAGER and run one
//! at a time: the first one ensures the row (one upsert through the same
//! queue), then every file lands on the DRAFT, so Create carries only
//! `draftId` and the server reparents them in the create's own transaction.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use gpui::{AnyWindowHandle, EventEmitter, SharedString, Task};

use crate::drafts::DraftSave;
use crate::markdown::image_paste::{StagedImage, UploadedImage};

/// The autosave debounce — the contract's (`fixtures/issue-draft.json`).
pub(crate) const AUTOSAVE_DEBOUNCE_MS: u64 = domain::issue_draft::AUTOSAVE_DEBOUNCE_MS;

/// How often [`DraftEditor::flush_for_create`] re-checks for settled writes.
const SETTLE_POLL: Duration = Duration::from_millis(40);

/// What the page tells the draft to do as it is LEFT. Pure, so the whole
/// table is a unit test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeaveAction {
    Save,
    Delete,
    Nothing,
}

/// The ONE leave rule: a draft that owes nothing (filed, discarded) writes
/// nothing; content saves; an emptied draft that exists (opened from a row,
/// or ensured by an earlier write) is deleted; a blank page that never wrote
/// anything stays free.
pub(crate) fn leave_action(
    has_content: bool,
    from_existing: bool,
    ensured: bool,
    owed: bool,
) -> LeaveAction {
    if !owed {
        LeaveAction::Nothing
    } else if has_content {
        LeaveAction::Save
    } else if from_existing || ensured {
        LeaveAction::Delete
    } else {
        LeaveAction::Nothing
    }
}

/// Single-flight write coalescing: at most ONE request in flight, and at
/// most ONE queued behind it — a later push REPLACES the queued one (the
/// full row wins, never a stale one). Pure, so the rule is a unit test.
#[derive(Debug)]
pub(crate) struct WriteQueue<T> {
    in_flight: bool,
    queued: Option<T>,
}

impl<T> Default for WriteQueue<T> {
    fn default() -> Self {
        Self {
            in_flight: false,
            queued: None,
        }
    }
}

impl<T> WriteQueue<T> {
    /// Offer `item`: `Some(item)` = send it NOW (nothing was in flight), else
    /// it is parked as the queued one.
    pub(crate) fn push(&mut self, item: T) -> Option<T> {
        if self.in_flight {
            self.queued = Some(item);
            None
        } else {
            self.in_flight = true;
            Some(item)
        }
    }

    /// The in-flight request returned: the queued one (if any) goes next and
    /// stays in flight, else the queue is idle.
    pub(crate) fn finish(&mut self) -> Option<T> {
        match self.queued.take() {
            Some(next) => Some(next),
            None => {
                self.in_flight = false;
                None
            }
        }
    }

    /// Drop the queued write (a create or a discard supersedes it).
    pub(crate) fn clear_queued(&mut self) {
        self.queued = None;
    }

    pub(crate) fn is_idle(&self) -> bool {
        !self.in_flight && self.queued.is_none()
    }

    pub(crate) fn has_queued(&self) -> bool {
        self.queued.is_some()
    }
}

/// Can an upsert of a snapshot be skipped? Only when the row exists, the
/// snapshot equals the last one SENT, and nothing else is queued behind it —
/// a queued different snapshot would otherwise land LAST and win (pick L,
/// pick M queued, unpick M back to L: L must replace the queued M).
pub(crate) fn skip_identical_save(row_exists: bool, same_as_sent: bool, has_queued: bool) -> bool {
    row_exists && same_as_sent && !has_queued
}

/// What a failed ensuring upsert does to the upload queue: with the row
/// still missing nothing can land, so the uploads FAIL (no retry loop — the
/// next successful write is what re-ensures); with the row in place the
/// uploads carry on.
pub(crate) fn uploads_fail_after_write_error(row_exists: bool, uploads_waiting: bool) -> bool {
    !row_exists && uploads_waiting
}

/// One write of the draft row.
#[derive(Clone, Debug)]
enum DraftWrite {
    Upsert(DraftSave),
    Delete,
}

/// The draft this page composes. The id is minted at click time
/// (`api::issue_drafts::new_draft_id`) or carried over from the row it was
/// reopened from, and every write is keyed by it.
#[derive(Clone, Debug)]
struct DraftIdentity {
    id: String,
    /// Opened FROM a synced draft row (emptying it on leave deletes it).
    from_existing: bool,
    /// The page still owes writes. Cleared once the draft is filed (the
    /// server deleted the row in the create's transaction) or discarded.
    owed: bool,
    /// An upsert for this id has landed, so the row exists server-side and
    /// uploads may target it.
    ensured: bool,
}

impl DraftIdentity {
    fn row_exists(&self) -> bool {
        self.from_existing || self.ensured
    }
}

/// One eager draft upload, queued so they run ONE at a time: the first job
/// is what creates the row, and a second racing it would 404.
struct DraftUpload {
    filename: String,
    content_type: String,
    bytes: Arc<Vec<u8>>,
    kind: DraftUploadKind,
}

enum DraftUploadKind {
    /// A pasted/dropped inline image, still a `draft://` block in the editor.
    Image(StagedImage),
    /// A file picked through an attach button, a pending row until it lands.
    File(u64),
}

/// One attachment already uploaded onto the draft — what
/// `issueDrafts.listAttachments` repopulates when a draft is reopened.
#[derive(Clone, Debug)]
pub(crate) struct DraftFile {
    pub(crate) id: String,
    pub(crate) filename: String,
    pub(crate) content_type: Option<String>,
    pub(crate) size_bytes: i64,
}

/// A picked file still going up (or failed going up).
#[derive(Clone, Debug)]
pub(crate) struct PendingDraftFile {
    pub(crate) key: u64,
    pub(crate) filename: String,
    pub(crate) error: Option<SharedString>,
}

/// What the draft tells the page — everything that touches the description
/// editor needs the page's window, so it rides an event.
pub(crate) enum DraftEditorEvent {
    /// A pasted image is a real attachment now: swap its `draft://` source.
    ImageUploaded { staged: StagedImage, url: String },
    /// A pasted image failed to upload: drop it from the description.
    ImageFailed { staged: StagedImage, message: String },
    /// A non-media file joined the Files list.
    FileUploaded,
    /// EXP-824: a video/audio file joins the description as a link paragraph.
    MediaUploaded { fragment: String },
    /// `issueDrafts.listAttachments` answered.
    FilesLoaded,
    /// A row write landed.
    Saved,
}

pub(crate) struct DraftEditor {
    identity: DraftIdentity,
    /// Attachments already uploaded onto the draft.
    files: Vec<DraftFile>,
    /// `issueDrafts.listAttachments` has answered, so [`Self::files`] is the
    /// truth. Until it does, a REOPENED draft is assumed to hold files —
    /// leaving it fast must never delete a row whose only content is an
    /// attachment we had not heard about yet.
    files_loaded: bool,
    pending: Vec<PendingDraftFile>,
    next_key: u64,
    uploads: Vec<DraftUpload>,
    uploading: bool,
    /// `draft://` urls already queued, so each pasted image is claimed once.
    claimed_images: HashSet<String>,
    writes: WriteQueue<DraftWrite>,
    /// The latest snapshot the page handed over (the debounce saves it, the
    /// first upload ensures the row with it).
    latest: Option<DraftSave>,
    /// The last upsert SENT — an identical snapshot is not written twice.
    last_written: Option<DraftSave>,
    debounce: Option<Task<()>>,
    /// A create is running: autosave pauses (a write landing after the
    /// create would resurrect the row the server just deleted) and no upload
    /// starts.
    creating: bool,
    /// The window failures toast in — held so a draft the page already left
    /// (its subscription gone) can still say a write failed.
    window: Option<AnyWindowHandle>,
    /// A write failure was already toasted; quiet until a write succeeds
    /// (offline autosave must not toast every keystroke).
    failing: bool,
    /// EXP-1231: the synced row went (discarded elsewhere?) and the page is
    /// waiting out the grace: nothing is written — no save, no leave, no
    /// ensuring upsert — until [`Self::hold`] lets go (the row came back).
    held: bool,
}

impl EventEmitter<DraftEditorEvent> for DraftEditor {}

impl DraftEditor {
    /// `from_existing` = opened from a synced row (its files load at once).
    pub(crate) fn new(
        id: String,
        from_existing: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let mut this = Self {
            identity: DraftIdentity {
                id: id.clone(),
                from_existing,
                owed: true,
                ensured: false,
            },
            files: Vec::new(),
            files_loaded: !from_existing,
            pending: Vec::new(),
            next_key: 0,
            uploads: Vec::new(),
            uploading: false,
            claimed_images: HashSet::new(),
            writes: WriteQueue::default(),
            latest: None,
            last_written: None,
            debounce: None,
            creating: false,
            window,
            failing: false,
            held: false,
        };
        if from_existing {
            this.load_files(cx);
        }
        this
    }

    pub(crate) fn files(&self) -> &[DraftFile] {
        &self.files
    }

    pub(crate) fn pending(&self) -> &[PendingDraftFile] {
        &self.pending
    }

    /// An upload is queued or running — Create waits for it (web, iOS and
    /// Android disable Create the same way).
    pub(crate) fn uploads_busy(&self) -> bool {
        self.uploading
            || !self.uploads.is_empty()
            || self.pending.iter().any(|file| file.error.is_none())
    }

    /// Show `message` as an error toast in the page's window, deferred (this
    /// runs inside an entity update).
    fn toast(&self, message: String, cx: &mut gpui::Context<Self>) {
        let Some(window) = self.window else {
            return;
        };
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| crate::toast::error(message, window, cx));
        });
    }

    /// The page's state as of opening — what an unedited page must never
    /// write back (a reopened draft is already saved).
    pub(crate) fn prime(&mut self, snapshot: DraftSave) {
        if self.identity.from_existing {
            self.last_written = Some(snapshot.clone());
        }
        self.latest = Some(snapshot);
    }

    /// The attachment count [`crate::drafts::has_content`] reads: uploaded,
    /// pending and queued files all count (the first upload ensures the row
    /// anyway), and a reopened draft whose list has not landed counts as
    /// holding one.
    fn content_files(&self) -> usize {
        let known = self.files.len()
            + self.pending.iter().filter(|file| file.error.is_none()).count()
            + self.uploads.len()
            + usize::from(self.uploading);
        if self.files_loaded {
            known
        } else {
            known.max(1)
        }
    }

    /// Whether `snapshot` plus this draft's files is content — the autosave's
    /// rule, and (EXP-1212) whether leaving or discarding asks first.
    pub(crate) fn has_content(&self, snapshot: &DraftSave) -> bool {
        crate::drafts::has_content(&snapshot.title, &snapshot.description, self.content_files())
    }

    // -- autosave ---------------------------------------------------------------

    /// A title/description edit: save [`AUTOSAVE_DEBOUNCE_MS`] after the LAST
    /// one. A snapshot identical to the latest one is a repaint, not an edit,
    /// and leaves the running timer alone.
    pub(crate) fn schedule_save(&mut self, snapshot: DraftSave, cx: &mut gpui::Context<Self>) {
        if !self.identity.owed || self.creating || self.held {
            return;
        }
        if self.latest.as_ref() == Some(&snapshot) {
            return;
        }
        self.latest = Some(snapshot);
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(AUTOSAVE_DEBOUNCE_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.debounce = None;
                if let Some(latest) = this.latest.clone() {
                    this.save_now(latest, cx);
                }
            });
        }));
    }

    /// Save at once (a chip pick, a blur, the debounce firing). Nothing is
    /// written without content, nor twice for the same snapshot.
    pub(crate) fn save_now(&mut self, snapshot: DraftSave, cx: &mut gpui::Context<Self>) {
        self.debounce = None;
        if !self.identity.owed || self.creating || self.held {
            return;
        }
        self.latest = Some(snapshot.clone());
        if !self.has_content(&snapshot) {
            return;
        }
        if skip_identical_save(
            self.identity.row_exists(),
            self.last_written.as_ref() == Some(&snapshot),
            self.writes.has_queued(),
        ) {
            return;
        }
        self.enqueue(DraftWrite::Upsert(snapshot), cx);
    }

    /// Leaving the page: the ONE [`leave_action`], returned so the page can
    /// remember what it left behind. The page keeps its state (a Back and
    /// Forward resumes it), so a leave never forgets anything.
    pub(crate) fn leave(&mut self, snapshot: DraftSave, cx: &mut gpui::Context<Self>) -> LeaveAction {
        self.debounce = None;
        if self.creating {
            return LeaveAction::Nothing; // the create deletes the row (or, failing, re-saves)
        }
        if self.held {
            return LeaveAction::Nothing; // EXP-1231: the row may be consumed elsewhere
        }
        self.latest = Some(snapshot.clone());
        let action = leave_action(
            self.has_content(&snapshot),
            self.identity.from_existing,
            self.identity.ensured || !self.writes.is_idle(),
            self.identity.owed,
        );
        match action {
            LeaveAction::Save => self.save_now(snapshot, cx),
            LeaveAction::Delete => self.enqueue(DraftWrite::Delete, cx),
            LeaveAction::Nothing => {}
        }
        action
    }

    /// EXP-1212 (R3): the leave dialog's "Save draft" — save NOW and answer,
    /// once the write queue has drained, whether the last write succeeded
    /// (`false` = the page stays; the failure toasted as the normal "Could
    /// not save the draft" error, even after a quiet autosave failure). A
    /// snapshot already written (nothing to send) answers `true` at once.
    pub(crate) fn save_for_leave(
        &mut self,
        snapshot: DraftSave,
        cx: &mut gpui::Context<Self>,
    ) -> Task<bool> {
        self.failing = false;
        self.save_now(snapshot, cx);
        cx.spawn(async move |this, cx| loop {
            let settled = this
                .update(cx, |this, _| this.writes.is_idle().then(|| !this.failing))
                .ok();
            match settled {
                None => return false,
                Some(Some(ok)) => return ok,
                Some(None) => cx.background_executor().timer(SETTLE_POLL).await,
            }
        })
    }

    /// The draft was FILED: the server deleted the row in the create's own
    /// transaction, so nothing may write it again.
    pub(crate) fn mark_filed(&mut self) {
        self.identity.owed = false;
        self.creating = false;
        self.debounce = None;
        self.writes.clear_queued();
        self.uploads.clear();
    }

    /// EXP-1231: hold (or release) every write while the page waits out the
    /// `discardedGraceMs` grace of a synced row that went. A release writes
    /// nothing itself — the page saves its current state after it.
    pub(crate) fn hold(&mut self, held: bool, cx: &mut gpui::Context<Self>) {
        if self.held == held {
            return;
        }
        self.held = held;
        if held {
            self.debounce = None;
            self.writes.clear_queued();
        } else {
            self.pump_uploads(cx);
        }
    }

    /// The create failed: autosave resumes, and the page's current state is
    /// written (a flush may have dropped a queued snapshot).
    pub(crate) fn create_failed(&mut self, snapshot: DraftSave, cx: &mut gpui::Context<Self>) {
        self.creating = false;
        self.save_now(snapshot, cx);
    }

    /// "Discard draft": the row goes (if it exists, or a write that would
    /// create it is in flight) and nothing is written again.
    pub(crate) fn discard(&mut self, cx: &mut gpui::Context<Self>) {
        let delete = self.identity.row_exists() || !self.writes.is_idle();
        self.identity.owed = false;
        self.debounce = None;
        self.uploads.clear();
        self.writes.clear_queued();
        if delete {
            self.enqueue(DraftWrite::Delete, cx);
        }
    }

    /// Before Create: drop the timer and any queued write, wait for the
    /// in-flight write and uploads to settle, then answer whether the row
    /// exists (the create's `draftId`). Autosave stays paused until
    /// [`Self::mark_filed`] or [`Self::create_failed`].
    pub(crate) fn flush_for_create(&mut self, cx: &mut gpui::Context<Self>) -> Task<bool> {
        self.debounce = None;
        self.creating = true;
        self.writes.clear_queued();
        cx.spawn(async move |this, cx| loop {
            let settled = this
                .update(cx, |this, _| {
                    (this.writes.is_idle() && !this.uploading && this.uploads.is_empty())
                        .then(|| this.identity.row_exists())
                })
                .ok();
            match settled {
                None => return false,
                Some(Some(exists)) => return exists,
                Some(None) => cx.background_executor().timer(SETTLE_POLL).await,
            }
        })
    }

    fn enqueue(&mut self, write: DraftWrite, cx: &mut gpui::Context<Self>) {
        if let Some(write) = self.writes.push(write) {
            self.send(write, cx);
        }
    }

    fn send(&mut self, write: DraftWrite, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            // Signed out: nothing to write through. Release the queue.
            self.writes = WriteQueue::default();
            return;
        };
        let id = self.identity.id.clone();
        if let DraftWrite::Upsert(save) = &write {
            self.last_written = Some(save.clone());
        }
        // STRONG on purpose: the page re-points its one view at the next
        // draft and drops this editor, and a queued snapshot (or Delete)
        // behind this write must still go out — the editor lives until its
        // queue drains.
        let this = cx.entity();
        cx.spawn(async move |_, cx| {
            let is_upsert = matches!(write, DraftWrite::Upsert(_));
            let result = cx
                .background_executor()
                .spawn(async move {
                    match write {
                        DraftWrite::Upsert(save) => {
                            api::issue_drafts::issue_drafts_upsert(&trpc, &save.to_input())
                                .map(|_| ())
                        }
                        DraftWrite::Delete => {
                            api::issue_drafts::issue_drafts_delete(&trpc, &id).map(|_| ())
                        }
                    }
                })
                .await;
            this.update(cx, |this, cx| this.finish_write(is_upsert, result, cx));
        })
        .detach();
    }

    fn finish_write(
        &mut self,
        is_upsert: bool,
        result: Result<(), api::ApiError>,
        cx: &mut gpui::Context<Self>,
    ) {
        let ok = result.is_ok();
        match (is_upsert, result) {
            (true, Ok(())) => {
                self.identity.ensured = true;
                cx.emit(DraftEditorEvent::Saved);
            }
            (false, Ok(())) => {
                self.identity.ensured = false;
                self.identity.from_existing = false;
                self.last_written = None;
            }
            (true, Err(err)) => {
                log::warn!("[ui] issueDrafts.upsert({}) failed: {err}", self.identity.id);
                // Let the next save retry the same snapshot.
                self.last_written = None;
                // EXP-1231: CONFLICT = the server refusing a draft another
                // client already created or discarded. No "could not save":
                // the shape delivers the issue (or the row's absence) and the
                // page follows it.
                if !self.failing && !err.is_conflict() {
                    self.toast(format!("Could not save the draft: {}", err.user_message()), cx);
                }
            }
            (false, Err(err)) => {
                log::warn!("[ui] issueDrafts.delete({}) failed: {err}", self.identity.id);
                if !self.failing {
                    self.toast(format!("Could not delete the draft: {}", err.user_message()), cx);
                }
            }
        }
        self.failing = !ok;
        if let Some(next) = self.writes.finish() {
            self.send(next, cx);
        }
        if ok {
            // The first upload waits for the row to exist.
            self.pump_uploads(cx);
        } else if uploads_fail_after_write_error(
            self.identity.row_exists(),
            !self.uploads.is_empty(),
        ) {
            // No row, no uploads: fail them rather than re-ensuring in a loop.
            self.fail_all_uploads("Could not save the draft.".to_string(), cx);
        }
        cx.notify();
    }

    // -- uploads ----------------------------------------------------------------

    /// A freshly pasted image joins the eager-upload queue, exactly once.
    pub(crate) fn queue_image(
        &mut self,
        staged: StagedImage,
        snapshot: DraftSave,
        cx: &mut gpui::Context<Self>,
    ) {
        // During a create the image stays staged: the create uploads it.
        if !self.identity.owed
            || self.creating
            || !self.claimed_images.insert(staged.draft_url.clone())
        {
            return;
        }
        self.latest = Some(snapshot);
        self.uploads.push(DraftUpload {
            filename: staged.filename.clone(),
            content_type: staged.content_type.clone(),
            bytes: staged.bytes.clone(),
            kind: DraftUploadKind::Image(staged),
        });
        self.pump_uploads(cx);
    }

    /// A picked file (already read off disk) goes up NOW, onto the draft; it
    /// shows as a pending row while it runs.
    pub(crate) fn queue_file(
        &mut self,
        filename: String,
        content_type: String,
        bytes: Vec<u8>,
        snapshot: DraftSave,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.identity.owed {
            return;
        }
        if self.creating {
            self.toast(format!("{filename} was not attached: the issue is being created."), cx);
            return;
        }
        self.latest = Some(snapshot);
        let key = self.next_key;
        self.next_key += 1;
        self.pending.push(PendingDraftFile {
            key,
            filename: filename.clone(),
            error: None,
        });
        self.uploads.push(DraftUpload {
            filename,
            content_type,
            bytes: Arc::new(bytes),
            kind: DraftUploadKind::File(key),
        });
        cx.notify();
        self.pump_uploads(cx);
    }

    /// A failed pending row's ✕.
    pub(crate) fn dismiss_pending(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        self.pending.retain(|file| file.key != key);
        cx.notify();
    }

    /// Run the queue ONE job at a time. The row must exist first: when it
    /// does not, ONE upsert of the latest snapshot goes through the write
    /// queue, and its completion pumps again.
    fn pump_uploads(&mut self, cx: &mut gpui::Context<Self>) {
        if self.uploading
            || self.uploads.is_empty()
            || !self.identity.owed
            || self.creating
            || self.held
        {
            return;
        }
        let Some(transport) = crate::queries::attachment_transport(cx) else {
            self.fail_all_uploads("Not signed in.".to_string(), cx);
            return;
        };
        if !self.identity.row_exists() {
            if self.writes.is_idle() {
                let Some(snapshot) = self.latest.clone() else {
                    return;
                };
                self.enqueue(DraftWrite::Upsert(snapshot), cx);
            }
            return;
        }
        let job = self.uploads.remove(0);
        self.uploading = true;
        let draft_id = self.identity.id.clone();
        let filename = job.filename.clone();
        let content_type = job.content_type.clone();
        let bytes = job.bytes.clone();
        // STRONG: an upload outlives a re-point like a write does.
        let this = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    transport
                        .upload_draft(&draft_id, &filename, &content_type, &bytes)
                        .map_err(|err| err.to_string())
                })
                .await;
            this.update(cx, |this, cx| this.finish_upload(job, result, cx));
        })
        .detach();
    }

    /// Land one eager upload: an image swaps its `draft://` source for the
    /// canonical one, a video/audio file joins the description as a plain
    /// link paragraph (EXP-824), everything else joins the Files list.
    fn finish_upload(
        &mut self,
        job: DraftUpload,
        result: Result<UploadedImage, String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.uploading = false;
        match (job.kind, result) {
            (DraftUploadKind::Image(staged), Ok(uploaded)) => {
                self.claimed_images.remove(&staged.draft_url);
                cx.emit(DraftEditorEvent::ImageUploaded {
                    staged,
                    url: uploaded.url,
                });
            }
            (DraftUploadKind::Image(staged), Err(message)) => {
                self.claimed_images.remove(&staged.draft_url);
                cx.emit(DraftEditorEvent::ImageFailed { staged, message });
            }
            (DraftUploadKind::File(key), Ok(uploaded)) => {
                self.pending.retain(|file| file.key != key);
                match crate::issue_files::description_embed(&job.content_type) {
                    Some(embed) => {
                        let fragment = crate::issue_files::description_fragment(
                            embed,
                            uploaded.filename.as_deref(),
                            &uploaded.url,
                        );
                        cx.emit(DraftEditorEvent::MediaUploaded { fragment });
                    }
                    None => {
                        self.files.push(DraftFile {
                            id: uploaded.id,
                            filename: uploaded.filename.unwrap_or(job.filename),
                            content_type: uploaded.content_type.or(Some(job.content_type)),
                            size_bytes: uploaded.size_bytes.unwrap_or(job.bytes.len() as i64),
                        });
                        cx.emit(DraftEditorEvent::FileUploaded);
                    }
                }
            }
            (DraftUploadKind::File(key), Err(message)) => {
                if let Some(file) = self.pending.iter_mut().find(|file| file.key == key) {
                    file.error = Some(message.into());
                }
            }
        }
        cx.notify();
        self.pump_uploads(cx);
    }

    /// Nothing to upload through (signed out) — drop every queued job with
    /// one message rather than leaving half-rendered rows behind.
    fn fail_all_uploads(&mut self, message: String, cx: &mut gpui::Context<Self>) {
        for job in std::mem::take(&mut self.uploads) {
            match job.kind {
                DraftUploadKind::Image(staged) => {
                    self.claimed_images.remove(&staged.draft_url);
                    cx.emit(DraftEditorEvent::ImageFailed {
                        staged,
                        message: message.clone(),
                    });
                }
                DraftUploadKind::File(key) => {
                    if let Some(file) = self.pending.iter_mut().find(|file| file.key == key) {
                        file.error = Some(message.clone().into());
                    }
                }
            }
        }
        cx.notify();
    }

    // -- files ------------------------------------------------------------------

    /// Repopulate the files of a REOPENED draft. Draft-owned attachments are
    /// server-only rows (the `attachments` shape excludes them), so this is
    /// the only way to see them again.
    pub(crate) fn load_files(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let draft_id = self.identity.id.clone();
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move {
                    api::issue_drafts::issue_drafts_list_attachments(&trpc, &draft_id)
                })
                .await;
            let rows = match rows {
                Ok(rows) => rows,
                Err(err) => {
                    log::warn!("[ui] issueDrafts.listAttachments failed: {err}");
                    let message = format!("Could not load the draft's files: {}", err.user_message());
                    let _ = this.update(cx, |this, cx| this.toast(message, cx));
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                let listed: Vec<DraftFile> = rows
                    .into_iter()
                    // Inline images and media live IN the description, not in
                    // the Files list — the same split the issue detail makes.
                    .filter(|row| {
                        !crate::issue_files::is_inline_media(row.content_type.as_deref())
                    })
                    .map(|row| DraftFile {
                        id: row.id,
                        filename: row.filename.unwrap_or_else(|| "file".to_string()),
                        content_type: row.content_type,
                        size_bytes: row.size_bytes.unwrap_or(0),
                    })
                    .collect();
                this.files = merge_files(listed, std::mem::take(&mut this.files));
                this.files_loaded = true;
                cx.emit(DraftEditorEvent::FilesLoaded);
                cx.notify();
            });
        })
        .detach();
    }

    /// Delete one uploaded draft attachment. The row is server-only, so the
    /// file is dropped locally — there is no echo.
    pub(crate) fn remove_file(&mut self, attachment_id: String, cx: &mut gpui::Context<Self>) {
        let Some(index) = self.files.iter().position(|file| file.id == attachment_id) else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let removed = self.files.remove(index);
        cx.notify();
        let this = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::attachments::attachments_delete(&trpc, &attachment_id) })
                .await;
            if let Err(err) = result {
                log::warn!("[ui] draft attachment delete failed: {err}");
                // The file is still there — put its row back.
                this.update(cx, |this, cx| {
                    let message = format!("Could not delete {}: {}", removed.filename, err.user_message());
                    let at = index.min(this.files.len());
                    this.files.insert(at, removed);
                    this.toast(message, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

/// `issueDrafts.listAttachments` answered: its rows, plus any file this page
/// uploaded before the answer that the list does not carry yet (merged by
/// id, the listed row winning).
pub(crate) fn merge_files(listed: Vec<DraftFile>, local: Vec<DraftFile>) -> Vec<DraftFile> {
    let mut merged = listed;
    for file in local {
        if !merged.iter().any(|known| known.id == file.id) {
            merged.push(file);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ONE leave rule, every row of it.
    #[test]
    fn leave_action_table() {
        use LeaveAction::*;
        // (has_content, from_existing, ensured, owed) → action
        let rows = [
            ((true, false, false, true), Save),
            ((true, true, false, true), Save),
            ((true, false, true, true), Save),
            // An emptied draft that exists is deleted…
            ((false, true, false, true), Delete),
            ((false, false, true, true), Delete),
            ((false, true, true, true), Delete),
            // …a blank page that never wrote anything stays free.
            ((false, false, false, true), Nothing),
            // A filed or discarded draft owes nothing at all.
            ((true, true, true, false), Nothing),
            ((false, true, true, false), Nothing),
            ((true, false, false, false), Nothing),
        ];
        for ((content, existing, ensured, owed), expected) in rows {
            assert_eq!(
                leave_action(content, existing, ensured, owed),
                expected,
                "content={content} existing={existing} ensured={ensured} owed={owed}"
            );
        }
    }

    /// Single-flight: one request at a time, ONE queued snapshot that every
    /// later one replaces, sent when the request returns.
    #[test]
    fn writes_coalesce_to_one_in_flight_and_one_queued() {
        let mut queue = WriteQueue::default();
        assert!(queue.is_idle());
        assert_eq!(queue.push(1), Some(1), "an idle queue sends at once");
        assert!(!queue.is_idle());
        assert_eq!(queue.push(2), None, "a second write waits");
        assert_eq!(queue.push(3), None, "and a third replaces it");
        assert_eq!(queue.finish(), Some(3), "the latest queued one goes next");
        assert!(!queue.is_idle(), "still in flight while it runs");
        assert_eq!(queue.finish(), None);
        assert!(queue.is_idle());
        // A create/discard drops the queued snapshot, never the in-flight one.
        assert_eq!(queue.push(4), Some(4));
        assert_eq!(queue.push(5), None);
        queue.clear_queued();
        assert_eq!(queue.finish(), None);
        assert!(queue.is_idle());
    }

    /// S2: an identical snapshot is skipped only when nothing different is
    /// queued behind the last one sent.
    #[test]
    fn an_identical_save_is_skipped_only_with_nothing_queued() {
        assert!(skip_identical_save(true, true, false));
        // pick L (sent), pick M (queued), unpick M == L: L must replace M.
        assert!(!skip_identical_save(true, true, true));
        assert!(!skip_identical_save(true, false, false));
        // No row yet: the write is what creates it.
        assert!(!skip_identical_save(false, true, false));
    }

    /// B2: a failed ensuring upsert fails the waiting uploads instead of
    /// re-ensuring forever; with the row in place they carry on.
    #[test]
    fn a_failed_ensure_fails_the_uploads_instead_of_retrying() {
        assert!(uploads_fail_after_write_error(false, true));
        assert!(!uploads_fail_after_write_error(true, true));
        assert!(!uploads_fail_after_write_error(false, false));
    }

    fn file(id: &str, name: &str) -> DraftFile {
        DraftFile {
            id: id.into(),
            filename: name.into(),
            content_type: None,
            size_bytes: 1,
        }
    }

    /// A file uploaded before `listAttachments` answered survives the answer.
    #[test]
    fn listed_files_merge_with_local_uploads_by_id() {
        let merged = merge_files(
            vec![file("a", "listed-a"), file("b", "listed-b")],
            vec![file("b", "local-b"), file("c", "local-c")],
        );
        let ids: Vec<(&str, &str)> = merged
            .iter()
            .map(|file| (file.id.as_str(), file.filename.as_str()))
            .collect();
        assert_eq!(ids, [("a", "listed-a"), ("b", "listed-b"), ("c", "local-c")]);
    }

    #[test]
    fn the_queue_reports_a_queued_write() {
        let mut queue = WriteQueue::default();
        assert!(!queue.has_queued());
        queue.push(1);
        assert!(!queue.has_queued());
        queue.push(2);
        assert!(queue.has_queued());
        queue.finish();
        assert!(!queue.has_queued());
    }

    #[test]
    fn autosave_debounce_is_the_contracts() {
        assert_eq!(AUTOSAVE_DEBOUNCE_MS, domain::issue_draft::AUTOSAVE_DEBOUNCE_MS);
        assert_eq!(AUTOSAVE_DEBOUNCE_MS, 800);
    }
}
