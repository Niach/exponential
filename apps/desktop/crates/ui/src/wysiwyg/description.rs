//! EXP-261: `WysiwygDescription` — the host view wrapping the vendored
//! WYSIWYG editor entity for the issue-description surfaces (issue detail +
//! create-issue dialog). Owns focus tracking (save-on-blur), theme
//! observation, and the image pipeline (paste → stage/upload → resolve, the
//! `?w=` resize commit, and the image context menu).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    anchored, deferred, div, point, px, App, AppContext as _, ClipboardItem, Context, Entity,
    FocusHandle, Focusable, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, Point, Render, SharedString, Styled as _, Subscription, Window,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{
    h_flex, notification::Notification, v_flex, ActiveTheme as _, Icon, WindowExt as _,
};
use gpui_markdown_editor::{
    DismissTransientUi, FocusNext, FocusPrev, FormatCommand, ImageSourceResolution, IndentBlock,
    MarkdownEditor as VendoredEditor, MarkdownEditorEvent, MarkdownEditorMode,
    MarkdownEditorOptions, Newline, ReferenceKind,
};

use super::images::{self, SharedImageState, WysiwygImageResolver, WysiwygPasteHandler};
use super::refs::{refresh_ref_state, SharedRefState, WysiwygReferenceDecorator};
use crate::markdown::image_paste::{markdown_for_save, DRAFT_SCHEME};
use crate::markdown::{
    detect_trigger, download_image, normalize_for_wysiwyg, restore_blank_line_markers,
    store_completion_source, CompletionItem, CompletionSource, ImageCache, ImageSlot, StagedImage,
};
use crate::queries;

/// Save hook: current markdown at save time (same shape as
/// [`crate::issue_detail::OnSaveDescription`]).
pub(crate) type OnSave = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// EXP-335: host hook for NON-inline-image files picked via the toolbar's
/// attach button (detail: upload to the issue's Files section; create dialog:
/// stage for post-create upload). Inline-image picks never reach it — they
/// embed at the caret through the editor's own image pipeline.
pub(crate) type OnAttachFiles = Rc<dyn Fn(Vec<std::path::PathBuf>, &mut Window, &mut App)>;

/// An open image context menu (host-rendered — the vendored editor only
/// reports the right-click via `ImageContextMenuRequested`).
struct ImageMenuState {
    src: String,
    position: Point<Pixels>,
}

/// An active `@`/`#` completion (host-rendered popup over the vendored
/// caret; §4.6 parity with the block editor).
struct ActiveCompletion {
    /// Byte length of the pending token (trigger char + query) — what
    /// acceptance replaces before the caret.
    token_len: usize,
    items: Vec<CompletionItem>,
    selected: usize,
}

pub struct WysiwygDescription {
    pub(super) editor: Entity<VendoredEditor>,
    focus_handle: FocusHandle,
    placeholder: SharedString,
    /// Pills + autocomplete scope (the issue's team).
    team_id: Option<String>,
    /// `Some(issue_id)` = detail mode (immediate upload on paste);
    /// `None` = create-dialog mode (stage as `draft://`, resolve at submit).
    upload_issue: Option<String>,
    on_save: Option<OnSave>,
    /// EXP-335: set by hosts with a Files destination — gates the toolbar's
    /// attach button (the action-prompt editor has none, so no button).
    on_attach_files: Option<OnAttachFiles>,
    /// Authenticated fetch/decode cache (same type the block editor uses).
    images: Entity<ImageCache>,
    /// State shared with the vendored environment's paste handler + resolver.
    shared: Arc<SharedImageState>,
    /// Member/issue snapshot behind the reference-pill decorator.
    refs: Arc<SharedRefState>,
    completion_source: Option<Rc<dyn CompletionSource>>,
    completion: Option<ActiveCompletion>,
    /// Images staged in create-dialog mode (`draft://` URLs; resolved at
    /// submit by the dialog's existing upload flow).
    staged: Vec<StagedImage>,
    image_menu: Option<ImageMenuState>,
    /// Open toolbar link editor (the URL field replaces the Link button while
    /// it is up). `None` = the button is showing. The subscription rides along
    /// so Enter commits: gpui-component binds Enter to its own `Input` action,
    /// so the keystroke never reaches this view's key handler.
    pub(super) link_input: Option<(Entity<InputState>, Subscription)>,
    /// Vendored-editor revision at the last load/save. The vendored engine
    /// NORMALIZES many render-equivalent forms (`_i_`→`*i*`, setext→ATX,
    /// `1)`→`1.`, …), so re-serialized bytes differing from the loaded
    /// description does NOT mean the user edited — the revision counter only
    /// moves on real edits (typed input, undo/redo, structural commands,
    /// image rewrites), never on `replace_markdown`/construction. Blur saves
    /// only when the live revision has moved past this (EXP-261).
    clean_revision: u64,
    /// EXP-285: memoized header-probed natural sizes per cache key — the
    /// fallback when the synced `attachments` row carries no dimensions
    /// (drafts, legacy rows). The synced row stays authoritative.
    probed_sizes: HashMap<String, (f32, f32)>,
    /// EXP-285: `true` = the host renders the toolbar itself (the
    /// create-issue dialog pins it above its scroll region) and the inline
    /// toolbar is suppressed.
    external_toolbar: bool,
    /// EXP-354: a retry for `Failed` attachment fetches is already pending —
    /// at most one timer runs at a time.
    failed_retry_scheduled: bool,
    _subscriptions: Vec<Subscription>,
}

