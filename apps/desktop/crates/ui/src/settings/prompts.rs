//! NEW-6: Settings → Prompts — the web `settings/prompts` pages
//! (`components/team/prompts-section.tsx` + `components/prompt-form.tsx`).
//!
//! Prompts replaced actions (NEW-2): a name, a curated icon and a markdown
//! body, nothing else. They RUN from the composer's `/` menu; this is the one
//! place they are managed. Built like web: the list ([`PromptsPane`]) is the
//! "Prompts" band ("New prompt" its owner-only trailing pill) over one row per
//! prompt (icon + name); a row opens the prompt's page ([`PromptPage`]: icon +
//! name in ONE row over the body, Save, and the owner's Delete); "New prompt"
//! opens the same page in create mode. Members read every page; the
//! owner-only controls are HIDDEN from them (the server enforces the writes).
//!
//! The list is LIVE off the synced `prompts` shape (body-less rows); the body
//! comes from `prompts.get` when a page opens.

use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::controls::{glass_input, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;

use super::{section, SettingsSection};

/// The band's label and the nav entry (web `PROMPTS_TITLE`).
pub(crate) const PROMPTS_TITLE: &str = "Prompts";
/// The band's owner-only pill and the create page's header (web
/// `NEW_PROMPT_LABEL`).
pub(crate) const NEW_PROMPT_LABEL: &str = "New prompt";
/// The list's empty state (web `prompts-section.tsx`).
pub(crate) const PROMPTS_EMPTY: &str = "No prompts yet";
/// The body field when `prompts.get` failed (web `prompt-form.tsx`).
const PROMPT_UNAVAILABLE: &str = "Prompt unavailable";
/// The name field's required error (web `prompt-form.tsx`).
const NAME_REQUIRED: &str = "Enter a name";
/// The server's `MAX_PROMPT_BODY` — a zod `.max`, so CHARACTERS (web
/// `PROMPT_BODY_MAX_CHARS`): over it is an input error under the body.
pub(crate) const PROMPT_BODY_MAX_CHARS: usize = 64 * 1024;

/// The body-too-long error (web `prompt-form.tsx`, verbatim).
pub(crate) fn body_too_long(chars: usize) -> Option<String> {
    (chars > PROMPT_BODY_MAX_CHARS)
        .then(|| format!("Too long: {chars} of {PROMPT_BODY_MAX_CHARS} characters"))
}

/// The submit label (web: `Create prompt` / `Save changes`).
pub(crate) fn submit_label(creating: bool) -> &'static str {
    if creating {
        "Create prompt"
    } else {
        "Save changes"
    }
}

/// The confirm before Delete: the prompts fixture's `delete-action` TITLE
/// alone (`Delete "{name}"?`), web `prompts-section.tsx` parity — a prompt
/// has no triggers or sessions, so the action entry's body does not apply.
fn delete_prompt_confirm(name: &str) -> domain::prompts::Prompt {
    domain::prompts::DELETE_ACTION.render("title", None, &[("name", name)])
}

