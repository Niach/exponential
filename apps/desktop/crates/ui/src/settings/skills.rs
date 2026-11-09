//! #EXP-1660: Settings › Skills — the desktop twin of the web
//! `settings/skills` pages. The spec is
//! `packages/domain-contract/fixtures/skills.json` (`list`, `page`), its copy
//! and rules through `domain::skills`.
//!
//! Built like Prompts (NEW-6) and the MCP servers' rows: the list
//! ([`SkillsPane`]) is the "Skills" band ("New skill" its owner-only
//! trailing pill) over one row per skill (the `ui-docs` glyph, the name, the
//! Default pill while `enabledByDefault`, the description as a one-line
//! caption); a row opens the skill's page ([`SkillPage`]: name + description
//! in one group, the SKILL.md body in the prompt page's editor, the "On by
//! default" switch, Save, and the owner's Delete behind THE Prompt). Members
//! read every page; the owner-only controls are hidden or disabled for them
//! (the server enforces the writes).
//!
//! The list is LIVE off the synced `skills` shape (body-less rows); the body
//! comes from `skills.get` when a page opens.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};

use domain::skills::{self as spec, SkillField};

use crate::controls::{glass_input, PointerMenuButtonExt as _, WebControl as _};
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;
use crate::surface::{glass_pill, PillMode, PillSize};

use super::{section, SettingsSection};

/// The body field when `skills.get` failed.
const SKILL_UNAVAILABLE: &str = "Skill unavailable";

/// Open one skill's settings page.
pub(crate) fn open_skill_page(window: &mut Window, cx: &mut App, skill_id: String) {
    select_settings_section(window, cx, SettingsSection::Skill(skill_id));
    navigate(window, cx, Screen::Settings);
}

/// The skill glyph (`ui-docs` = book-open, skills.json / settings-nav.json).
pub(crate) fn skill_icon() -> Icon {
    Icon::from(registry::UI_DOCS)
}

// ---------------------------------------------------------------------------
// The list
// ---------------------------------------------------------------------------

/// The owners' row "…" menu (web `SkillsListBand`): Edit, then the
/// destructive Delete behind THE Prompt. The wrapper keeps its click off the
/// row (which opens the page).
fn row_menu(id: &str, name: &str, cx: &App) -> gpui::AnyElement {
    let (edit_id, delete_id) = (id.to_string(), id.to_string());
    let button = crate::controls::ghost_icon_button(
        SharedString::from(format!("skill-menu-{id}")),
        Icon::new(registry::UI_MORE),
        cx,
    )
    .tooltip(SharedString::from(format!("Skill menu for {name}")))
    .pointer_menu(move |menu, _window, cx| {
        let (edit_id, delete_id) = (edit_id.clone(), delete_id.clone());
        menu.item(
            crate::controls::pointer_label_item("Edit", false)
                .icon(Icon::new(registry::UI_EDIT))
                .on_click(move |_, window, cx| open_skill_page(window, cx, edit_id.clone())),
        )
        .separator()
        .item(
            crate::controls::danger_menu_item(spec::DELETE_CONFIRM, Icon::new(registry::UI_DELETE), cx)
                .on_click(move |_, window, cx| confirm_delete(delete_id.clone(), window, cx)),
        )
    });
    // a11y: a click guard, not a control
    div()
        .id(SharedString::from(format!("skill-menu-wrap-{id}")))
        .flex_shrink_0()
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(button)
        .into_any_element()
}

/// Delete behind THE Prompt (skills.json `page.deleteConfirm`).
fn confirm_delete(skill_id: String, window: &mut Window, cx: &mut App) {
    let alert = AlertSpec::from_prompt(spec::DELETE_LABEL, &spec::delete_confirm()).on_ok(move |_, cx| {
        spawn_skill_delete(cx, skill_id.clone());
        true
    });
    native_dialog::open_alert(window, cx, alert);
}