impl WysiwygDescription {
    pub fn new(
        team_id: Option<String>,
        upload_issue: Option<String>,
        placeholder: &str,
        initial_markdown: &str,
        on_save: Option<OnSave>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = SharedString::from(placeholder.to_string());
        let transport = queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        let shared = Arc::new(SharedImageState::default());
        let refs = Arc::new(SharedRefState::default());
        let completion_source: Option<Rc<dyn CompletionSource>> = team_id
            .clone()
            .map(|team_id| store_completion_source(team_id) as Rc<dyn CompletionSource>);
        // Resize writes a `?w=` param back into the document — only sensible
        // in detail mode where the image is (or becomes) a real attachment.
        let enable_image_resize = upload_issue.is_some();

        let editor = cx.new(|cx| {
            let mut environment = gpui_markdown_editor::MarkdownEditorEnvironment::default();
            environment.theme = super::editor_theme_with_placeholder(cx, placeholder.as_ref());
            environment.show_source_line_numbers = false;
            environment.image_paste_handler = Arc::new(WysiwygPasteHandler {
                state: shared.clone(),
            });
            environment.image_source_resolver = Some(Arc::new(WysiwygImageResolver {
                state: shared.clone(),
            }));
            environment.enable_image_resize = enable_image_resize;
            // Images keep their own `…` menu and drag handles; clicking one
            // must never swap the picture for its markdown source.
            environment.enable_image_source_editing = false;
            // Links render as links with the caret inside them; the toolbar's
            // link control edits the target (web parity).
            environment.expand_focused_links = false;
            if team_id.is_some() {
                environment.reference_decorator = Some(Arc::new(WysiwygReferenceDecorator {
                    state: refs.clone(),
                }));
            }
            VendoredEditor::new(
                normalize_for_wysiwyg(initial_markdown),
                MarkdownEditorOptions {
                    mode: MarkdownEditorMode::Rendered,
                    environment,
                    embedded: true,
                    ..MarkdownEditorOptions::default()
                },
                cx,
            )
        });

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe_in(
            &editor,
            window,
            |this, _editor, event: &MarkdownEditorEvent, window, cx| match event {
                MarkdownEditorEvent::Changed { .. } => {
                    this.after_change(window, cx);
                    this.refresh_completion(window, cx);
                    cx.notify();
                }
                MarkdownEditorEvent::OpenLinkRequested(request) => {
                    if let Err(error) = api::opener::open_in_browser(&request.open_target) {
                        log::warn!("open link failed: {error}");
                    }
                }
                MarkdownEditorEvent::ImageContextMenuRequested { src, position } => {
                    this.image_menu = Some(ImageMenuState {
                        src: src.clone(),
                        position: *position,
                    });
                    cx.notify();
                }
                MarkdownEditorEvent::ImageResized { src, width } => {
                    this.commit_image_width(src, *width, window, cx);
                }
                MarkdownEditorEvent::ReferenceClicked { kind, value } => {
                    // Issue pills navigate in-app; mention pills are inert
                    // (block-editor parity).
                    if *kind == ReferenceKind::IssueRef {
                        if let Some(team_id) = this.team_id.clone() {
                            let identifier = value.trim_start_matches('#');
                            crate::description_editor::open_issue_by_identifier(
                                &team_id, identifier, window, cx,
                            );
                        }
                    }
                }
                MarkdownEditorEvent::Error { message } => {
                    log::warn!("wysiwyg editor error: {message}");
                }
                // EXP-421: files dropped onto the editor — same type policy
                // as `pick_attach`: inline images embed at the drop index
                // (the `Changed` event then drives the upload pipeline),
                // everything else rides the host attach hook.
                MarkdownEditorEvent::ExternalFilesDropped { paths, root_index } => {
                    let (images, others): (Vec<_>, Vec<_>) =
                        paths.iter().cloned().partition(|path| {
                            crate::markdown::image_paste::is_inline_image_path(path)
                        });
                    if !images.is_empty() {
                        let root_index = *root_index;
                        this.editor.update(cx, |editor, cx| {
                            editor.insert_image_paths_at(root_index, images, cx);
                        });
                    }
                    if !others.is_empty() {
                        if let Some(on_attach) = this.on_attach_files.clone() {
                            on_attach(others, window, cx);
                        }
                    }
                }
                MarkdownEditorEvent::SelectionChanged(_) => {
                    // Caret moves re-anchor or dismiss the popup, and move the
                    // toolbar's pressed-button state onto the new block.
                    this.refresh_completion(window, cx);
                    cx.notify();
                }
                MarkdownEditorEvent::ModeChanged { .. } => {}
            },
        ));