/// Open one prompt's settings page — the list rows, an entity chip naming a
/// prompt, a pinned prompt's "Edit".
pub(crate) fn open_prompt_page(window: &mut Window, cx: &mut App, prompt_id: String) {
    select_settings_section(window, cx, SettingsSection::Prompt(prompt_id));
    navigate(window, cx, Screen::Settings);
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// The Prompts band (web `PromptsListBand`).
pub struct PromptsPane {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl PromptsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.team_members.clone())
        });
        if let Some((prompts, members)) = watched {
            subscriptions.push(cx.observe(&prompts, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for PromptsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        let owner = super::is_owner(cx, &team_id);
        let (prompts, ready) = queries::team_prompts(cx, &team_id);

        let trailing = owner.then(|| {
            crate::surface::glass_pill_button("prompts-new", crate::surface::PillSize::Sm, cx)
                .icon(registry::UI_ADD)
                .label(NEW_PROMPT_LABEL)
                .on_click(|_, window, cx| {
                    select_settings_section(window, cx, SettingsSection::NewPrompt);
                    navigate(window, cx, Screen::Settings);
                })
                .into_any_element()
        });
        let mut body = section(cx).child(crate::surface::glass_section_header(
            PROMPTS_TITLE,
            trailing,
            cx,
        ));
        if prompts.is_empty() {
            body = body.child(if ready {
                crate::controls::empty_state_list(
                    Icon::new(registry::NAV_ACTIONS),
                    PROMPTS_EMPTY,
                    cx,
                )
                .into_any_element()
            } else {
                super::settings_skeleton("prompts-skeleton", cx)
            });
            return v_flex().child(body);
        }

        let muted = cx.theme().muted_foreground;
        let row_hover = cx.theme().list_hover;
        let mut list = v_flex().w_full().min_w_0();
        for prompt in prompts {
            let open_id = prompt.id.clone();
            list = list.child(crate::surface::list_row(
                crate::surface::flat_row()
                    .id(SharedString::from(format!("prompts-list-row-{}", prompt.id)))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .hover(move |this| this.bg(row_hover))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        open_prompt_page(window, cx, open_id.clone());
                    })
                    .child(
                        crate::icons::action_icon(prompt.icon.as_deref())
                            .small()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(prompt.name)),
                    ),
            ));
        }
        v_flex().child(body.child(list))
    }
}

// ---------------------------------------------------------------------------
// One prompt's page (web `PromptSettingsPage` + `PromptForm`)
// ---------------------------------------------------------------------------

