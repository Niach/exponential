//! The Actions center screen (EXP-467): the web `t/$teamSlug/agents` page
//! 1:1 — "My machines" on top, then the team's reusable action prompts as a
//! wrapping CARD grid. The old master/detail split (Actions tool window →
//! full-page action detail) is gone: editing happens in
//! [`crate::action_editor_dialog`] behind each card's owner ⋯ menu, exactly
//! like the web's edit dialog, and the rail's Actions entry navigates here.
//!
//! EXP-431 carries over: the create builtin is not a card — creation lives
//! behind the header's "New action" button
//! ([`crate::start_coding_dialog::open_for_create_action`]), and only the
//! fix-conflicts builtin stays pinned first. The list is LIVE off the synced
//! `actions` shape (body-less rows; the edit dialog fetches the body via
//! `actions.get` on open). ▶ Run opens the unified Start-coding dialog's
//! Actions tab, which owns agent/model/effort and the typed input fields.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, FontWeight, InteractiveElement,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// The page column's width cap — the web page's `md:max-w-5xl`.
const PAGE_COLUMN_W: f32 = 1024.;

pub struct ActionsView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// EXP-403: the same per-user device registry web/iOS/Android show,
    /// leading the page like the web's MyMachines section. It owns its own
    /// `devices.list` poll and only polls while rendered — i.e. while this
    /// screen's tab is visible.
    machines: Entity<crate::machines::MachinesSection>,
    /// `repositories.list` rows for the card repo badges, keyed by the team
    /// they belong to (a team switch refetches).
    repos: Option<(String, Vec<crate::action_run::ActionRepoRow>)>,
    /// The team the current fetch belongs to (set before the spawn so a
    /// render storm can't stack requests).
    repos_key: Option<String>,
    /// Bumped per fetch — a stale response checks it before landing.
    repos_seq: u64,
    _subscriptions: Vec<Subscription>,
}