/// One list row: glyph, name + its pill (Default, or Built-in for a
/// built-in: `domain::skills::skill_pill`), the description caption and the
/// owners' "…" menu (the MCP server row's layout; never on a built-in).
pub(crate) fn skill_row(
    id: &str,
    name: &str,
    description: &str,
    pill: Option<&'static str>,
    owner: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let muted = cx.theme().muted_foreground;
    let row_hover = cx.theme().list_hover;
    let mut title = h_flex().w_full().min_w_0().gap_2().items_center().child(
        div()
            .min_w_0()
            .truncate()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .child(SharedString::from(name.to_string())),
    );
    if let Some(pill) = pill {
        title = title.child(
            glass_pill(SharedString::from(format!("skill-default-{id}")), PillSize::Sm, PillMode::Readonly, cx)
                .child(div().text_xs().child(pill)),
        );
    }
    crate::surface::flat_row()
        .id(SharedString::from(format!("settings-skills-row-{id}")))
        .flex()
        .w_full()
        .min_w_0()
        .gap_3()
        .items_center()
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(move |this| this.bg(row_hover))
        .child(skill_icon().size(gpui::px(14.)).flex_shrink_0().text_color(muted))
        .child(
            v_flex().flex_1().min_w_0().gap_1().child(title).child(
                div()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .truncate()
                    .child(SharedString::from(description.to_string())),
            ),
        )
        .when(owner, |row| row.child(row_menu(id, name, cx)))
}

/// The Skills band (web `SkillsListBand`).
pub struct SkillsPane {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl SkillsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.skills.clone(), collections.team_members.clone())
        });
        if let Some((skills, members)) = watched {
            subscriptions.push(cx.observe(&skills, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }
        Self { nav, _subscriptions: subscriptions }
    }
}

impl Render for SkillsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SkillsPane").entered();
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        let owner = super::is_owner(cx, &team_id);
        let (skills, ready) = queries::team_skills(cx, &team_id);

        let trailing = owner.then(|| {
            crate::surface::glass_pill_button("skills-new", PillSize::Sm, cx)
                .icon(registry::UI_ADD)
                .label(spec::NEW_BUTTON)
                .on_click(|_, window, cx| {
                    select_settings_section(window, cx, SettingsSection::NewSkill);
                    navigate(window, cx, Screen::Settings);
                })
                .into_any_element()
        });
        let mut body = section(cx)
            .id("settings-skills-page")
            .child(crate::surface::glass_section_header(spec::TITLE, trailing, cx));
        if skills.is_empty() {
            body = body.child(if ready {
                crate::controls::empty_state_list(skill_icon(), spec::EMPTY_TITLE, cx).into_any_element()
            } else {
                super::settings_skeleton("skills-skeleton", cx)
            });
            return v_flex().child(body);
        }

        let mut list = v_flex().id("settings-skills-list").w_full().min_w_0();
        for skill in skills {
            let open_id = skill.id.clone();
            list = list.child(crate::surface::list_row(
                skill_row(
                    &skill.id,
                    &skill.name,
                    &skill.description,
                    spec::skill_pill(skill.builtin, skill.enabled_by_default),
                    spec::can_edit_skill(skill.builtin, owner),
                    cx,
                )
                .on_click(
                    move |_: &ClickEvent, window, cx| {
                        open_skill_page(window, cx, open_id.clone());
                    },
                ),
            ));
        }
        v_flex().child(body.child(list))
    }
}

// ---------------------------------------------------------------------------
// One skill's page
// ---------------------------------------------------------------------------

/// One skill's page, or "New skill" (`skill_id` None until created).
pub struct SkillPage {
    team_id: String,
    /// `None` = create mode; a create flips it to the new id.
    skill_id: Option<String>,
    /// The header: the live synced name, else "New skill".
    header: String,
    seen_synced: bool,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    body: Entity<TextareaState>,
    enabled_by_default: bool,
    /// What the form was seeded with / last saved — the dirty check.
    saved_name: String,
    saved_description: String,
    saved_body: String,
    saved_default: bool,
    body_loading: bool,
    body_failed: bool,
    body_retry_pressed: bool,
    submitting: bool,
    pending: crate::feedback::Pending,
    /// Save was pressed once — the create page shows its errors from then on.
    touched: bool,
    /// The server's CONFLICT (the name is taken), under the name.
    name_taken: bool,
    /// #VAPP-213: a built-in: read-only for everyone (`can_edit_skill`).
    builtin: bool,
    _subscriptions: Vec<Subscription>,
}

