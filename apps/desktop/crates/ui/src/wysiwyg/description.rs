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
use gpui_component::ActiveTheme as _;
use gpui_markdown_editor::{
    ImageSourceResolution, MarkdownEditor as VendoredEditor, MarkdownEditorEvent,
    MarkdownEditorMode, MarkdownEditorOptions, ReferenceKind,
};

use super::images::{
    self, SharedImageState, WysiwygImageResolver, WysiwygPasteHandler,
};
use super::refs::{refresh_ref_state, SharedRefState, WysiwygReferenceDecorator};
use crate::markdown::image_paste::{strip_draft_images, DRAFT_SCHEME};
use crate::markdown::{download_image, ImageCache, ImageSlot, StagedImage};
use crate::queries;

/// Save hook: current markdown at save time (same shape as
/// [`crate::issue_detail::OnSaveDescription`]).
pub(crate) type OnSave = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// An open image context menu (host-rendered — the vendored editor only
/// reports the right-click via `ImageContextMenuRequested`).
struct ImageMenuState {
    src: String,
    position: Point<Pixels>,
}

pub struct WysiwygDescription {
    editor: Entity<VendoredEditor>,
    focus_handle: FocusHandle,
    placeholder: SharedString,
    /// Pills + autocomplete scope (the issue's team).
    team_id: Option<String>,
    /// `Some(issue_id)` = detail mode (immediate upload on paste);
    /// `None` = create-dialog mode (stage as `draft://`, resolve at submit).
    upload_issue: Option<String>,
    on_save: Option<OnSave>,
    /// Authenticated fetch/decode cache (same type the block editor uses).
    images: Entity<ImageCache>,
    /// State shared with the vendored environment's paste handler + resolver.
    shared: Arc<SharedImageState>,
    /// Member/issue snapshot behind the reference-pill decorator.
    refs: Arc<SharedRefState>,
    /// Images staged in create-dialog mode (`draft://` URLs; resolved at
    /// submit by the dialog's existing upload flow).
    staged: Vec<StagedImage>,
    image_menu: Option<ImageMenuState>,
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
            if team_id.is_some() {
                environment.reference_decorator = Some(Arc::new(WysiwygReferenceDecorator {
                    state: refs.clone(),
                }));
            }
            VendoredEditor::new(
                initial_markdown,
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
                MarkdownEditorEvent::ModeChanged { .. }
                | MarkdownEditorEvent::SelectionChanged(_) => {}
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
                        this.update(cx, |this, cx| this.save_now(window, cx));
                    }
                },
            ));
        }

        // Loading → Ready repaints: the cache notifies when a fetch lands.
        subscriptions.push(cx.observe(&images, |this, _images, cx| {
            this.sync_images(cx);
        }));

        // Presentation-only theme refresh on light/dark switches.
        subscriptions.push(cx.observe_global::<gpui_component::Theme>(|this, cx| {
            let theme = super::editor_theme_with_placeholder(cx, this.placeholder.as_ref());
            this.editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }));

        let mut this = Self {
            editor,
            focus_handle,
            placeholder,
            team_id,
            upload_issue,
            on_save,
            images,
            shared,
            staged: Vec::new(),
            image_menu: None,
            refs,
            _subscriptions: subscriptions,
        };
        // Kick off fetches for images already present in the description and
        // seed the reference-pill snapshot.
        this.sync_images(cx);
        this.sync_refs(cx);
        this
    }

    /// Current canonical GFM. The vendored serializer IS the canonical form
    /// on this path — no comrak `canonicalize` pass (see wysiwyg_parity.rs).
    pub fn markdown(&self, cx: &App) -> String {
        self.editor.read(cx).markdown(cx)
    }

    /// Replace the whole buffer (remote echo / dialog reset). Resets undo
    /// history, matching the vendored `replace_markdown` semantics.
    pub fn set_markdown(&mut self, markdown: &str, _window: &mut Window, cx: &mut Context<Self>) {
        let markdown = markdown.to_string();
        self.editor
            .update(cx, |editor, cx| editor.replace_markdown(markdown, cx));
        // A full buffer replace discards any staged (unsubmitted) images.
        self.staged.clear();
        self.sync_images(cx);
        cx.notify();
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

    // -- save ---------------------------------------------------------------

    fn save_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(on_save) = self.on_save.clone() else {
            return;
        };
        let mut markdown = self.markdown(cx);
        // A `draft://` URL must never reach the server. In detail mode a
        // still-uploading paste is stripped from THIS save; the upload's own
        // completion save re-adds it with the real attachment URL.
        if markdown.contains(DRAFT_SCHEME) {
            markdown = strip_draft_images(&markdown);
        }
        on_save(markdown, window, cx);
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
                            if let Some(existing) =
                                resolutions.get(&staged.draft_url).cloned()
                            {
                                resolutions.insert(real_key.clone(), existing);
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
                    let _ = this.update_in(cx, |this, _window, cx| {
                        if let Ok(mut resolutions) = this.shared.resolutions.lock() {
                            resolutions.insert(
                                staged.draft_url.clone(),
                                ImageSourceResolution::Failed,
                            );
                        }
                        this.refresh_editor_environment(cx);
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
        for occurrence in &occurrences {
            let src = occurrence.url.as_str();
            if !images::is_hosted_src(src) {
                continue;
            }
            let key = images::cache_key(src).to_string();
            if next.contains_key(&key) {
                continue;
            }
            if src.starts_with(DRAFT_SCHEME) {
                // Draft bytes were inserted by the paste handler; keep
                // whatever state they carry (Decoded, or Failed on upload
                // error).
                let existing = self
                    .shared
                    .resolutions
                    .lock()
                    .ok()
                    .and_then(|resolutions| resolutions.get(&key).cloned())
                    .unwrap_or(ImageSourceResolution::Failed);
                next.insert(key, existing);
            } else {
                let slot = self.images.update(cx, |cache, cx| cache.slot(&key, cx));
                let resolution = match slot {
                    ImageSlot::Ready(image) => ImageSourceResolution::Decoded(image),
                    ImageSlot::Loading => ImageSourceResolution::Pending,
                    ImageSlot::Failed(_) => ImageSourceResolution::Failed,
                };
                next.insert(key, resolution);
            }
        }

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
        if changed {
            self.refresh_editor_environment(cx);
        }
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
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next_src =
            crate::markdown::image_url::src_with_width(src, Some(width.max(1.0) as u32));
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

    // -- image context menu -------------------------------------------------

    fn render_image_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.image_menu.as_ref()?;
        let src = menu.src.clone();
        let key = images::cache_key(&src).to_string();
        let own_attachment = key.starts_with("/api/attachments/");
        let theme = cx.theme();

        let item = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .text_sm()
                .rounded_sm()
                .cursor_pointer()
                .hover(|style| style.bg(theme.muted))
                .child(SharedString::from(label))
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
                item("wysiwyg-image-view", "View image").on_mouse_down(MouseButton::Left, {
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
            list = list
                .child(
                    item("wysiwyg-image-download", "Download").on_mouse_down(MouseButton::Left, {
                        let key = key.clone();
                        let images = images_entity.clone();
                        cx.listener(move |this, _event, window, cx| {
                            this.image_menu = None;
                            download_image(key.clone(), &images, window, cx);
                            cx.notify();
                        })
                    }),
                );
            if cfg!(target_os = "macos") {
                list = list.child(item("wysiwyg-image-copy", "Copy image").on_mouse_down(
                    MouseButton::Left,
                    {
                        let key = key.clone();
                        let images = images_entity.clone();
                        cx.listener(move |this, _event, _window, cx| {
                            this.image_menu = None;
                            if let Some(image) = images.read(cx).ready_image(&key) {
                                cx.write_to_clipboard(ClipboardItem::new_image(&image));
                            }
                            cx.notify();
                        })
                    },
                ));
            }
            list = list.child(item("wysiwyg-image-copy-link", "Copy link").on_mouse_down(
                MouseButton::Left,
                {
                    let key = key.clone();
                    cx.listener(move |this, _event, _window, cx| {
                        this.image_menu = None;
                        if let Some(absolute) = queries::absolute_api_url(cx, &key) {
                            cx.write_to_clipboard(ClipboardItem::new_string(absolute));
                        }
                        cx.notify();
                    })
                },
            ));
        }
        list = list.child(item("wysiwyg-image-delete", "Delete").on_mouse_down(
            MouseButton::Left,
            {
                let src = src.clone();
                cx.listener(move |this, _event, window, cx| {
                    this.image_menu = None;
                    this.delete_image(&src, window, cx);
                    cx.notify();
                })
            },
        ));

        Some(
            deferred(
                anchored()
                    .position(point(menu.position.x, menu.position.y))
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_open = self.image_menu.is_some();
        div()
            .w_full()
            .track_focus(&self.focus_handle)
            // Any press outside the menu items dismisses the menu.
            .when(menu_open, |this| {
                this.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.image_menu = None;
                        cx.notify();
                    }),
                )
            })
            .child(self.editor.clone())
            .children(self.render_image_menu(cx))
    }
}