/// One prompt's page, or "New prompt" (`prompt_id` None until created).
pub struct PromptPage {
    team_id: String,
    /// The prompt on the page; `None` = create mode. A create flips it to the
    /// new id and the page carries on as that prompt's.
    prompt_id: Option<String>,
    /// The header label: the live synced name (an edit mode page), else
    /// "New prompt".
    header: String,
    /// Whether the prompt's synced row has been seen — after that, its
    /// absence from a READY shape means it was deleted.
    seen_synced: bool,
    name: Entity<InputState>,
    /// The curated registry glyph (the boards set).
    icon: String,
    body: Entity<TextareaState>,
    /// What the form was seeded with / last saved — the dirty check.
    saved_name: String,
    saved_icon: String,
    saved_body: String,
    /// `prompts.get` in flight — the body field shows its skeleton and Save
    /// stays off, so a save can never blank the body.
    body_loading: bool,
    /// FEED-79: `prompts.get` failed (the field's empty state + Retry);
    /// `body_retry_pressed` = the load in flight is the Retry press, whose
    /// failure toasts.
    body_failed: bool,
    body_retry_pressed: bool,
    submitting: bool,
    /// Save's pending face.
    pending: crate::feedback::Pending,
    /// Submit was pressed once — the create page shows the required error
    /// only from then on (web `touched`).
    touched: bool,
    /// The duplicate-name CONFLICT — invalid input, under the name.
    name_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl PromptPage {
    /// The page over `prompt` (the synced row), or create mode in `team_id`.
    pub(crate) fn new(
        team_id: String,
        prompt: Option<api::prompts::Prompt>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // EXP-694: the placeholders ARE the field titles (web parity).
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let body = cx.new(|cx| {
            crate::controls::web_textarea(14, 40, window, cx).placeholder("Prompt")
        });
        let creating = prompt.is_none();
        let default_icon = domain::contract::BOARD_ICON_VALUES[0].to_string();
        let (prompt_id, seed_name, seed_icon) = match &prompt {
            Some(prompt) => (
                Some(prompt.id.clone()),
                prompt.name.clone(),
                prompt.icon.clone().unwrap_or_else(|| default_icon.clone()),
            ),
            None => (None, String::new(), default_icon),
        };
        name.update(cx, |state, cx| state.set_value(seed_name.clone(), window, cx));
        if creating {
            name.update(cx, |state, cx| state.focus(window, cx));
        }

        let mut subscriptions = vec![
            cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::Change => {
                        this.name_error = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                }
            }),
            // Save-gating follows the body live; Enter inserts a newline.
            cx.subscribe_in(&body, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.prompts.clone(), collections.team_members.clone())
        });
        if let Some((prompts, members)) = watched {
            subscriptions.push(cx.observe(&prompts, |this, _, cx| this.on_rows_changed(cx)));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }

        let mut this = Self {
            team_id,
            header: if creating { NEW_PROMPT_LABEL.to_string() } else { seed_name.clone() },
            prompt_id,
            seen_synced: !creating,
            name,
            icon: seed_icon.clone(),
            body,
            saved_name: seed_name,
            saved_icon: seed_icon,
            saved_body: String::new(),
            body_loading: !creating,
            body_failed: false,
            body_retry_pressed: false,
            submitting: false,
            pending: crate::feedback::Pending::default(),
            touched: false,
            name_error: None,
            _subscriptions: subscriptions,
        };
        if !creating {
            this.fetch_body(window, cx);
        }
        this
    }

    /// The prompt this page edits (`None` while it is "New prompt").
    pub(crate) fn prompt_id(&self) -> Option<&str> {
        self.prompt_id.as_deref()
    }

    /// The page's prompt was deleted (by its own Delete or a teammate): its
    /// synced row was seen and a READY shape no longer has it.
    pub(crate) fn is_gone(&self, cx: &App) -> bool {
        let Some(id) = self.prompt_id.as_deref() else {
            return false;
        };
        let Some(store) = sync::Store::try_global(cx) else {
            return false;
        };
        let prompts = store.collections().prompts.read(cx);
        self.seen_synced && prompts.is_ready() && prompts.get(id).is_none()
    }

    /// The synced rows moved: the header follows the live name.
    fn on_rows_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if let (Some(id), Some(store)) = (self.prompt_id.as_deref(), sync::Store::try_global(cx)) {
            if let Some(row) = store.collections().prompts.read(cx).get(id) {
                self.seen_synced = true;
                if let Some(name) = row.name.clone() {
                    self.header = name;
                }
            }
        }
        cx.notify();
    }

    fn read_only(&self, cx: &App) -> bool {
        !super::is_owner(cx, &self.team_id)
    }

    /// The ONLY body path: synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(prompt_id)) = (queries::trpc_client(cx), self.prompt_id.clone())
        else {
            self.body_loading = false;
            return;
        };
        self.body_loading = true;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::prompts::get(&trpc, &prompt_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                let attended = std::mem::take(&mut view.body_retry_pressed);
                match result {
                    Ok(prompt) => {
                        view.body_failed = false;
                        view.saved_body = prompt.body.clone();
                        view.body.update(cx, |state, cx| {
                            state.set_value(prompt.body, window, cx);
                        });
                    }
                    // FEED-79: the load on open fails quietly (the field's
                    // empty state + Retry); only a failed Retry press toasts.
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
                view.body_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn retry_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.body_retry_pressed = true;
        self.fetch_body(window, cx);
        cx.notify();
    }

    /// A name is one logical line; pasted newlines collapse (EXP-230).
    fn typed_name(&self, cx: &App) -> String {
        self.name.read(cx).value().replace(['\r', '\n'], " ").trim().to_string()
    }

    fn can_submit(&self, cx: &App) -> bool {
        let name = self.typed_name(cx);
        let body = self.body.read(cx).value().to_string();
        let dirty = self.prompt_id.is_none()
            || name != self.saved_name
            || self.icon != self.saved_icon
            || body != self.saved_body;
        dirty
            && !name.is_empty()
            && !body.trim().is_empty()
            && body_too_long(body.chars().count()).is_none()
            && !self.body_loading
            && !self.body_failed
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.touched = true;
        cx.notify();
        if self.read_only(cx) || self.submitting || !self.can_submit(cx) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let name = self.typed_name(cx);
        let icon = self.icon.clone();
        let body = self.body.read(cx).value().to_string();
        self.submitting = true;
        self.pending.start(cx);
        self.name_error = None;

        let team_id = self.team_id.clone();
        let prompt_id = self.prompt_id.clone();
        let (sent_name, sent_icon, sent_body) = (name.clone(), icon.clone(), body.clone());
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match prompt_id {
                        Some(id) => {
                            let input = api::prompts::PromptUpdate {
                                name: Some(name),
                                icon: Some(Some(icon)),
                                body: Some(body),
                                ..api::prompts::PromptUpdate::new(id)
                            };
                            api::prompts::update(&trpc, &input)
                        }
                        None => api::prompts::create(&trpc, &team_id, &name, Some(&icon), &body),
                    }
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                view.submitting = false;
                view.pending.finish();
                let creating = view.prompt_id.is_none();
                match result {
                    Ok(saved) => {
                        view.saved_name = sent_name;
                        view.saved_icon = sent_icon;
                        view.saved_body = sent_body;
                        // "Create prompt" → the page becomes the new prompt's
                        // (web: navigate to `/settings/prompts/$id`).
                        if creating {
                            view.header = saved.name.clone();
                            view.prompt_id = Some(saved.id.clone());
                            select_settings_section(window, cx, SettingsSection::Prompt(saved.id));
                        }
                    }
                    Err(err) if crate::action_error::is_name_conflict(&err) => {
                        if let api::ApiError::Http { message, .. } = err {
                            view.name_error = Some(message.into());
                        }
                    }
                    // FEED-79: anything else is the error toast.
                    Err(err) => {
                        let description = crate::action_error::action_error_description(&err);
                        if creating {
                            crate::action_error::failed_action("create", "the prompt", description, cx);
                        } else {
                            let form = cx.entity().downgrade();
                            crate::action_error::failed_action_with_retry(
                                "save",
                                "the prompt",
                                description,
                                move |window, cx| {
                                    let _ = form.update(cx, |view, cx| view.submit(window, cx));
                                },
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Delete behind the shared confirm; the page leaves for the list once
    /// the write lands.
    fn open_delete(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(prompt_id) = self.prompt_id.clone() else {
            return;
        };
        let spec = AlertSpec::from_prompt("Delete prompt", &delete_prompt_confirm(&self.header))
            .on_ok(move |_, cx| {
                spawn_prompt_delete(cx, prompt_id.clone());
                true
            });
        native_dialog::open_alert(window, cx, spec);
    }
}

/// `prompts.delete` over tRPC; a failure toasts (the person's own action).
/// The synced collection drops the row and the page falls back to the list.
fn spawn_prompt_delete(cx: &mut App, prompt_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::prompts::delete(&trpc, &prompt_id) })
            .await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("delete", "the prompt", &err, cx);
            }
        });
    })
    .detach();
}