impl SkillPage {
    pub(crate) fn new(
        team_id: String,
        skill: Option<api::skills::Skill>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(spec::NAME_PLACEHOLDER));
        let description = cx.new(|cx| {
            crate::controls::web_textarea(2, 6, window, cx).placeholder(spec::DESCRIPTION_PLACEHOLDER)
        });
        let body = cx.new(|cx| crate::controls::web_textarea(14, 40, window, cx).placeholder(spec::BODY_PLACEHOLDER));
        let creating = skill.is_none();
        let builtin = skill.as_ref().is_some_and(|skill| skill.builtin);
        let (skill_id, seed_name, seed_description, seed_default) = match &skill {
            Some(skill) => (Some(skill.id.clone()), skill.name.clone(), skill.description.clone(), skill.enabled_by_default),
            None => (None, String::new(), String::new(), false),
        };
        name.update(cx, |state, cx| state.set_value(seed_name.clone(), window, cx));
        description.update(cx, |state, cx| state.set_value(seed_description.clone(), window, cx));
        if creating {
            name.update(cx, |state, cx| state.focus(window, cx));
        }

        let mut subscriptions = vec![
            cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.name_taken = false;
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&description, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&body, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (collections.skills.clone(), collections.team_members.clone())
        });
        if let Some((skills, members)) = watched {
            subscriptions.push(cx.observe(&skills, |this, _, cx| this.on_rows_changed(cx)));
            subscriptions.push(cx.observe(&members, |_, _, cx| cx.notify()));
        }

        let mut this = Self {
            team_id,
            header: if creating { spec::NEW_BUTTON.to_string() } else { seed_name.clone() },
            skill_id,
            seen_synced: !creating,
            name,
            description,
            body,
            enabled_by_default: seed_default,
            saved_name: seed_name,
            saved_description: seed_description,
            saved_body: String::new(),
            saved_default: seed_default,
            body_loading: !creating,
            body_failed: false,
            body_retry_pressed: false,
            submitting: false,
            pending: crate::feedback::Pending::default(),
            touched: false,
            name_taken: false,
            builtin,
            _subscriptions: subscriptions,
        };
        if !creating {
            this.fetch_body(window, cx);
        }
        this
    }

    pub(crate) fn skill_id(&self) -> Option<&str> {
        self.skill_id.as_deref()
    }

    /// The page's skill was deleted: its synced row was seen and a READY
    /// shape no longer has it.
    pub(crate) fn is_gone(&self, cx: &App) -> bool {
        let (Some(id), Some(store)) = (self.skill_id.as_deref(), sync::Store::try_global(cx)) else {
            return false;
        };
        let skills = store.collections().skills.read(cx);
        self.seen_synced && skills.is_ready() && skills.get(id).is_none()
    }

    fn on_rows_changed(&mut self, cx: &mut gpui::Context<Self>) {
        if let (Some(id), Some(store)) = (self.skill_id.as_deref(), sync::Store::try_global(cx)) {
            if let Some(row) = store.collections().skills.read(cx).get(id) {
                self.seen_synced = true;
                self.header = row.name.clone();
            }
        }
        cx.notify();
    }

    fn read_only(&self, cx: &App) -> bool {
        !spec::can_edit_skill(self.builtin, super::is_owner(cx, &self.team_id))
    }

    /// The ONLY body path: synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(trpc), Some(skill_id)) = (queries::trpc_client(cx), self.skill_id.clone()) else {
            self.body_loading = false;
            return;
        };
        self.body_loading = true;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::skills::get(&trpc, &skill_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                let attended = std::mem::take(&mut view.body_retry_pressed);
                match result {
                    Ok(skill) => {
                        view.body_failed = false;
                        view.saved_body = skill.body.clone();
                        view.body.update(cx, |state, cx| state.set_value(skill.body, window, cx));
                    }
                    Err(err) => {
                        view.body_failed = true;
                        crate::action_error::failed_load(
                            "the skill",
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

    /// The form as typed: the name is one logical line.
    fn typed(&self, cx: &App) -> (String, String, String) {
        (
            self.name.read(cx).value().replace(['\r', '\n'], " ").trim().to_string(),
            self.description.read(cx).value().trim().to_string(),
            self.body.read(cx).value().to_string(),
        )
    }

    fn errors(&self, cx: &App) -> Vec<spec::SkillFormError> {
        let (name, description, body) = self.typed(cx);
        spec::validate_skill_form(&name, &description, &body)
    }

    fn dirty(&self, cx: &App) -> bool {
        let (name, description, body) = self.typed(cx);
        self.skill_id.is_none()
            || name != self.saved_name
            || description != self.saved_description
            || body != self.saved_body
            || self.enabled_by_default != self.saved_default
    }

    fn can_submit(&self, cx: &App) -> bool {
        self.dirty(cx) && self.errors(cx).is_empty() && !self.body_loading && !self.body_failed
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
        let (name, description, body) = self.typed(cx);
        let enabled_by_default = self.enabled_by_default;
        self.submitting = true;
        self.pending.start(cx);
        self.name_taken = false;

        let team_id = self.team_id.clone();
        let skill_id = self.skill_id.clone();
        let sent = (name.clone(), description.clone(), body.clone(), enabled_by_default);
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    match skill_id {
                        Some(id) => api::skills::update(
                            &trpc,
                            &api::skills::SkillUpdate {
                                name: Some(name),
                                description: Some(description),
                                body: Some(body),
                                enabled_by_default: Some(enabled_by_default),
                                ..api::skills::SkillUpdate::new(id)
                            },
                        ),
                        None => api::skills::create(
                            &trpc,
                            &api::skills::SkillCreate {
                                team_id,
                                name,
                                description,
                                body,
                                enabled_by_default: Some(enabled_by_default),
                            },
                        ),
                    }
                })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                view.submitting = false;
                view.pending.finish();
                let creating = view.skill_id.is_none();
                match result {
                    Ok(saved) => {
                        (view.saved_name, view.saved_description, view.saved_body, view.saved_default) = sent;
                        if creating {
                            view.header = saved.name.clone();
                            view.skill_id = Some(saved.id.clone());
                            select_settings_section(window, cx, SettingsSection::Skill(saved.id));
                        }
                    }
                    // skills.json: a CONFLICT reads errors.nameTaken inline.
                    Err(err) if crate::action_error::is_name_conflict(&err) => view.name_taken = true,
                    Err(err) => {
                        let description = crate::action_error::action_error_description(&err);
                        if creating {
                            crate::action_error::failed_action("create", "the skill", description, cx);
                        } else {
                            let form = cx.entity().downgrade();
                            crate::action_error::failed_action_with_retry(
                                "save",
                                "the skill",
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

    fn open_delete(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(skill_id) = self.skill_id.clone() else {
            return;
        };
        confirm_delete(skill_id, window, cx);
    }

    /// The inline error under `field`, once it may show.
    fn field_error(&self, field: SkillField, errors: &[spec::SkillFormError]) -> Option<&'static str> {
        if field == SkillField::Name && self.name_taken {
            return Some(spec::ERR_NAME_TAKEN);
        }
        (self.touched || self.skill_id.is_some())
            .then(|| errors.iter().find(|error| error.field == field).map(|error| error.message))
            .flatten()
    }
}

/// `skills.delete` over tRPC; a failure toasts. The synced collection drops
/// the row and the page falls back to the list.
fn spawn_skill_delete(cx: &mut App, skill_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::skills::delete(&trpc, &skill_id) })
            .await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("delete", "the skill", &err, cx);
            }
        });
    })
    .detach();
}

