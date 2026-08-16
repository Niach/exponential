//! The "Edit action" dialog (EXP-467) — the web `ActionEditorDialog` 1:1,
//! replacing the old full-page action detail's inline per-field editing.
//! Two columns like the web: metadata left (Name / Description / Icon /
//! Repository), the prompt right as a plain monospace multiline editor (the
//! web textarea — NOT the WYSIWYG), one batched `actions.update` on Save.
//!
//! Synced rows carry no body (EXP-268), so the prompt is fetched via
//! `actions.get` on open and the field stays disabled until it lands — a
//! save can never blank it. A duplicate-name CONFLICT (HTTP 409) renders
//! inline under the Name field, everything else in the generic box above the
//! footer. There is deliberately NO inputs editor — the web dialog has none;
//! input definitions are authored by the "Create action" builtin run, and
//! the batched update omits `inputs`, leaving them untouched.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Open the edit dialog for a non-builtin action, seeded from its synced
/// row. A native window — never nests, and it is only ever opened from the
/// main window's card menu (the Actions screen is not undockable).
pub(crate) fn open(window: &mut Window, cx: &mut App, action_id: String) {
    if api::actions::is_builtin_action_id(&action_id) {
        return;
    }
    let Some(store) = sync::Store::try_global(cx) else {
        return;
    };
    let collection = store.collections().actions.clone();
    let Some(action) = collection.read(cx).get(&action_id).map(api::actions::from_row) else {
        return;
    };
    // Web sm:max-w-4xl, 16:9-ish — the prompt column is the tall field, so
    // splitting columns keeps the dialog wide instead of a narrow tower.
    let height = (window.viewport_size().height * 0.85).min(px(560.));
    let spec = DialogSpec::new("Edit action", size(px(760.), height))
        .resizable(size(px(600.), px(420.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| ActionEditorDialogView::new(action, window, cx));
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own footer and scrolls its columns — the
            // shell's wrapper would scroll "Save changes" out of reach.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).submitting)
    });
}

struct ActionEditorDialogView {
    action_id: String,
    team_id: String,
    name: gpui::Entity<InputState>,
    description: gpui::Entity<InputState>,
    /// The curated registry glyph (`actionIconSchema` — the boards set).
    icon: String,
    /// `None` = repo-less scratch run (the web select's "None").
    repo_id: Option<String>,
    /// `repositories.list` rows for the picker; `None` = still loading.
    repos: Option<Vec<crate::action_run::ActionRepoRow>>,
    /// The prompt — a plain multiline editor (web textarea parity).
    body: gpui::Entity<TextareaState>,
    /// `actions.get` in flight — the prompt and Save stay disabled so a
    /// save can never blank the body.
    body_loading: bool,
    submitting: bool,
    /// Duplicate-name CONFLICT, rendered inline under Name.
    name_error: Option<SharedString>,
    error: Option<SharedString>,
    focused_once: bool,
    left_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ActionEditorDialogView {
    fn new(
        action: api::actions::Action,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Code review sweep"));
        name.update(cx, |state, cx| {
            state.set_value(action.name.clone(), window, cx);
        });
        let description = cx.new(|cx| {
            InputState::new(window, cx).placeholder("What this action does, for the list")
        });
        description.update(cx, |state, cx| {
            state.set_value(action.description.clone().unwrap_or_default(), window, cx);
        });
        let body = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("The markdown prompt the agent runs with…")
        });