impl ActionsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let machines = cx.new(|cx| crate::machines::MachinesSection::new(window, cx));
        // Live list: re-render on any synced actions change (EXP-268) and on
        // navigation (team switch re-scopes the read).
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        let actions_collection =
            sync::Store::try_global(cx).map(|store| store.collections().actions.clone());
        if let Some(collection) = actions_collection {
            subscriptions.push(cx.observe(&collection, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            scroll: ScrollHandle::new(),
            machines,
            repos: None,
            repos_key: None,
            repos_seq: 0,
            _subscriptions: subscriptions,
        }
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    /// Fetch the team's repos once per team — the card badges only need the
    /// id → fullName join. A failed fetch degrades to badge-less cards.
    fn ensure_repos(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        if self.repos_key.as_deref() == Some(team_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.repos_key = Some(team_id.to_string());
        self.repos_seq += 1;
        let seq = self.repos_seq;
        let team = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let fetch_team = team.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::action_run::fetch_repositories(&trpc, &fetch_team) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.repos_seq != seq {
                    return;
                }
                match result {
                    Ok(rows) => {
                        this.repos = Some((team, rows));
                        cx.notify();
                    }
                    Err(err) => log::warn!("actions: repositories.list failed: {err}"),
                }
            });
        })
        .detach();
    }

    /// ▶ Run — open the unified Start-coding dialog's Actions tab with this
    /// action preselected (EXP-257: the dialog owns agent/model/effort and
    /// the typed input fields).
    fn run(&mut self, action_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        // EXP-367: belt to the disabled button — no agent CLI, nothing to run.
        if crate::coding_flow::no_agent_reason(cx).is_some() {
            return;
        }
        crate::start_coding_dialog::open_for_action(window, cx, team_id, action_id);
    }

    /// Destructive native actions confirm first — the web delete dialog's
    /// copy behind the shared alert window (the machines Remove pattern).
    fn prompt_delete(
        &mut self,
        action_id: String,
        name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let spec = AlertSpec::new(
            "Delete action",
            format!(
                "Delete \"{name}\"? Live runs keep going and keep their label; \
                 this cannot be undone."
            ),
            "Delete",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            spawn_action_delete(cx, action_id.clone());
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render -------------------------------------------------------------

    /// One action card — the web `ActionCard` shape: [glyph · name · ⋯],
    /// repo badge, 2-line description, Run pinned to the bottom edge.
    fn render_card(
        &self,
        index: usize,
        action: &api::actions::Action,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let run_id = action.id.clone();
        let repo_name = action.repository_id.as_deref().and_then(|repo_id| {
            self.repos.as_ref().and_then(|(_, rows)| {
                rows.iter()
                    .find(|row| row.id == repo_id)
                    .map(|row| row.full_name.clone())
            })
        });

        let mut title_row = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .child(
                div().flex_shrink_0().child(
                    crate::icons::action_icon(action.icon.as_deref())
                        .xsmall()
                        .text_color(muted),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(action.name.clone())),
            );
        // Web parity: the ⋯ menu renders only for owners on non-builtin
        // cards — builtins have no editable row and no delete.
        if is_owner && !action.builtin {
            let edit_id = action.id.clone();
            let delete_view = cx.entity().downgrade();
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            title_row = title_row.child(
                div().flex_shrink_0().child(
                    Button::new(("action-menu", index))
                        .ghost()
                        .xsmall()
                        .icon(Icon::from(registry::UI_MORE))
                        .dropdown_menu(move |menu, _window, _cx| {
                            let edit_id = edit_id.clone();
                            let delete_view = delete_view.clone();
                            let delete_id = delete_id.clone();
                            let delete_name = delete_name.clone();
                            menu.item(
                                PopupMenuItem::new("Edit")
                                    .icon(Icon::from(registry::UI_EDIT))
                                    .on_click(move |_, window, cx| {
                                        crate::action_editor_dialog::open(
                                            window,
                                            cx,
                                            edit_id.clone(),
                                        );
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Delete")
                                    .icon(Icon::from(registry::UI_DELETE))
                                    .on_click(move |_, window, cx| {
                                        let Some(view) = delete_view.upgrade() else {
                                            return;
                                        };
                                        let id = delete_id.clone();
                                        let name = delete_name.clone();
                                        view.update(cx, |this, cx| {
                                            this.prompt_delete(id, name, window, cx);
                                        });
                                    }),
                            )
                        }),
                ),
            );
        }

        // EXP-367: no agent CLI → Run disabled with the reason, never hidden.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        gpui_component::v_flex()
            // The wrap-grid cell: grow to share the line, capped so a lone
            // last-row card can't span the whole page (CSS-grid-ish).
            .flex_basis(px(260.))
            .flex_grow(1.)
            .min_w(px(240.))
            .max_w(px(420.))
            .gap_2()
            .rounded(px(theme::tokens::radius::SM))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .p_3()
            .child(title_row)
            .when_some(repo_name, |this, repo_name| {
                this.child(
                    gpui_component::h_flex().child(
                        gpui_component::h_flex()
                            .gap_1()
                            .px_1p5()
                            .py_0p5()
                            .rounded(px(theme::tokens::radius::SM))
                            .border_1()
                            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                            .child(Icon::from(registry::UI_GITHUB).xsmall().text_color(muted))
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(theme::terminal::FONT_FAMILY)
                                    .text_color(muted)
                                    .child(SharedString::from(repo_name)),
                            ),
                    ),
                )
            })
            .when_some(action.description.clone(), |this, description| {
                this.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .line_clamp(2)
                        .child(SharedString::from(description)),
                )
            })
            // Spacer pins Run to the bottom on stretched (same-line) cards.
            .child(div().flex_1())
            .child(
                gpui_component::h_flex().pt_1().child(
                    Button::new(("action-run", index))
                        .outline()
                        .small()
                        .icon(Icon::from(ExpIcon::Play))
                        .label("Run")
                        .tooltip(
                            no_agent
                                .clone()
                                .unwrap_or_else(|| "Run on this device".into()),
                        )
                        .disabled(no_agent.is_some())
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.run(run_id.clone(), window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    /// The web `NoCustomActionsNudge`: a dashed tile that opens the creator
    /// run, shown while every listed action is a builtin.
    fn render_nudge(&self, team_id: String, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().list_hover;
        div()
            .id("actions-empty-nudge")
            .w_full()
            .rounded(px(theme::tokens::radius::SM))
            .border_1()
            .border_dashed()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .p_3()
            .cursor_pointer()
            .hover(move |this| this.bg(hover))
            .on_click(move |_: &ClickEvent, window, cx| {
                crate::start_coding_dialog::open_for_create_action(window, cx, team_id.clone());
            })
            .child(
                gpui_component::v_flex()
                    .gap_1()
                    .child(
                        gpui_component::h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::from(registry::ACTION_CREATE)
                                    .xsmall()
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("No custom actions yet"),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Describe one and your agent will build it."),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let team_id = self.team_id(cx);
        if let Some(team_id) = team_id.as_deref() {
            let team_id = team_id.to_string();
            self.ensure_repos(&team_id, cx);
        }
        let (mut actions, ready) = match team_id.as_deref() {
            Some(team_id) => queries::team_actions(cx, team_id),
            None => (Vec::new(), true),
        };
        // EXP-431: the create builtin is not a card — it lives behind the
        // header's "New action" button. Filtered HERE, not in `team_actions`:
        // the Start-coding dialog's pool must keep it or its preselect
        // dead-ends in `select_action`.
        actions.retain(|action| action.id != api::actions::BUILTIN_CREATE_ACTION_ID);
        let loading = !ready;
        let is_owner = team_id
            .as_deref()
            .is_some_and(|team_id| crate::settings::is_owner(cx, team_id));
        let has_custom = actions.iter().any(|action| !action.builtin);
        let count = actions.len();

        let cards: Vec<gpui::AnyElement> = actions
            .iter()
            .enumerate()
            .map(|(index, action)| self.render_card(index, action, is_owner, cx))
            .collect();

        // Section header — web's SectionLabel row: label · count · spacer ·
        // owner-only "New action" (EXP-367: disabled with the reason when no
        // agent CLI is installed, never hidden).
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let mut header = gpui_component::h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(crate::issue_header::group_label("Actions", cx))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format!("{count}"))),
            )
            .child(div().flex_1());
        if is_owner {
            if let Some(new_team) = team_id.clone() {
                header = header.child(
                    Button::new("actions-new")
                        .outline()
                        .xsmall()
                        .icon(Icon::from(registry::ACTION_CREATE))
                        .label("New action")
                        .tooltip(no_agent.clone().unwrap_or_else(|| "New action".into()))
                        .disabled(no_agent.is_some())
                        .on_click(move |_, window, cx| {
                            crate::start_coding_dialog::open_for_create_action(
                                window,
                                cx,
                                new_team.clone(),
                            );
                        }),
                );
            }
        }

        // The cards render as soon as rows hydrate; readiness only appends
        // the loading note (the tool-window list's behavior — never blank a
        // list that already has data).
        let mut actions_section = gpui_component::v_flex()
            .w_full()
            .px_3()
            .pt_2()
            .gap_3()
            .child(header)
            .child(
                gpui_component::h_flex()
                    .w_full()
                    .flex_wrap()
                    .items_stretch()
                    .gap_3()
                    .children(cards),
            );
        if loading {
            actions_section = actions_section.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Loading actions…"),
            );
        } else if is_owner && !has_custom {
            if let Some(team_id) = team_id.clone() {
                actions_section = actions_section.child(self.render_nudge(team_id, cx));
            }
        }

        // Web page order: machines first, then the Actions grid — one
        // centered column (block wrapper + `mx_auto`, the EXP-179-safe
        // centering recipe) inside the one scroll pane.
        let column = gpui_component::v_flex()
            .w_full()
            .py_2()
            .gap_2()
            .child(self.machines.clone())
            .child(actions_section);

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "actions-screen-scroll",
                &self.scroll,
                div()
                    .w_full()
                    .child(column.max_w(px(PAGE_COLUMN_W)).mx_auto()),
            ))
    }
}

/// `actions.delete` over tRPC — the synced collection drops the row; a live
/// run keeps going and keeps its label.
pub(crate) fn spawn_action_delete(cx: &mut App, action_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::actions::delete(&trpc, &action_id) })
            .await;
        let _ = cx.update(|_| {
            if let Err(err) = result {
                log::warn!("actions: delete failed: {err}");
            }
        });
    })
    .detach();
}
