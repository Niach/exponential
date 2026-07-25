//! Full-page action detail (EXP-277) — the Actions tool-window rows' center
//! screen, mirroring the issue detail's shape: the full markdown prompt in a
//! centered column, a properties-style right sidebar (Run, Repository,
//! Inputs, Created/Updated, owner-only Edit/Delete).
//!
//! Data model: the synced `actions` shape carries the body-less row (name,
//! description, inputs, repo binding); the prompt body is tRPC-only
//! (`actions.get` — EXP-268), fetched per [`ActionDetailView::set_action`]
//! and refetched when the synced row's `updated_at` moves (covers both the
//! local Edit dialog save and remote/MCP edits). Builtins never navigate
//! here (no stable body — their rows open the start dialog instead); a
//! builtin or unknown id renders the not-found state.

use gpui::{
    div, px, App, ClickEvent, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::action_run::{fetch_repositories, ActionRepoRow};
use crate::icons::ExpIcon;
use crate::issue_detail::centered_column;
use crate::properties_panel::property_group;
use crate::queries;

pub struct ActionDetailView {
    action_id: Option<String>,
    /// The fetched prompt body (`None` = in flight).
    body: Option<SharedString>,
    body_error: Option<SharedString>,
    /// The synced row's `updated_at` at fetch time — a moved value on the
    /// collection observer triggers a refetch.
    fetched_updated_at: Option<String>,
    /// The team's repo rows (for the Repository chip's full name).
    repos: Vec<ActionRepoRow>,
    body_scroll: gpui::ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl ActionDetailView {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(store) = sync::Store::try_global(cx) {
            let actions = store.collections().actions.clone();
            subscriptions.push(cx.observe(&actions, |this, _, cx| {
                this.refetch_if_stale(cx);
                cx.notify();
            }));
        }
        Self {
            action_id: None,
            body: None,
            body_error: None,
            fetched_updated_at: None,
            repos: Vec::new(),
            body_scroll: gpui::ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Point the view at an action (the screens panel calls this on
    /// navigation, never mid-render). Same-id re-points keep state.
    pub fn set_action(&mut self, action_id: String, cx: &mut gpui::Context<Self>) {
        if self.action_id.as_deref() == Some(action_id.as_str()) {
            return;
        }
        self.action_id = Some(action_id);
        self.body = None;
        self.body_error = None;
        self.fetched_updated_at = None;
        self.repos = Vec::new();
        // The scroll offset belongs to the previous action (the shared-
        // instance rule — issue detail's EXP-67 lesson).
        self.body_scroll
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
        self.fetch_body(cx);
        self.fetch_repos(cx);
        cx.notify();
    }

    /// The synced row, if visible in this team.
    fn action(&self, cx: &App) -> Option<api::actions::Action> {
        let action_id = self.action_id.as_deref()?;
        if api::actions::is_builtin_action_id(action_id) {
            return None;
        }
        Store::global(cx)
            .collections()
            .actions
            .read(cx)
            .get(action_id)
            .map(api::actions::from_row)
    }

    /// Refetch the body when the synced row's `updated_at` moved past the
    /// fetched one (edit-dialog save, remote/MCP edit).
    fn refetch_if_stale(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.action(cx) else {
            return;
        };
        if self.fetched_updated_at.is_some() && action.updated_at != self.fetched_updated_at {
            self.fetch_body(cx);
        }
    }

    /// One guarded `actions.get`: the landing closure re-checks the id so a
    /// stale response for a previous action never repaints the current one.
    fn fetch_body(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action_id) = self.action_id.clone() else {
            return;
        };
        if api::actions::is_builtin_action_id(&action_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.body_error = None;
        self.fetched_updated_at = self
            .action(cx)
            .and_then(|action| action.updated_at.clone());
        let fetch_id = action_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::get(&trpc, &fetch_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.action_id.as_deref() != Some(action_id.as_str()) {
                    return;
                }
                match result {
                    Ok(action) => {
                        this.body = Some(SharedString::from(action.body));
                        this.fetched_updated_at = action.updated_at;
                    }
                    Err(err) => {
                        this.body_error =
                            Some(SharedString::from(format!("Could not load the prompt: {err}")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The team repo rows for the Repository chip. Failure degrades to the
    /// bare "Repository" label — never blocks the screen.
    fn fetch_repos(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.action(cx).map(|action| action.team_id) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(rows) = rows {
                    this.repos = rows;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn not_found(&self, cx: &gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Action not found in this team."),
            )
            .into_any_element()
    }
}

impl Render for ActionDetailView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let ready = Store::global(cx).collections().actions.read(cx).is_ready();
        if !ready {
            // §4.1: never render "not found" off an unsynced snapshot.
            return v_flex()
                .size_full()
                .p_4()
                .gap_2()
                .child(Skeleton::new().h_4().w_48())
                .child(Skeleton::new().h_4().w_64())
                .child(Skeleton::new().h_4().w_56())
                .into_any_element();
        }
        let Some(action) = self.action(cx) else {
            return self.not_found(cx);
        };

        let muted = cx.theme().muted_foreground;
        let owner = crate::settings::is_owner(cx, &action.team_id);

        // ---- center column: name, description, prompt ----------------------
        let mut column = v_flex()
            .w_full()
            .px_4()
            .pt_3()
            .pb_6()
            .gap_3()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(action.name.clone())),
            );
        if let Some(description) = action
            .description
            .clone()
            .filter(|text| !text.trim().is_empty())
        {
            column = column.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(SharedString::from(description)),
            );
        }
        let prompt: gpui::AnyElement = if let Some(error) = self.body_error.clone() {
            div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error)
                .into_any_element()
        } else if let Some(body) = self.body.clone() {
            crate::surface::glass_card()
                .p_4()
                .child(TextView::markdown("action-body", body).selectable(true))
                .into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(muted)
                .child("Loading prompt…")
                .into_any_element()
        };
        column = column
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child("PROMPT"),
            )
            .child(prompt);

        // ---- sidebar --------------------------------------------------------
        let run_id = action.id.clone();
        let run_team = action.team_id.clone();
        let run_button = Button::new("action-detail-run")
            .primary()
            .small()
            .icon(Icon::from(ExpIcon::Play))
            .label("Run on this device")
            .on_click(move |_: &ClickEvent, window, cx| {
                crate::start_coding_dialog::open_for_action(
                    window,
                    cx,
                    run_team.clone(),
                    run_id.clone(),
                );
            });

        let repo_chip: gpui::AnyElement = match action.repository_id.as_deref() {
            Some(repo_id) => {
                let name = self
                    .repos
                    .iter()
                    .find(|row| row.id == repo_id)
                    .map(|row| row.full_name.clone())
                    .unwrap_or_else(|| "Repository".to_string());
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .child(Icon::from(ExpIcon::GitMerge).xsmall().text_color(muted))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(name)),
                    )
                    .into_any_element()
            }
            None => div()
                .text_xs()
                .text_color(muted)
                .child("No repository (scratch run)")
                .into_any_element(),
        };

        let mut sidebar = v_flex()
            .w(px(240.))
            .flex_shrink_0()
            .h_full()
            .px_3()
            .py_3()
            .gap_3()
            .text_sm()
            .child(run_button)
            .child(property_group("Repository", repo_chip, cx));

        if !action.inputs.is_empty() {
            let mut inputs = v_flex().gap_1();
            for input in &action.inputs {
                inputs = inputs.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .child(SharedString::from(input.label.clone()))
                        .child(
                            div()
                                .text_color(muted)
                                .child(SharedString::from(format!(
                                    "· {}{}",
                                    input.input_type,
                                    if input.required { " · required" } else { "" }
                                ))),
                        ),
                );
            }
            sidebar = sidebar.child(property_group("Inputs", inputs, cx));
        }
        if let Some(created) = action.created_at.as_deref() {
            sidebar = sidebar.child(property_group(
                "Created",
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(crate::inbox::relative_time(created))),
                cx,
            ));
        }
        if let Some(updated) = action.updated_at.as_deref() {
            sidebar = sidebar.child(property_group(
                "Updated",
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(crate::inbox::relative_time(updated))),
                cx,
            ));
        }
        if owner {
            let edit_action = action.clone();
            let edit_team = action.team_id.clone();
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            sidebar = sidebar.child(
                v_flex()
                    .gap_1()
                    .items_start()
                    .child(
                        Button::new("action-detail-edit")
                            .ghost()
                            .xsmall()
                            .label("Edit action…")
                            .on_click(move |_: &ClickEvent, window, cx| {
                                crate::actions_panel::open_action_editor(
                                    window,
                                    cx,
                                    edit_team.clone(),
                                    edit_action.clone(),
                                );
                            }),
                    )
                    .child(
                        Button::new("action-detail-delete")
                            .ghost()
                            .xsmall()
                            .text_color(cx.theme().danger)
                            .label("Delete action…")
                            .on_click(move |_: &ClickEvent, window, cx| {
                                crate::actions_panel::prompt_delete_action(
                                    window,
                                    cx,
                                    delete_id.clone(),
                                    delete_name.clone(),
                                );
                            }),
                    ),
            );
        }

        let body = h_flex()
            .flex_1()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .child(
                div()
                    .id("action-detail-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(centered_column(column)),
            )
            .child(sidebar);

        v_flex().size_full().child(body).into_any_element()
    }
}