impl Render for SkillPage {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SkillPage").entered();
        let read_only = self.read_only(cx);
        let creating = self.skill_id.is_none();
        let muted = cx.theme().muted_foreground;
        let errors = if read_only { Vec::new() } else { self.errors(cx) };
        let with_error = |group: gpui::Div, error: Option<&'static str>, cx: &App| match error {
            Some(error) => v_flex().gap_2().child(group).child(crate::action_error::field_error(error, cx).px_1()),
            None => v_flex().gap_2().child(group),
        };

        // Name (mono) + description: ONE group, the prompt page's name row.
        let name_row = crate::surface::glass_row_shell().child(skill_icon().text_color(muted)).child(
            div().flex_1().min_w_0().font_family(theme::terminal::FONT_FAMILY).child(
                glass_input(&self.name, window, cx).appearance(false).h_auto().px_0().py_0().disabled(read_only),
            ),
        );
        // The description is a grouped field row of its own (web
        // `GROUPED_FIELD_ROW`): the prompt body's insets, so its text lines
        // up with the body below.
        let description_row = div().w_full().child(
            Textarea::new(&self.description)
                .appearance(false)
                .w_full()
                .px_4()
                .py_3()
                .text_sm()
                .disabled(read_only),
        );
        let name_error = self.field_error(SkillField::Name, &errors);
        let description_error = self.field_error(SkillField::Description, &errors);
        let identity = with_error(
            crate::surface::glass_group_rows(vec![name_row, description_row]),
            name_error.or(description_error),
            cx,
        );

        // The body: THE prompt page's editor; skeleton / Retry while loading.
        let body_field: gpui::AnyElement = if self.body_loading {
            div()
                .w_full()
                .min_h(gpui::px(256.))
                .px_4()
                .py_4()
                .child(crate::feedback::skeleton_text("skill-body-placeholder", cx))
                .into_any_element()
        } else if self.body_failed {
            crate::action_error::load_failed_state(
                "skill-body-retry",
                skill_icon(),
                SKILL_UNAVAILABLE,
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
        let body = with_error(
            crate::surface::glass_group().child(body_field),
            self.field_error(SkillField::Body, &errors),
            cx,
        );

        // Pre-select in new sessions: THE toggle row, no subtitle.
        let switch = crate::controls::web_switch("skill-enabled-by-default")
            .checked(self.enabled_by_default)
            .disabled(read_only)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.enabled_by_default = *on;
                cx.notify();
            }));
        let default_group = crate::surface::glass_group_rows(vec![crate::surface::glass_toggle_row(
            spec::DEFAULT_LABEL,
            None,
            switch.into_any_element(),
            cx,
        )]);

        let mut form = v_flex().min_w_0().gap_3().child(identity).child(body).child(default_group);
        if !read_only {
            let off = !self.can_submit(cx) && !self.submitting;
            form = form.child(
                h_flex().min_w_0().justify_end().gap_2().child(self.pending.apply(
                    Button::new("skill-submit")
                        .primary()
                        .cursor_pointer()
                        .web_sm()
                        .label(spec::SAVE)
                        .disabled(off)
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
        if !creating && !read_only {
            page = page.child(
                h_flex().child(
                    Button::new("skill-delete")
                        .danger()
                        .cursor_pointer()
                        .web_sm()
                        .icon(registry::UI_DELETE)
                        .label(spec::DELETE_LABEL)
                        .on_click(cx.listener(|this, _, window, cx| this.open_delete(window, cx))),
                ),
            );
        }
        v_flex().child(page)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn delete_confirms_with_the_fixture_copy() {
        let prompt = domain::skills::delete_confirm();
        assert_eq!(prompt.title, "Delete this skill?");
        assert_eq!(prompt.body.as_deref(), Some("Sessions that picked it run without it from their next start."));
        let labels: Vec<&str> = prompt.actions.iter().map(|a| a.label).collect();
        assert_eq!(labels, vec!["Cancel", "Delete"]);
        assert_eq!(prompt.focus, "cancel");
    }
}
