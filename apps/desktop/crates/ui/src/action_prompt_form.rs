//! The Prompt part of a saved prompt's page (EXP-467 as the "Edit action"
//! dialog; SLOP-2 folded it into [`crate::action_view`]). Two columns: the
//! metadata left (icon + Name), the body right as a plain monospace
//! multiline editor, one batched `prompts.update` on Save. Writes are
//! owner-only (the server enforces it), so a NON-owner gets the same form
//! read-only, with no Save.
//!
//! NEW-2: prompts replaced actions — a name, an icon and a body, nothing
//! else (no description, inputs or composer hint). NEW-6 moves this into
//! Settings → Prompts.
//!
//! Synced rows carry no body, so it is fetched via `prompts.get` when the
//! form is built and the field stays disabled until it lands — a save can
//! never blank it. A duplicate-name CONFLICT (HTTP 409) renders inline under
//! the Name field, everything else beside Save.

use gpui::{
    div, px, AppContext as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::queries;

/// The metadata column's width — the old dialog's left column.
const META_COLUMN_W: f32 = 400.;

pub(crate) struct ActionPromptForm {
    action_id: String,
    team_id: String,
    name: gpui::Entity<InputState>,
    /// The curated registry glyph (`actionIconSchema` — the boards set).
    icon: String,
    /// The prompt — a plain multiline editor (web textarea parity).
    body: gpui::Entity<TextareaState>,
    /// `prompts.get` in flight — the prompt and Save stay disabled so a
    /// save can never blank the body.
    body_loading: bool,
    /// FEED-79: `prompts.get` failed. The load on open is quiet: the prompt
    /// field becomes its empty state + Retry and Save stays off, so the real
    /// body is never blanked. `body_retry_pressed` = the load in flight is
    /// the person's Retry press, whose failure toasts.
    body_failed: bool,
    body_retry_pressed: bool,
    submitting: bool,
    /// EXP-1259 D8: Save's pending face ("Saving").
    pending: crate::feedback::Pending,
    /// Duplicate-name CONFLICT — INVALID INPUT, so it stays under Name
    /// (FEED-79); every other failure is a toast.
    name_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

/// The prompt field when `prompts.get` failed (web `action-prompt-form.tsx`).
const PROMPT_UNAVAILABLE: &str = "Prompt unavailable";

impl ActionPromptForm {
    /// The form over one synced (non-builtin) action row.
    pub(crate) fn new(
        action: api::prompts::Prompt,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // EXP-694: the placeholders ARE the field titles — same three strings
        // on every client (web `action-editor-dialog.tsx`).
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        name.update(cx, |state, cx| {
            state.set_value(action.name.clone(), window, cx);
        });
        // "Prompt" once `prompts.get` lands (web parity); until then the
        // group shows the skeleton (D8), never a "Loading…" placeholder.
        // The prompt AUTO-GROWS: the page is the one scroller (SLOP-2), so
        // the field is as tall as its text, inside a sane band.
        let body = cx.new(|cx| crate::controls::web_textarea(12, 40, window, cx));

        // Enter submits from the one-line fields; in the prompt it inserts a
        // newline (hence no shell-level `on_enter`).
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        ));
        // Save-gating (empty body) follows the editor live. It is a
        // textarea, so Enter inserts a newline instead of submitting.
        subscriptions.push(cx.subscribe_in(
            &body,
            window,
            |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        ));

        let mut this = Self {
            action_id: action.id.clone(),
            team_id: action.team_id.clone(),
            name,
            icon: action
                .icon
                .clone()
                .unwrap_or_else(|| domain::contract::BOARD_ICON_VALUES[0].to_string()),
            body,
            body_loading: true,
            body_failed: false,
            body_retry_pressed: false,
            submitting: false,
            pending: crate::feedback::Pending::default(),
            name_error: None,
            _subscriptions: subscriptions,
        };
        this.fetch_body(window, cx);
        this
    }

    /// The ONLY body path (EXP-268): synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.body_loading = false;
            return;
        };
        let action_id = self.action_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::prompts::get(&trpc, &action_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                let attended = std::mem::take(&mut view.body_retry_pressed);
                match result {
                    Ok(action) => {
                        view.body_failed = false;
                        view.body.update(cx, |state, cx| {
                            state.set_value(action.body, window, cx);
                        });
                    }
                    // FEED-79: the load on open fails quietly (the field's
                    // empty state + Retry, Save off); only a Retry press
                    // that fails again toasts.
                    Err(err) => {
                        view.body_failed = true;
                        crate::action_error::failed_load(
                            "the prompt",
                            attended,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                    }
                }
                view.body.update(cx, |state, cx| {
                    state.set_placeholder("Prompt", window, cx);
                });
                view.body_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// The failed prompt's Retry: the same load, attended.
    fn retry_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.body_retry_pressed = true;
        self.body_loading = true;
        self.fetch_body(window, cx);
        cx.notify();
    }

    /// ONE batched `prompts.update`.
    /// Writes are owner-only: everyone else reads the form.
    fn read_only(&self, cx: &gpui::App) -> bool {
        !crate::settings::is_owner(cx, &self.team_id)
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting || self.body_loading || self.body_failed || self.read_only(cx) {
            return;
        }
        // A name is one logical line; pasted newlines collapse (EXP-230).
        let name = self
            .name
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        let body = self.body.read(cx).value().to_string();
        if name.is_empty() || body.trim().is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.submitting = true;
        self.pending.start(cx);
        self.name_error = None;
        cx.notify();

        let mut input = api::prompts::PromptUpdate::new(self.action_id.clone());
        input.name = Some(name);
        input.icon = Some(Some(self.icon.clone()));
        input.body = Some(body);

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::prompts::update(&trpc, &input).map(|_| ()) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                view.submitting = false;
                view.pending.finish();
                match result {
                    // The synced echo repaints the header and the list —
                    // nothing to gate on; the form stays where it is.
                    Ok(()) => {}
                    Err(err) => {
                        match err {
                            // The (teamId, name) CONFLICT — inline, web parity.
                            api::ApiError::Http { status: 409, message } => {
                                view.name_error = Some(message.into());
                            }
                            // FEED-79: anything else is the error toast.
                            other => {
                                let form = cx.entity().downgrade();
                                crate::action_error::failed_action_with_retry(
                                    "save",
                                    "the prompt",
                                    crate::action_error::action_error_description(&other),
                                    move |window, cx| {
                                        let _ = form.update(cx, |view, cx| view.submit(window, cx));
                                    },
                                    cx,
                                );
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for ActionPromptForm {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let read_only = self.read_only(cx);
        let name_empty = self.name.read(cx).value().trim().is_empty();
        let body_empty = self.body.read(cx).value().trim().is_empty();
        let disabled =
            name_empty || body_empty || self.body_loading || self.body_failed || self.submitting;
        let muted = cx.theme().muted_foreground;

        // -- left column: the metadata form ---------------------------------
        // EXP-642: the icon picker sits LEFT of the name input, one row
        // (create_board_dialog's `name_field` shape; web parity). A reader
        // sees the glyph, not a picker.
        let icon: gpui::AnyElement = if read_only {
            crate::icons::action_icon(Some(&self.icon))
                .text_color(muted)
                .into_any_element()
        } else {
            let icon_view = cx.entity().clone();
            crate::board_form::icon_picker(
                "action-edit",
                crate::icons::registry::PICKABLE_ICONS,
                Some(&self.icon),
                None,
                false,
                move |name, _, cx| {
                    let Some(name) = name else { return };
                    icon_view.update(cx, |this, cx| {
                        this.icon = name.to_string();
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        };
        // The glyph leads the row, the name types straight into it — the
        // group's fill and hairlines ARE the field chrome.
        let name_row = crate::surface::glass_row_shell()
            .child(icon)
            .child(
                div().flex_1().min_w_0().child(
                    glass_input(&self.name, window, cx)
                        .appearance(false)
                        .h_auto()
                        .px_0()
                        .py_0()
                        .disabled(read_only),
                ),
            );
        let mut form = v_flex()
            .gap_2()
            .child(crate::surface::glass_group_rows(vec![name_row]));
        if let Some(name_error) = self.name_error.clone() {
            form = form.child(crate::action_error::field_error(name_error, cx).px_1());
        }
        let left = form.w(px(META_COLUMN_W)).flex_shrink_0();

        // -- right column: the prompt ---------------------------------------
        // One group, one field: the placeholder is the title here too. A
        // failed load swaps the field for its empty state + Retry (FEED-79).
        let prompt: gpui::AnyElement = if self.body_failed && !self.body_loading {
            crate::action_error::load_failed_state(
                "action-prompt-retry",
                gpui_component::Icon::new(crate::icons::registry::ACTION_DEFAULT),
                PROMPT_UNAVAILABLE,
                cx.listener(|this, _, window, cx| this.retry_body(window, cx)),
                cx,
            )
            .into_any_element()
        } else if self.body_loading {
            div()
                .w_full()
                .px_4()
                .py_4()
                .child(crate::feedback::skeleton_text("action-prompt-placeholder", cx))
                .into_any_element()
        } else {
            Textarea::new(&self.body)
                .appearance(false)
                .w_full()
                .px_4()
                .py_3()
                .font_family(theme::terminal::FONT_FAMILY)
                .disabled(self.body_loading || read_only)
                .into_any_element()
        };
        let right = v_flex()
            .flex_1()
            .min_w_0()
            .child(crate::surface::glass_group().child(prompt));

        // The one write of the section: owners only (a reader has no Save).
        let footer = (!read_only).then(|| {
            h_flex()
                .min_w_0()
                .justify_end()
                .items_center()
                .gap_3()
                .child(self.pending.apply(
                    Button::new("action-edit-save")
                        .primary()
                        .cursor_pointer()
                        .web_sm()
                        .label("Save changes")
                        .disabled(disabled && !self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    crate::feedback::PENDING_SAVE,
                    cx,
                ))
        });

        v_flex()
            .min_w_0()
            .gap_3()
            .child(
                h_flex()
                    .min_w_0()
                    .items_start()
                    .gap_6()
                    .child(left)
                    .child(right),
            )
            .children(footer)
    }
}