        // Save-on-blur: gpui's focus path includes this wrapper's handle for
        // every descendant block (track_focus ancestor), so focus_out fires
        // only when focus leaves the WHOLE editor — block-to-block moves
        // (Enter splits, clicks) never trigger it. The detail view's
        // `last_saved_description` dedupe absorbs any residual double-fire.
        let focus_handle = cx.focus_handle();
        {
            let this = cx.entity().downgrade();
            subscriptions.push(window.on_focus_out(
                &focus_handle,
                cx,
                move |_event, window, cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            // EXP-261: only a real user edit saves. A
                            // view-only open must never persist the
                            // serializer's normalizations over what other
                            // clients wrote.
                            if this.is_dirty(cx) {
                                this.save_now(window, cx);
                            }
                        });
                    }
                },
            ));
        }

        // Loading → Ready repaints: the cache notifies when a fetch lands.
        subscriptions.push(cx.observe(&images, |this, _images, cx| {
            this.sync_images(cx);
        }));

        // EXP-354: the synced `attachments` row can land after the issues
        // echo that referenced it — re-derive natural sizes (and resolutions)
        // when it does; the equality gates in `sync_images` keep quiet
        // updates from repainting.
        if let Some(store) = sync::Store::try_global(cx) {
            let attachments = store.collections().attachments.clone();
            subscriptions.push(cx.observe(&attachments, |this, _, cx| {
                this.sync_images(cx);
            }));
        }

        // Presentation-only theme refresh on light/dark switches.
        subscriptions.push(cx.observe_global::<gpui_component::Theme>(|this, cx| {
            let theme = super::editor_theme_with_placeholder(cx, this.placeholder.as_ref());
            this.editor
                .update(cx, |editor, cx| editor.set_theme(theme, cx));
        }));

        let clean_revision = editor.read(cx).revision();
        let mut this = Self {
            editor,
            focus_handle,
            placeholder,
            team_id,
            upload_issue,
            on_save,
            on_attach_files: None,
            images,
            shared,
            staged: Vec::new(),
            image_menu: None,
            link_input: None,
            clean_revision,
            refs,
            completion_source,
            completion: None,
            probed_sizes: HashMap::new(),
            external_toolbar: false,
            failed_retry_scheduled: false,
            _subscriptions: subscriptions,
        };
        // Kick off fetches for images already present in the description and
        // seed the reference-pill snapshot.
        this.sync_images(cx);
        this.sync_refs(cx);
        this
    }

    /// Current canonical GFM — what every persist site writes. The vendored
    /// serializer IS the canonical form on this path (no comrak
    /// `canonicalize` pass, see wysiwyg_parity.rs); the one thing it cannot
    /// express is the `&nbsp;` blank-line marker it trimmed on the way in
    /// (EXP-271), which [`restore_blank_line_markers`] writes back.
    pub fn markdown(&self, cx: &App) -> String {
        restore_blank_line_markers(&self.editor.read(cx).markdown(cx))
    }

    /// Replace the whole buffer (remote echo / dialog reset). Resets undo
    /// history, matching the vendored `replace_markdown` semantics.
    pub fn set_markdown(&mut self, markdown: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let markdown = normalize_for_wysiwyg(markdown);
        self.editor
            .update(cx, |editor, cx| editor.replace_markdown(markdown, cx));
        // A full buffer replace discards any staged (unsubmitted) images.
        self.staged.clear();
        // EXP-261: programmatic loads are clean — the buffer now mirrors the
        // remote truth, so there is nothing user-authored to persist.
        self.clean_revision = self.editor.read(cx).revision();
        self.sync_images(cx);
        cx.notify();
    }

    /// EXP-261: has the user actually edited since the last load/save? Keyed
    /// off the vendored revision counter, which never moves on programmatic
    /// content loading — only on real edits.
    pub fn is_dirty(&self, cx: &App) -> bool {
        self.editor.read(cx).revision() != self.clean_revision
    }

    /// EXP-261: record that the current content was just persisted or synced
    /// through a path outside [`Self::save_now`] (the detail view's
    /// tab-switch flush saves through its own tRPC hook).
    pub fn mark_clean(&mut self, cx: &App) {
        self.clean_revision = self.editor.read(cx).revision();
    }

    /// Images staged in create-dialog mode (fed to the dialog's submit-time
    /// `upload_staged_images` + `rewrite_image_urls` flow).
    pub fn staged_images(&self, _cx: &App) -> Vec<StagedImage> {
        self.staged.clone()
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle.contains_focused(window, cx)
    }

    pub fn focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.focus_first_block(cx));
    }

    /// EXP-285: caret to the very end of the document — the textarea
    /// "click the empty area below the text" affordance (the render's filler
    /// strip routes here).
    pub fn focus_end(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.focus_document_end(cx));
    }

    /// EXP-285: suppress the inline toolbar — the host renders it via
    /// [`Self::toolbar_row`] instead (the create-issue dialog pins it above
    /// its scroll region so it never scrolls away with a long description).
    pub fn use_external_toolbar(&mut self) {
        self.external_toolbar = true;
    }

    /// EXP-288: hand the HOST's tracked scroll handle to the vendored
    /// editor so its caret-follow machinery keeps the caret visible inside
    /// the host's scroll container while typing/pasting.
    pub fn set_scroll_handle(&mut self, handle: gpui::ScrollHandle, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, _| editor.set_external_scroll_handle(handle));
    }

    /// The toolbar row for [`Self::use_external_toolbar`] hosts — called via
    /// `entity.update` from the host's render (the `render_tab_strip`
    /// precedent: safe outside this view's own render pass).
    pub fn toolbar_row(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.render_toolbar(window, cx)
    }

    // -- save ---------------------------------------------------------------

    fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(on_save) = self.on_save.clone() else {
            return;
        };
        // EXP-261: everything up to the current revision is being persisted;
        // a later blur with no further edits must not re-save. The vendored
        // revision bumps synchronously inside `mark_dirty`, so structural
        // commits (image rewrite/resize) that call `save_now` right after
        // mutating already see their own bump here.
        self.clean_revision = self.editor.read(cx).revision();
        // A `draft://` URL must never reach the server. In detail mode a
        // still-uploading paste is stripped from THIS save; the upload's own
        // completion save re-adds it with the real attachment URL. The same
        // shared derivation backs every other persist site (EXP-261 —
        // `DescriptionEditor::markdown_for_save`, create-dialog submit).
        on_save(markdown_for_save(self.markdown(cx)), window, cx);
    }

    // -- image pipeline -----------------------------------------------------

    /// Post-change bookkeeping: adopt freshly pasted images (upload in detail
    /// mode, stage in dialog mode) and refresh resolutions.
    fn after_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let drained: Vec<StagedImage> = self
            .shared
            .paste_inbox
            .lock()
            .map(|mut inbox| inbox.drain(..).collect())
            .unwrap_or_default();
        for staged in drained {
            if self.upload_issue.is_some() {
                self.spawn_upload(staged, window, cx);
            } else {
                self.staged.push(staged);
            }
        }
        self.sync_images(cx);
        self.sync_refs(cx);
    }

    /// Refresh the member/issue snapshot the pill decorator reads. Runs on
    /// init and every change — newly synced members/issues decorate on the
    /// next edit (or reopen).
    fn sync_refs(&mut self, cx: &mut Context<Self>) {
        if let Some(team_id) = self.team_id.clone() {
            refresh_ref_state(&self.refs, &team_id, cx);
        }
    }

    fn spawn_upload(&mut self, staged: StagedImage, window: &mut Window, cx: &mut Context<Self>) {
        let Some(transport) = self.images.read(cx).transport() else {
            return;
        };
        let Some(issue_id) = self.upload_issue.clone() else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let upload = {
                let staged = staged.clone();
                cx.background_executor()
                    .spawn(async move {
                        transport.upload(
                            &issue_id,
                            &staged.filename,
                            &staged.content_type,
                            &staged.bytes,
                        )
                    })
                    .await
            };
            match upload {
                Ok(uploaded) => {
                    let _ = this.update_in(cx, |this, window, cx| {
                        let real_key = images::cache_key(&uploaded.url).to_string();
                        if let Ok(mut resolutions) = this.shared.resolutions.lock() {
                            if let Some(existing) = resolutions.get(&staged.draft_url).cloned() {
                                resolutions.insert(real_key.clone(), existing);
                            }
                        }
                        // EXP-285: carry the probed natural size across the
                        // draft→real rewrite so there is no one-frame
                        // letterbox flash before the next sync re-probes.
                        let draft_size =
                            this.probed_sizes.get(&staged.draft_url).copied().or_else(|| {
                                this.shared
                                    .natural_sizes
                                    .lock()
                                    .ok()
                                    .and_then(|sizes| sizes.get(&staged.draft_url).copied())
                            });
                        if let Some(size) = draft_size {
                            this.probed_sizes.insert(real_key.clone(), size);
                            if let Ok(mut sizes) = this.shared.natural_sizes.lock() {
                                sizes.insert(real_key.clone(), size);
                            }
                        }
                        this.images.update(cx, |cache, _cx| {
                            cache.insert_bytes(
                                real_key,
                                &staged.content_type,
                                staged.bytes.as_ref().clone(),
                            );
                        });
                        let mut map = HashMap::new();
                        map.insert(staged.draft_url.clone(), uploaded.url.clone());
                        this.editor
                            .update(cx, |editor, cx| editor.rewrite_image_sources(&map, cx));
                        this.refresh_editor_environment(cx);
                        // Structural commit (masterplan §8.2): the insert has
                        // no blur to ride on — persist immediately.
                        this.save_now(window, cx);
                    });
                }
                Err(error) => {
                    log::warn!("image upload failed: {error}");
                    let _ = this.update_in(cx, |this, window, cx| {
                        // EXP-261: a failed draft must not linger — it is
                        // invisible to the user as a `Failed` placeholder yet
                        // silently stripped from every save. Remove it from
                        // the document (same path as the context-menu Delete)
                        // and surface the failure like sibling views do.
                        this.delete_image(&staged.draft_url, window, cx);
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "Image upload failed: {error}"
                            ))),
                            cx,
                        );
                    });
                }
            }
        })
        .detach();
    }

    /// Recompute the resolver map from the live document (kicking off fetches
    /// through the authenticated cache) and repaint the editor if it moved.
    fn sync_images(&mut self, cx: &mut Context<Self>) {
        let markdown = self.markdown(cx);
        let occurrences = crate::attachments_row::extract_image_occurrences(&markdown);
        let mut next: HashMap<String, ImageSourceResolution> = HashMap::new();
        let mut natural: HashMap<String, (f32, f32)> = HashMap::new();
        for occurrence in &occurrences {
            let src = occurrence.url.as_str();
            if !images::is_hosted_src(src) {
                continue;
            }
            let key = images::cache_key(src).to_string();
            if next.contains_key(&key) {
                continue;
            }
            let resolution = if src.starts_with(DRAFT_SCHEME) {
                // Draft bytes were inserted by the paste handler; keep
                // whatever state they carry (Decoded, or Failed on upload
                // error).
                self.shared
                    .resolutions
                    .lock()
                    .ok()
                    .and_then(|resolutions| resolutions.get(&key).cloned())
                    .unwrap_or(ImageSourceResolution::Failed)
            } else {
                let slot = self.images.update(cx, |cache, cx| cache.slot(&key, cx));
                match slot {
                    ImageSlot::Ready(image) => ImageSourceResolution::Decoded(image),
                    ImageSlot::Loading => ImageSourceResolution::Pending,
                    ImageSlot::Failed(_) => ImageSourceResolution::Failed,
                }
            };
            // EXP-285: natural size — the synced `attachments` row first
            // (the cross-client contract stays authoritative), else a
            // memoized header probe of the decoded bytes. Without a size the
            // vendored editor letterboxes the picture in a max-height
            // Contain box (huge empty bands) — this makes every draft paste
            // and dimension-less legacy row render aspect-correct.
            let size = crate::markdown::attachment_natural_size(src, cx).or_else(|| {
                if let Some(memoized) = self.probed_sizes.get(&key).copied() {
                    return Some(memoized);
                }
                let ImageSourceResolution::Decoded(image) = &resolution else {
                    return None;
                };
                let probed = images::probe_natural_size(&image.bytes);
                if let Some(probed) = probed {
                    self.probed_sizes.insert(key.clone(), probed);
                }
                probed
            });
            if let Some(size) = size {
                natural.insert(key.clone(), size);
            }
            next.insert(key, resolution);
        }

        let sizes_changed = self
            .shared
            .natural_sizes
            .lock()
            .map(|mut sizes| {
                if *sizes == natural {
                    false
                } else {
                    *sizes = natural;
                    true
                }
            })
            .unwrap_or(false);
        // EXP-354: a failed FETCH (never a failed `draft://` upload — its
        // bytes are gone for good) must not stay broken until the issue is
        // reopened. The block editor re-fetches from its render loop; this
        // surface has no render-driven `slot()` call, so schedule the retry
        // explicitly.
        let any_failed_fetch = next.iter().any(|(key, resolution)| {
            matches!(resolution, ImageSourceResolution::Failed) && !key.starts_with(DRAFT_SCHEME)
        });
        let changed = self
            .shared
            .resolutions
            .lock()
            .map(|mut resolutions| {
                if images::resolutions_equal(&resolutions, &next) {
                    false
                } else {
                    *resolutions = next;
                    true
                }
            })
            .unwrap_or(false);
        if changed || sizes_changed {
            self.refresh_editor_environment(cx);
        }
        if any_failed_fetch {
            self.schedule_failed_retry(cx);
        }
    }

    /// EXP-354: one delayed re-sync per failure round. After the backoff the
    /// `Failed` slots are evicted so [`ImageCache::slot`] re-fetches
    /// immediately, and a transport that was missing at construction (editor
    /// built mid-login) is re-resolved. Still-failing images re-enter through
    /// `sync_images`, so the retry keeps its `IMAGE_RETRY_AFTER` cadence.
    fn schedule_failed_retry(&mut self, cx: &mut Context<Self>) {
        if self.failed_retry_scheduled {
            return;
        }
        self.failed_retry_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::markdown::IMAGE_RETRY_AFTER)
                .await;
            this.update(cx, |this, cx| {
                this.failed_retry_scheduled = false;
                if this.images.read(cx).transport().is_none() {
                    let transport = queries::attachment_transport(cx);
                    this.images
                        .update(cx, |cache, _| cache.set_transport(transport));
                }
                this.images.update(cx, |cache, _| cache.evict_failed());
                this.sync_images(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Redistribute the (unchanged) environment so the vendored editor
    /// rebuilds its image runtimes against the fresh resolver state.
    fn refresh_editor_environment(&mut self, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let environment = editor.environment().clone();
            editor.set_environment(environment, cx);
        });
    }

    fn commit_image_width(
        &mut self,
        src: &str,
        width: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_src =
            crate::markdown::image_url::src_with_width(src, width.map(|w| w.max(1.0) as u32));
        if next_src == src {
            return;
        }
        let mut map = HashMap::new();
        map.insert(src.to_string(), next_src);
        self.editor
            .update(cx, |editor, cx| editor.rewrite_image_sources(&map, cx));
        // Resize is a structural commit — persist immediately (EXP-256).
        self.save_now(window, cx);
    }

    fn delete_image(&mut self, src: &str, window: &mut Window, cx: &mut Context<Self>) {
        let markdown = self.markdown(cx);
        let occurrences = crate::attachments_row::extract_image_occurrences(&markdown);
        let Some(index) = occurrences
            .iter()
            .position(|occurrence| occurrence.url == src)
        else {
            return;
        };
        let next = crate::attachments_row::remove_image_occurrence(&markdown, index);
        self.editor
            .update(cx, |editor, cx| editor.replace_markdown(next, cx));
        self.sync_images(cx);
        self.save_now(window, cx);
    }

    // -- toolbar link + image ------------------------------------------------

    /// Web parity (`LinkControl.open`): the field opens prefilled with the
    /// link already under the caret, so retargeting is an edit, not a retype.
    pub(super) fn open_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existing = self.editor.read(cx).format_state(window, cx).link;
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("https://…");
            if let Some(existing) = existing {
                state.set_value(existing, window, cx);
            }
            state
        });
        let subscription = cx.subscribe_in(&input, window, |this, _input, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_link(window, cx);
            }
        });
        input.focus_handle(cx).focus(window, cx);
        self.link_input = Some((input, subscription));
        cx.notify();
    }

    pub(super) fn close_link_editor(&mut self, cx: &mut Context<Self>) {
        self.link_input = None;
        cx.notify();
    }

    /// Web parity (`LinkControl.apply`): an empty URL clears the link instead
    /// of doing nothing.
    pub(super) fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((input, _)) = self.link_input.take() else {
            return;
        };
        let target = input.read(cx).value().trim().to_string();
        let target = (!target.is_empty()).then_some(target);
        self.editor.update(cx, |editor, cx| {
            editor.apply_format(FormatCommand::Link(target), window, cx);
        });
        cx.notify();
    }

    /// Web parity (`LinkControl.remove`).
    pub(super) fn remove_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.link_input = None;
        self.editor.update(cx, |editor, cx| {
            editor.apply_format(FormatCommand::Link(None), window, cx);
        });
        cx.notify();
    }

    /// EXP-335: wire the toolbar's attach button to the host's Files flow.
    pub fn set_attach_handler(&mut self, handler: OnAttachFiles) {
        self.on_attach_files = Some(handler);
    }

    /// EXP-335: does the toolbar render the attach button?
    pub(super) fn has_attach_handler(&self) -> bool {
        self.on_attach_files.is_some()
    }

    /// Toolbar "Attach file": any file type. Inline-image picks embed at the
    /// caret through the editor's own image pipeline (identical to the image
    /// button); everything else routes to the host's attach hook.
    pub(super) fn pick_attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_attach_files.is_none() {
            return;
        }
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let (images, others): (Vec<_>, Vec<_>) = paths.into_iter().partition(|path| {
                    crate::markdown::image_paste::is_inline_image_path(path)
                });
                for path in images {
                    this.editor.update(cx, |editor, cx| {
                        editor.insert_image_path(path, window, cx);
                    });
                }
                if !others.is_empty() {
                    if let Some(on_attach) = this.on_attach_files.clone() {
                        on_attach(others, window, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Toolbar "Insert image": pick files and feed them through the SAME
    /// materialize path a paste takes, so staging/upload behaves identically.
    pub(super) fn pick_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.editor.update(cx, |editor, cx| {
                        editor.insert_image_path(path, window, cx);
                    });
                }
            })
            .ok();
        })
        .detach();
    }

    // -- autocomplete -------------------------------------------------------

    fn refresh_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let had = self.completion.is_some();
        self.completion = None;
        if let Some(source) = self.completion_source.clone() {
            if let Some(text) = self.editor.read(cx).focused_text_before_caret(window, cx) {
                if let Some(token) = detect_trigger(&text, text.len()) {
                    let items = source.query(token.trigger, &token.query, cx);
                    if !items.is_empty() {
                        self.completion = Some(ActiveCompletion {
                            token_len: text.len() - token.start,
                            items,
                            selected: 0,
                        });
                    }
                }
            }
        }
        if had || self.completion.is_some() {
            cx.notify();
        }
    }

    fn accept_completion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        let Some(item) = completion.items.get(index) else {
            return;
        };
        // Insert the canonical interchange form plus the same trailing space
        // the block editor adds.
        let replacement = format!("{} ", item.insert);
        self.editor.update(cx, |editor, cx| {
            editor.replace_text_before_caret(completion.token_len, &replacement, window, cx);
        });
        cx.notify();
    }

    fn move_completion(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(completion) = self.completion.as_mut() {
            let len = completion.items.len() as isize;
            if len > 0 {
                completion.selected =
                    (completion.selected as isize + delta).rem_euclid(len) as usize;
                cx.notify();
            }
        }
    }

    /// EXP-307: capture-phase ACTION handlers while the popup is open. gpui
    /// dispatches matched key BINDINGS before any key-down listener runs, and
    /// the vendored editor binds enter/up/down/tab/escape in its own key
    /// context — so while a block is focused those keys arrive here as the
    /// editor's actions (`Newline`, `FocusPrev`, …), never as raw key events.
    /// This wrapper is the focused block's ancestor, so its capture handlers
    /// run before the editor's own and can claim the keystroke for the
    /// completion popup (the block-editor comment composer does the same with
    /// gpui-component's input actions).
    fn on_editor_newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.link_input.is_some() {
            self.apply_link(window, cx);
            cx.stop_propagation();
            return;
        }
        if let Some(selected) = self.completion.as_ref().map(|completion| completion.selected) {
            self.accept_completion(selected, window, cx);
            cx.stop_propagation();
        }
    }

    fn on_editor_indent(&mut self, _: &IndentBlock, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected) = self.completion.as_ref().map(|completion| completion.selected) {
            self.accept_completion(selected, window, cx);
            cx.stop_propagation();
        }
    }

    fn on_editor_focus_prev(&mut self, _: &FocusPrev, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_editor_focus_next(&mut self, _: &FocusNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(1, cx);
            cx.stop_propagation();
        }
    }

    fn on_editor_dismiss(
        &mut self,
        _: &DismissTransientUi,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_menu.is_some() {
            self.image_menu = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if self.link_input.is_some() {
            self.close_link_editor(cx);
            cx.stop_propagation();
            return;
        }
        if self.completion.is_some() {
            self.completion = None;
            cx.notify();
            cx.stop_propagation();
        }
    }

    /// Capture-phase keys while the popup is open — the fallback for keys the
    /// vendored editor does NOT bind (when a binding matches, gpui dispatches
    /// the action instead and the handlers above run).
    fn on_key_down_capture(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_menu.is_some() && event.keystroke.key.as_str() == "escape" {
            self.image_menu = None;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        // Web parity: Enter commits the link field, Escape abandons it.
        if self.link_input.is_some() {
            match event.keystroke.key.as_str() {
                "enter" => {
                    self.apply_link(window, cx);
                    cx.stop_propagation();
                    return;
                }
                "escape" => {
                    self.close_link_editor(cx);
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }
        if self.completion.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "up" => {
                self.move_completion(-1, cx);
                cx.stop_propagation();
            }
            "down" => {
                self.move_completion(1, cx);
                cx.stop_propagation();
            }
            "enter" | "tab" => {
                let selected = self
                    .completion
                    .as_ref()
                    .map(|completion| completion.selected)
                    .unwrap_or(0);
                self.accept_completion(selected, window, cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.completion = None;
                cx.notify();
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn render_completion(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let completion = self.completion.as_ref()?;
        let caret = self.editor.read(cx).caret_viewport_bounds(window, cx)?;
        let theme = cx.theme();
        let selected = completion.selected;
        let list = div()
            .flex()
            .flex_col()
            .min_w(px(260.))
            .max_w(px(380.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_md()
            .children(completion.items.iter().enumerate().map(|(index, item)| {
                let is_selected = index == selected;
                div()
                    .id(gpui::ElementId::from(("wysiwyg-completion-item", index)))
                    .w_full()
                    .flex()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(theme.accent))
                    .hover(|el| el.bg(theme.accent))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            this.accept_completion(index, window, cx);
                        }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .child(SharedString::from(item.label.to_string())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .overflow_hidden()
                            .child(SharedString::from(item.detail.to_string())),
                    )
            }));
        Some(
            deferred(
                anchored()
                    .position(point(
                        caret.origin.x,
                        caret.origin.y + caret.size.height + px(4.),
                    ))
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(1),
        )
    }

    // -- image context menu -------------------------------------------------

    fn render_image_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.image_menu.as_ref()?;
        let src = menu.src.clone();
        let key = images::cache_key(&src).to_string();
        let own_attachment = key.starts_with("/api/attachments/");
        let theme = cx.theme();

        // EXP-421: parity with the web image menu — icons per row and a
        // destructive "Remove from description" (Copy link is gone on both).
        let item = |id: &'static str,
                    label: &'static str,
                    icon: crate::icons::ExpIcon,
                    destructive: bool| {
            let (icon_color, label_color) = if destructive {
                (theme.danger, theme.danger)
            } else {
                (theme.muted_foreground, theme.popover_foreground)
            };
            h_flex()
                .id(id)
                .px_3()
                .py_1()
                .gap_2()
                .text_sm()
                .rounded_sm()
                .cursor_pointer()
                .hover(|style| style.bg(theme.muted))
                .child(Icon::from(icon).size_4().text_color(icon_color))
                .child(
                    div()
                        .text_color(label_color)
                        .child(SharedString::from(label)),
                )
        };

        let images_entity = self.images.clone();
        let mut list = div()
            .flex()
            .flex_col()
            .py_1()
            .min_w(px(160.))
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .child(
                item(
                    "wysiwyg-image-view",
                    "View image",
                    crate::icons::registry::UI_WATCH,
                    false,
                )
                .on_mouse_down(MouseButton::Left, {
                    let key = key.clone();
                    let images = images_entity.clone();
                    cx.listener(move |this, _event, window, cx| {
                        this.image_menu = None;
                        crate::image_preview::open_image_preview(
                            key.clone(),
                            String::new(),
                            Some(images.clone()),
                            window,
                            cx,
                        );
                        cx.notify();
                    })
                }),
            );
        if own_attachment {
            list = list.child(
                item(
                    "wysiwyg-image-download",
                    "Download",
                    crate::icons::registry::UI_DOWNLOAD,
                    false,
                )
                .on_mouse_down(MouseButton::Left, {
                    let key = key.clone();
                    let images = images_entity.clone();
                    cx.listener(move |this, _event, window, cx| {
                        this.image_menu = None;
                        download_image(key.clone(), &images, window, cx);
                        cx.notify();
                    })
                }),
            );
            // Linux clipboard backends are text-only at pin 1d217ee (Wayland
            // offers only TEXT_MIME_TYPES; X11 routes write_to_clipboard
            // through set_text — its set_image exists but is unwired), so
            // Copy image ships on macOS/Windows only.
            if cfg!(any(target_os = "macos", target_os = "windows")) {
                list = list.child(
                    item(
                        "wysiwyg-image-copy",
                        "Copy image",
                        crate::icons::registry::UI_COPY,
                        false,
                    )
                    .on_mouse_down(MouseButton::Left, {
                        let key = key.clone();
                        let images = images_entity.clone();
                        cx.listener(move |this, _event, _window, cx| {
                            this.image_menu = None;
                            if let Some(image) = images.read(cx).ready_image(&key) {
                                cx.write_to_clipboard(ClipboardItem::new_image(&image));
                            }
                            cx.notify();
                        })
                    }),
                );
            }
        }
        list = list
            .child(div().my_1().h(px(1.)).bg(theme.border))
            .child(
                item(
                    "wysiwyg-image-delete",
                    "Remove from description",
                    crate::icons::registry::UI_DELETE,
                    true,
                )
                .on_mouse_down(MouseButton::Left, {
                    let src = src.clone();
                    cx.listener(move |this, _event, window, cx| {
                        this.image_menu = None;
                        this.delete_image(&src, window, cx);
                        cx.notify();
                    })
                }),
            );

        Some(
            deferred(
                anchored()
                    .position(point(menu.position.x, menu.position.y))
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(2),
        )
    }

    /// Click-anywhere-to-dismiss layer under the open image menu. A plain
    /// `on_mouse_down` on the wrapper cannot do this job: the editor's blocks
    /// stop propagation on mouse-down, and clicks outside the description slot
    /// never reach the wrapper at all.
    fn render_image_menu_backdrop(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.image_menu.as_ref()?;
        let viewport = window.viewport_size();
        let dismiss = |this: &mut Self, cx: &mut Context<Self>| {
            this.image_menu = None;
            cx.notify();
        };
        Some(
            deferred(
                anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .w(viewport.width)
                        .h(viewport.height)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| dismiss(this, cx)),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _event, _window, cx| dismiss(this, cx)),
                        ),
                ),
            )
            .with_priority(1),
        )
    }
}

impl Focusable for WysiwygDescription {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WysiwygDescription {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let completion_popup = self.render_completion(window, cx);
        let menu_backdrop = self.render_image_menu_backdrop(window, cx);
        let toolbar = if self.external_toolbar {
            None
        } else {
            self.render_toolbar(window, cx)
        };
        // EXP-285: a flex column that STRETCHES when the host slot gives it
        // height, with a trailing filler strip below the (content-hugging
        // embedded) editor. Clicking the filler focuses the document end —
        // the textarea affordance; without it the leftover slot area was a
        // dead zone where clicks (and therefore typing) went nowhere. The
        // filler sits below the editor, so it can never shadow the toolbar
        // or the editor's own gap-click handling.
        v_flex()
            .w_full()
            .flex_1()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_key_down_capture))
            .capture_action(cx.listener(Self::on_editor_newline))
            .capture_action(cx.listener(Self::on_editor_indent))
            .capture_action(cx.listener(Self::on_editor_focus_prev))
            .capture_action(cx.listener(Self::on_editor_focus_next))
            .capture_action(cx.listener(Self::on_editor_dismiss))
            .children(toolbar)
            .child(self.editor.clone())
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h(px(24.))
                    // EXP-421: the filler is a text-entry surface (clicking
                    // focuses the document end), so it shows the I-beam like
                    // the editor itself.
                    .cursor(gpui::CursorStyle::IBeam)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.focus_end(window, cx)),
                    ),
            )
            .children(completion_popup)
            .children(menu_backdrop)
            .children(self.render_image_menu(cx))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gpui::TestAppContext;

    use super::*;
    use crate::markdown::autocomplete::{CompletionItem, CompletionTrigger};
    use crate::markdown::{AttachmentTransport, UploadedImage};

    /// A valid 1×1 PNG (imagesize can probe its header).
    const PNG_1X1: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// Fetch-only transport: fails the first `failures` fetches, then serves
    /// [`PNG_1X1`]. Uploads are unreachable in these tests.
    struct StubTransport {
        failures_left: AtomicUsize,
        fetches: AtomicUsize,
    }

    impl StubTransport {
        fn new(failures: usize) -> Arc<Self> {
            Arc::new(Self {
                failures_left: AtomicUsize::new(failures),
                fetches: AtomicUsize::new(0),
            })
        }
    }

    impl AttachmentTransport for StubTransport {
        fn upload(
            &self,
            _issue_id: &str,
            _filename: &str,
            _content_type: &str,
            _bytes: &[u8],
        ) -> anyhow::Result<UploadedImage> {
            unreachable!("fetch-only stub")
        }

        fn upload_file(
            &self,
            _issue_id: &str,
            _filename: &str,
            _content_type: &str,
            _bytes: &[u8],
        ) -> anyhow::Result<UploadedImage> {
            unreachable!("fetch-only stub")
        }

        fn fetch(&self, _url: &str) -> anyhow::Result<Vec<u8>> {
            self.fetches.fetch_add(1, Ordering::SeqCst);
            if self
                .failures_left
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok()
            {
                anyhow::bail!("stubbed network failure");
            }
            Ok(PNG_1X1.to_vec())
        }
    }

    fn resolution_of(view: &WysiwygDescription, key: &str) -> Option<ImageSourceResolution> {
        view.shared.resolutions.lock().unwrap().get(key).cloned()
    }

    // EXP-354: another machine adds an image to the description while the
    // issue is open — the remote echo's `set_markdown` must fetch and render
    // the new image without leaving the issue.
    #[gpui::test]
    async fn a_remote_echo_with_a_new_image_resolves_live(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(None, None, "Add description...", "hello", None, window, cx)
        });
        let transport = StubTransport::new(0);
        cx.update(|_window, cx| {
            view.update(cx, |view, cx| {
                view.images.update(cx, |cache, _| {
                    cache.set_transport(Some(transport.clone() as Arc<dyn AttachmentTransport>));
                });
            });
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown("hello\n\n![shot](/api/attachments/abc)", window, cx);
            });
        });
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(
                matches!(
                    resolution_of(view, "/api/attachments/abc"),
                    Some(ImageSourceResolution::Decoded(_))
                ),
                "the echoed image resolved without reopening the issue"
            );
        });
        assert_eq!(transport.fetches.load(Ordering::SeqCst), 1);
    }

    // EXP-354: a transient fetch failure (auth blip, brief offline) must not
    // brick the image until the issue is reopened. The block editor retries
    // failed slots from its render loop; the WYSIWYG surface has no such
    // driver, so it schedules its own retry.
    #[gpui::test]
    async fn a_failed_fetch_retries_and_recovers(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(None, None, "Add description...", "hello", None, window, cx)
        });
        let transport = StubTransport::new(1);
        cx.update(|_window, cx| {
            view.update(cx, |view, cx| {
                view.images.update(cx, |cache, _| {
                    cache.set_transport(Some(transport.clone() as Arc<dyn AttachmentTransport>));
                });
            });
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown("hello\n\n![shot](/api/attachments/abc)", window, cx);
            });
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                matches!(
                    resolution_of(view, "/api/attachments/abc"),
                    Some(ImageSourceResolution::Failed)
                ),
                "the first fetch failed"
            );
        });

        cx.executor()
            .advance_clock(crate::markdown::IMAGE_RETRY_AFTER + std::time::Duration::from_secs(1));
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(
                matches!(
                    resolution_of(view, "/api/attachments/abc"),
                    Some(ImageSourceResolution::Decoded(_))
                ),
                "the retry recovered the image"
            );
        });
        assert_eq!(transport.fetches.load(Ordering::SeqCst), 2);
    }

    /// One fixed `#EXP-42` candidate for every query — enough to open the
    /// popup without a synced Store behind it.
    struct StubCompletionSource;

    impl CompletionSource for StubCompletionSource {
        fn query(
            &self,
            trigger: CompletionTrigger,
            _query: &str,
            _cx: &gpui::App,
        ) -> Vec<CompletionItem> {
            vec![CompletionItem {
                trigger,
                insert: "#EXP-42".to_string(),
                label: "EXP-42".into(),
                detail: "Editor chip improvements".into(),
            }]
        }
    }

    // EXP-307: enter/tab must ACCEPT the open `#` completion instead of
    // reaching the vendored editor. gpui dispatches matched key bindings
    // before any key-down listener, and the editor binds enter/tab in its own
    // key context — so the old `capture_key_down`-only wiring never saw the
    // keystroke and enter split the block under the popup. The capture-phase
    // ACTION handlers claim it first.
    #[gpui::test]
    async fn enter_and_tab_accept_the_open_completion(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            cx.bind_keys(gpui_markdown_editor::default_key_bindings());
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view =
                WysiwygDescription::new(None, None, "Add description...", "", None, window, cx);
            view.completion_source = Some(Rc::new(StubCompletionSource));
            view
        });

        let draw_frames = |cx: &mut gpui::VisualTestContext| {
            for _ in 0..2 {
                cx.update(|window, cx| window.draw(cx).clear());
                cx.run_until_parked();
            }
        };

        // Focus applies during the next render (pending-focus handshake).
        cx.update(|window, cx| view.update(cx, |view, cx| view.focus(window, cx)));
        draw_frames(cx);

        // Type the pending token through the normal typed-edit path; the
        // Changed event runs refresh_completion against the stub source.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.editor.update(cx, |editor, cx| {
                    editor.replace_text_before_caret(0, "#EX", window, cx);
                });
            });
        });
        draw_frames(cx);
        view.read_with(cx, |view, _| {
            assert!(view.completion.is_some(), "typing `#EX` opens the popup");
        });

        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, cx| {
            assert!(view.completion.is_none(), "enter closed the popup");
            // Acceptance inserts the token plus a trailing space (block-editor
            // parity).
            assert_eq!(
                view.markdown(cx).trim_end(),
                "#EXP-42",
                "enter accepted the item"
            );
        });

        // Same again via tab.
        draw_frames(cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.editor.update(cx, |editor, cx| {
                    editor.replace_text_before_caret(0, "#EX", window, cx);
                });
            });
        });
        draw_frames(cx);
        view.read_with(cx, |view, _| {
            assert!(view.completion.is_some(), "typing `#EX` re-opens the popup");
        });
        cx.simulate_keystrokes("tab");
        view.read_with(cx, |view, cx| {
            assert!(view.completion.is_none(), "tab closed the popup");
            assert_eq!(
                view.markdown(cx).trim_end(),
                "#EXP-42 #EXP-42",
                "tab accepted the item in place of the pending token"
            );
        });
    }

    // Construction + round-trip through the full wrapper: theme bridge,
    // embedded vendored editor, image/reference seams — all with no
    // signed-in session (transport resolves to None).
    #[gpui::test]
    async fn wrapper_round_trips_markdown(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                "# Title\n\nSome **bold** text",
                None,
                window,
                cx,
            )
        });
        view.read_with(cx, |view, cx| {
            assert_eq!(view.markdown(cx), "# Title\n\nSome **bold** text");
            assert!(view.staged_images(cx).is_empty());
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown("replaced", window, cx);
            });
        });
        view.read_with(cx, |view, cx| {
            assert_eq!(view.markdown(cx), "replaced");
        });
    }

    // EXP-271: descriptions saved by the web carried the image WELDED onto
    // the next paragraph (`![](/api/attachments/x)text` — see
    // `markdown::image_blocks`). The vendored engine renders an image only
    // when its block holds nothing else, so the whole line showed up as
    // literal markdown text. Both load paths lift the image into its own
    // block, and doing so must not dirty the editor.
    #[gpui::test]
    async fn glued_web_images_load_as_image_blocks(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                "![](/api/attachments/abc)on mobile steering the plan.",
                None,
                window,
                cx,
            )
        });
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.markdown(cx),
                "![](/api/attachments/abc)\n\non mobile steering the plan."
            );
            assert!(!view.is_dirty(cx));
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown("before ![alt](/api/attachments/xyz) after", window, cx);
            });
        });
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.markdown(cx),
                "before\n\n![alt](/api/attachments/xyz)\n\nafter"
            );
            assert!(!view.is_dirty(cx));
        });
    }

    // EXP-271: `&nbsp;` is the interchange form of an intentional blank line
    // (web `markdown-paragraph.ts`), and the web's serializer also emits
    // `&gt;`/`&amp;` for literal `>`/`&`. The vendored engine has no notion of
    // HTML entities, so all of it rendered as raw source. Loading decodes
    // them — and the decoded blank-line paragraph must SURVIVE the engine's
    // round trip, or a save would silently drop the user's blank line.
    #[gpui::test]
    async fn html_entities_load_decoded_and_the_blank_line_survives(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                "close the laptop =&gt; no internet.\n\n&nbsp;\n\nTom &amp; Jerry",
                None,
                window,
                cx,
            )
        });
        view.read_with(cx, |view, cx| {
            // `&gt;`/`&amp;` stay decoded on the way back out — the block
            // editor persists them the same way — but the blank-line
            // paragraph the engine trims comes back as its stored marker
            // instead of the bare blank lines that parse as nothing.
            assert_eq!(
                view.markdown(cx),
                "close the laptop => no internet.\n\n&nbsp;\n\nTom & Jerry"
            );
            assert!(!view.is_dirty(cx));
        });
    }

    // EXP-271: the blank-line marker has to survive a real EDIT, not just a
    // load — a save that dropped it would silently delete the user's blank
    // line on every other client.
    #[gpui::test]
    async fn an_edited_save_keeps_the_blank_line_marker(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let saved: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
        let on_save: OnSave = Rc::new({
            let saved = saved.clone();
            move |markdown, _window, _cx| saved.borrow_mut().push(markdown)
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                "first\n\n&nbsp;\n\n![shot](/api/attachments/old)",
                Some(on_save),
                window,
                cx,
            )
        });

        // A structural edit (the image-upload rewrite) — any real mutation.
        cx.update(|_window, cx| {
            view.update(cx, |view, cx| {
                let mut map = HashMap::new();
                map.insert(
                    "/api/attachments/old".to_string(),
                    "/api/attachments/new".to_string(),
                );
                view.editor
                    .update(cx, |editor, cx| editor.rewrite_image_sources(&map, cx));
            });
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.save_now(window, cx));
        });
        assert_eq!(
            saved.borrow().as_slice(),
            ["first\n\n&nbsp;\n\n![shot](/api/attachments/new)".to_string()]
        );
    }

    // EXP-271, the engine-level counterfactual behind the fix above: the
    // vendored engine builds an image runtime (its one render path for a
    // picture) ONLY for a block that holds nothing but the image, so it
    // consults the host resolver for the lifted form and not for the glued
    // one. Without the lift there is no runtime, hence no picture.
    #[gpui::test]
    async fn the_engine_only_resolves_images_that_own_their_block(cx: &mut TestAppContext) {
        #[derive(Default)]
        struct Recorder(std::sync::Mutex<Vec<String>>);
        impl gpui_markdown_editor::ImageSourceResolver for Recorder {
            fn resolve(&self, src: &str) -> Option<ImageSourceResolution> {
                self.0.lock().unwrap().push(src.to_string());
                Some(ImageSourceResolution::Pending)
            }
        }

        let glued = "![](/api/attachments/abc)on mobile steering the plan.";
        let resolved_srcs = |markdown: String, cx: &mut TestAppContext| {
            let recorder = Arc::new(Recorder::default());
            let environment = gpui_markdown_editor::MarkdownEditorEnvironment {
                image_source_resolver: Some(recorder.clone()),
                ..Default::default()
            };
            cx.new(|cx| VendoredEditor::with_environment(markdown, environment, cx));
            // The engine may rebuild a runtime more than once per load; only
            // the distinct set of consulted sources is meaningful.
            let mut srcs = recorder.0.lock().unwrap().clone();
            srcs.sort();
            srcs.dedup();
            srcs
        };

        assert!(resolved_srcs(glued.to_string(), cx).is_empty());
        assert_eq!(
            resolved_srcs(normalize_for_wysiwyg(glued), cx),
            ["/api/attachments/abc"]
        );
    }

    // EXP-261: merely OPENING an issue must never dirty the editor, even
    // when the vendored serializer normalizes the loaded markdown into
    // different bytes (`_i_`→`*i*`, `1)`→`1.`, CRLF→LF, …) — the byte diff
    // used to trigger a phantom save-on-blur that rewrote other clients'
    // content.
    #[gpui::test]
    async fn view_only_open_is_never_dirty(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let loaded = "Setext title\n====\n\n_italic_ and __bold__\r\n\n1) one\n2) two";
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(None, None, "Add description...", loaded, None, window, cx)
        });
        view.read_with(cx, |view, cx| {
            // The engine really does normalize this input…
            assert_ne!(view.markdown(cx), loaded);
            // …but with no user edit the editor must stay clean.
            assert!(!view.is_dirty(cx));
        });

        // Programmatic loads (remote Electric echoes) stay clean too.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.set_markdown("- [X] task", window, cx);
            });
        });
        view.read_with(cx, |view, cx| {
            assert!(!view.is_dirty(cx));
        });
    }

    // EXP-261: real document mutations (here: the image-upload `draft://`
    // rewrite, which bumps the vendored revision like any edit) dirty the
    // editor, and `save_now` marks it clean again after persisting.
    #[gpui::test]
    async fn edits_dirty_and_save_cleans(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let saved: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
        let on_save: OnSave = Rc::new({
            let saved = saved.clone();
            move |markdown, _window, _cx| saved.borrow_mut().push(markdown)
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                "![shot](/api/attachments/old)",
                Some(on_save),
                window,
                cx,
            )
        });
        view.read_with(cx, |view, cx| assert!(!view.is_dirty(cx)));

        cx.update(|_window, cx| {
            view.update(cx, |view, cx| {
                let mut map = HashMap::new();
                map.insert(
                    "/api/attachments/old".to_string(),
                    "/api/attachments/new".to_string(),
                );
                view.editor
                    .update(cx, |editor, cx| editor.rewrite_image_sources(&map, cx));
            });
        });
        view.read_with(cx, |view, cx| assert!(view.is_dirty(cx)));

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.save_now(window, cx));
        });
        view.read_with(cx, |view, cx| assert!(!view.is_dirty(cx)));
        assert_eq!(
            saved.borrow().as_slice(),
            ["![shot](/api/attachments/new)".to_string()]
        );
    }

    // EXP-261 regression (BUG 2 + BUG 3): a draft-racing save strips drafts
    // STRUCTURALLY — inline occurrences (list item, mixed line) go too, a
    // real image sharing a line with a draft survives, and the save never
    // comrak-canonicalizes, so a table elsewhere in the document reaches the
    // save hook byte-identically instead of being flattened.
    #[gpui::test]
    async fn save_strips_inline_drafts_and_keeps_tables_byte_identical(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let loaded = "Intro\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n- ![img](draft://li-1) item\n\n![keep](/api/attachments/xyz) ![lose](draft://mix-1)\n\n![gone](draft://para-1)";
        let saved: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
        let on_save: OnSave = Rc::new({
            let saved = saved.clone();
            move |markdown, _window, _cx| saved.borrow_mut().push(markdown)
        });
        let (view, cx) = cx.add_window_view(|window, cx| {
            WysiwygDescription::new(
                None,
                None,
                "Add description...",
                loaded,
                Some(on_save),
                window,
                cx,
            )
        });
        // The vendored engine round-trips all of this (tables included) —
        // the raw buffer still holds every draft. The one difference from
        // `loaded` is EXP-271: the two images sharing the mixed line load as
        // their own blocks (the list item's inline image is left alone).
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.markdown(cx),
                loaded.replace(
                    "![keep](/api/attachments/xyz) ![lose](draft://mix-1)",
                    "![keep](/api/attachments/xyz)\n\n![lose](draft://mix-1)",
                )
            )
        });

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.save_now(window, cx));
        });
        assert_eq!(
            saved.borrow().as_slice(),
            ["Intro\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n-  item\n\n![keep](/api/attachments/xyz)"
                .to_string()]
        );
        // The document itself keeps the drafts (the uploads may still land).
        view.read_with(cx, |view, cx| {
            assert!(view.markdown(cx).contains("draft://"));
        });
    }
}