        // Enter submits from the one-line fields; in the prompt it inserts a
        // newline (hence no shell-level `on_enter`).
        let mut subscriptions = Vec::new();
        for field in [&name, &description] {
            subscriptions.push(cx.subscribe_in(
                field,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::PressEnter { .. } => this.submit(window, cx),
                    _ => {}
                },
            ));
        }
        // Save-gating (empty body) follows the editor live.
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
            description,
            icon: action
                .icon
                .clone()
                .unwrap_or_else(|| domain::contract::BOARD_ICON_VALUES[0].to_string()),
            repo_id: action.repository_id.clone(),
            repos: None,
            body,
            body_loading: true,
            submitting: false,
            name_error: None,
            error: None,
            focused_once: false,
            left_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        };
        this.fetch_body(window, cx);
        this.fetch_repos(window, cx);
        this
    }

    /// The ONLY body path (EXP-268): synced rows exclude it.
    fn fetch_body(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            self.body_loading = false;
            return;
        };
        let action_id = self.action_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::actions::get(&trpc, &action_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                match result {
                    Ok(action) => {
                        view.body.update(cx, |state, cx| {
                            state.set_value(action.body, window, cx);
                        });
                    }
                    Err(err) => view.error = Some(format!("{err}").into()),
                }
                view.body_loading = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn fetch_repos(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { crate::action_run::fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update_in(window, |view, _, cx| {
                match result {
                    Ok(rows) => view.repos = Some(rows),
                    // The picker degrades to the seeded choice — the save
                    // still round-trips it untouched.
                    Err(err) => log::warn!("actions: repositories.list failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// ONE batched `actions.update` — the web dialog's submit. `inputs`
    /// stays omitted (untouched).
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting || self.body_loading {
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
        let description = self
            .description
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.submitting = true;
        self.name_error = None;
        self.error = None;
        cx.notify();

        let mut input = api::actions::ActionUpdate::new(self.action_id.clone());
        input.name = Some(name);
        // "" clears — the existing desktop convention (the server nulls it).
        input.description = Some(description);
        input.icon = Some(self.icon.clone());
        input.repository_id = api::Patch::set_or_null(self.repo_id.clone());
        input.body = Some(body);

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::actions::update(&trpc, &input).map(|_| ()) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                match result {
                    // The synced echo repaints the cards — nothing to gate on.
                    Ok(()) => native_dialog::close_dialog_window(window, cx),
                    Err(err) => {
                        view.submitting = false;
                        match err {
                            // The (teamId, name) CONFLICT — inline, web parity.
                            api::ApiError::Http { status: 409, message } => {
                                view.name_error = Some(message.into());
                            }
                            other => view.error = Some(format!("{other}").into()),
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
}

fn field_label(cx: &App, label: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

impl Render for ActionEditorDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let name_empty = self.name.read(cx).value().trim().is_empty();
        let body_empty = self.body.read(cx).value().trim().is_empty();
        let disabled = name_empty || body_empty || self.body_loading || self.submitting;
        let danger = cx.theme().danger;
        let muted = cx.theme().muted_foreground;

        // -- left column: the metadata form ---------------------------------
        let icon_view = cx.entity().clone();
        let mut name_field = v_flex()
            .gap_2()
            .child(field_label(cx, "Name"))
            .child(Input::new(&self.name).small());
        if let Some(name_error) = self.name_error.clone() {
            name_field = name_field.child(div().text_xs().text_color(danger).child(name_error));
        }
        let form = v_flex()
            .gap_4()
            .child(name_field)
            .child(
                v_flex()
                    .gap_2()
                    .child(field_label(cx, "Description (optional)"))
                    .child(Input::new(&self.description).small()),
            )
            .child(
                v_flex().gap_2().child(field_label(cx, "Icon")).child(
                    crate::board_form::icon_swatch_grid(
                        "action-edit",
                        &self.icon,
                        move |name, _, cx| {
                            icon_view.update(cx, |this, cx| {
                                this.icon = name.to_string();
                                cx.notify();
                            });
                        },
                        cx,
                    ),
                ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(field_label(cx, "Repository (optional)"))
                    .child(self.render_repo_picker(cx))
                    .child(div().text_xs().text_color(muted).child(
                        "With a repository the run clones it first; without one the agent \
                         works in a scratch directory.",
                    )),
            );
        let left = div()
            .w(px(280.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(crate::scroll_pane::v_scroll_pane(
                "action-edit-left",
                &self.left_scroll,
                form.pr_2().pb_2(),
            ));

        // -- right column: the prompt (the scrollable field) ----------------
        let right = v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_2()
            .child(field_label(cx, "Prompt"))
            .child(
                div().flex_1().min_h_0().child(
                    Textarea::new(&self.body)
                        .h_full()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .disabled(self.body_loading),
                ),
            );

        let footer = h_flex()
            .flex_shrink_0()
            .justify_end()
            .gap_2()
            .child(
                Button::new("action-edit-cancel")
                    .ghost()
                    .small()
                    .label("Cancel")
                    .disabled(self.submitting)
                    .on_click(|_, window, cx| native_dialog::close_dialog_window(window, cx)),
            )
            .child(
                Button::new("action-edit-save")
                    .primary()
                    .small()
                    .label(if self.submitting {
                        "Saving…"
                    } else {
                        "Save changes"
                    })
                    .disabled(disabled)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            );

        v_flex()
            .size_full()
            .gap_3()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .gap_6()
                    .child(left)
                    .child(right),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(footer.pt_3().border_t_1().border_color(cx.theme().border))
    }
}

impl ActionEditorDialogView {
    /// The web repository `Select`: "None" + one entry per connected repo.
    fn render_repo_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let label: SharedString = match (&self.repos, &self.repo_id) {
            (None, _) => "Loading repositories…".into(),
            (Some(rows), Some(repo_id)) => rows
                .iter()
                .find(|row| &row.id == repo_id)
                .map(|row| SharedString::from(row.full_name.clone()))
                .unwrap_or_else(|| "Repository".into()),
            (Some(_), None) => "None".into(),
        };
        let trigger = Button::new("action-edit-repo")
            .outline()
            .small()
            .label(label)
            .w_full()
            .disabled(self.repos.is_none());
        let Some(rows) = self.repos.clone() else {
            return trigger.into_any_element();
        };
        let view = cx.entity().downgrade();
        trigger
            .dropdown_menu(move |mut menu, _window, _cx| {
                let none_view = view.clone();
                menu = menu.item(PopupMenuItem::new("None").on_click(move |_, _, cx| {
                    if let Some(view) = none_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.repo_id = None;
                            cx.notify();
                        });
                    }
                }));
                for repo in &rows {
                    let view = view.clone();
                    let repo_id = repo.id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(repo.full_name.clone())).on_click(
                            move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let repo_id = repo_id.clone();
                                    view.update(cx, |this, cx| {
                                        this.repo_id = Some(repo_id);
                                        cx.notify();
                                    });
                                }
                            },
                        ),
                    );
                }
                menu
            })
            .into_any_element()
    }
}