impl Render for PromptPage {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let read_only = self.read_only(cx);
        let creating = self.prompt_id.is_none();
        let muted = cx.theme().muted_foreground;

        // Icon and name are ONE row (web parity): the glyph picker leads, the
        // name types straight into the row. A reader sees the glyph.
        let icon: gpui::AnyElement = if read_only {
            crate::icons::action_icon(Some(&self.icon)).text_color(muted).into_any_element()
        } else {
            let view = cx.entity().downgrade();
            crate::board_form::icon_picker(
                "prompt-icon",
                registry::PICKABLE_ICONS,
                Some(&self.icon),
                None,
                false,
                move |name, _, cx| {
                    let Some(name) = name else { return };
                    let _ = view.update(cx, |this, cx| {
                        this.icon = name.to_string();
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        };
        let name_row = crate::surface::glass_row_shell().child(icon).child(
            div().flex_1().min_w_0().child(
                glass_input(&self.name, window, cx)
                    .appearance(false)
                    .h_auto()
                    .px_0()
                    .py_0()
                    .disabled(read_only),
            ),
        );
        let name_required = (!read_only
            && (self.touched || !creating)
            && self.typed_name(cx).is_empty())
        .then_some(NAME_REQUIRED);
        let name_error: Option<SharedString> = self
            .name_error
            .clone()
            .or_else(|| name_required.map(SharedString::from));
        let mut name_group =
            v_flex().gap_2().child(crate::surface::glass_group_rows(vec![name_row]));
        if let Some(error) = name_error {
            name_group = name_group.child(crate::action_error::field_error(error, cx).px_1());
        }

        // The body: one group, one field; a failed load swaps it for its
        // empty state + Retry (FEED-79), a load in flight for the skeleton.
        let body_field: gpui::AnyElement = if self.body_loading {
            div()
                .w_full()
                .min_h(gpui::px(256.))
                .px_4()
                .py_4()
                .child(crate::feedback::skeleton_text("prompt-body-placeholder", cx))
                .into_any_element()
        } else if self.body_failed {
            crate::action_error::load_failed_state(
                "prompt-body-retry",
                Icon::new(registry::NAV_ACTIONS),
                PROMPT_UNAVAILABLE,
                cx.listener(|this, _, window, cx| this.retry_body(window, cx)),
                cx,
            )
            .into_any_element()
        } else {
            Textarea::new(&self.body)
                .appearance(false)
                .w_full()
                .min_h(gpui::px(256.))
                .px_4()
                .py_3()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .disabled(read_only)
                .into_any_element()
        };
        let mut body_group = v_flex()
            .gap_2()
            .child(crate::surface::glass_group().child(body_field));
        if let Some(error) = body_too_long(self.body.read(cx).value().chars().count()) {
            body_group = body_group.child(crate::action_error::field_error(error, cx).px_1());
        }

        let mut form = v_flex().min_w_0().gap_3().child(name_group).child(body_group);
        // Read-only draws no save strip at all (web parity).
        if !read_only {
            let disabled = !self.can_submit(cx);
            form = form.child(
                h_flex().min_w_0().justify_end().gap_2().child(self.pending.apply(
                    Button::new("prompt-submit")
                        .primary()
                        .cursor_pointer()
                        .web_sm()
                        .label(submit_label(creating))
                        .disabled(disabled && !self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    crate::feedback::PENDING_SAVE,
                    cx,
                )),
            );
        }

        let mut page = section(cx)
            .gap_4()
            .child(crate::surface::glass_section_header(self.header.clone(), None, cx))
            .child(form);
        // The one owner-only control below the form: hidden from members.
        if !creating && !read_only {
            page = page.child(
                h_flex().child(
                    Button::new("prompt-delete")
                        .danger()
                        .cursor_pointer()
                        .web_sm()
                        .icon(registry::UI_DELETE)
                        .label("Delete prompt")
                        .on_click(cx.listener(|this, _, window, cx| this.open_delete(window, cx))),
                ),
            );
        }
        v_flex().child(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copy is the web page's, word for word.
    #[test]
    fn copy_matches_the_web_page() {
        assert_eq!(PROMPTS_TITLE, "Prompts");
        assert_eq!(NEW_PROMPT_LABEL, "New prompt");
        assert_eq!(PROMPTS_EMPTY, "No prompts yet");
        assert_eq!(submit_label(true), "Create prompt");
        assert_eq!(submit_label(false), "Save changes");
        assert_eq!(body_too_long(PROMPT_BODY_MAX_CHARS), None);
        assert_eq!(
            body_too_long(PROMPT_BODY_MAX_CHARS + 1).as_deref(),
            Some("Too long: 65537 of 65536 characters")
        );
    }

    /// Delete confirms with the fixture title alone, Cancel + a destructive
    /// Delete (web `prompts-section.tsx`).
    #[test]
    fn delete_confirm_is_the_title_alone() {
        let prompt = delete_prompt_confirm("Release train");
        assert_eq!(prompt.title, "Delete \"Release train\"?");
        assert_eq!(prompt.body, None);
        let labels: Vec<&str> = prompt.actions.iter().map(|a| a.label).collect();
        assert_eq!(labels, vec!["Cancel", "Delete"]);
    }
}
