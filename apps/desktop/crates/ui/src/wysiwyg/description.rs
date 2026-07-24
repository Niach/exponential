//! EXP-261: `WysiwygDescription` — the host view wrapping the vendored
//! WYSIWYG editor entity for the issue-description surfaces (issue detail +
//! create-issue dialog). Owns focus tracking (save-on-blur), theme
//! observation, and — in later phases — the image pipeline, pills, and
//! autocomplete state.

use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, Context, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window,
};
use gpui_markdown_editor::{
    MarkdownEditor as VendoredEditor, MarkdownEditorEvent, MarkdownEditorMode,
    MarkdownEditorOptions,
};

use crate::markdown::StagedImage;

/// Save hook: current markdown at save time (same shape as
/// [`crate::issue_detail::OnSaveDescription`]).
pub(crate) type OnSave = Rc<dyn Fn(String, &mut Window, &mut App)>;

pub struct WysiwygDescription {
    editor: Entity<VendoredEditor>,
    focus_handle: FocusHandle,
    placeholder: SharedString,
    /// Pills + autocomplete scope (the issue's team).
    team_id: Option<String>,
    /// `Some(issue_id)` = detail mode (immediate upload on paste);
    /// `None` = create-dialog mode (stage as `draft://`, resolve at submit).
    upload_issue: Option<String>,
    /// Images staged in create-dialog mode (`draft://` URLs; resolved at
    /// submit). Detail mode uploads immediately instead.
    staged: Vec<StagedImage>,
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
        let editor = cx.new(|cx| {
            let mut environment =
                gpui_markdown_editor::MarkdownEditorEnvironment::default();
            environment.theme = super::editor_theme_with_placeholder(cx, placeholder.as_ref());
            environment.show_source_line_numbers = false;
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

        // Bubble vendored events: change notifications re-render observers;
        // link activations open externally (issue-ref pills come in P6).
        subscriptions.push(cx.subscribe(
            &editor,
            |_this, _editor, event: &MarkdownEditorEvent, cx| match event {
                // Hosts react via `cx.observe` (the dialog's attachment
                // rail re-renders on every notify).
                MarkdownEditorEvent::Changed { .. } => cx.notify(),
                MarkdownEditorEvent::OpenLinkRequested(request) => {
                    if let Err(error) = api::opener::open_in_browser(&request.open_target) {
                        log::warn!("open link failed: {error}");
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
        if let Some(on_save) = on_save {
            let this = cx.entity().downgrade();
            subscriptions.push(window.on_focus_out(&focus_handle, cx, move |_event, window, cx| {
                if let Some(this) = this.upgrade() {
                    let markdown = this.read(cx).markdown(cx);
                    on_save(markdown, window, cx);
                }
            }));
        }

        // Presentation-only theme refresh on light/dark switches.
        subscriptions.push(cx.observe_global::<gpui_component::Theme>(|this, cx| {
            let theme = super::editor_theme_with_placeholder(cx, this.placeholder.as_ref());
            this.editor.update(cx, |editor, cx| editor.set_theme(theme, cx));
        }));

        Self {
            editor,
            focus_handle,
            placeholder,
            team_id,
            upload_issue,
            staged: Vec::new(),
            _subscriptions: subscriptions,
        }
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
        cx.notify();
    }

    /// Images staged in create-dialog mode (P5 fills this).
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
}

impl Focusable for WysiwygDescription {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WysiwygDescription {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .track_focus(&self.focus_handle)
            .child(self.editor.clone())
    }
}
