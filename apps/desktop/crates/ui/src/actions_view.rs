//! The Actions center screen (EXP-467): the web `t/$teamSlug/agents` page
//! 1:1 — "My machines" on top, then the team's reusable action prompts as a
//! wrapping CARD grid. The old master/detail split (Actions tool window →
//! full-page action detail) is gone: editing happens in
//! [`crate::action_editor_dialog`] behind each card's owner ⋯ menu, exactly
//! like the web's edit dialog, and the rail's Actions entry navigates here.
//! EXP-480: the page is a tab-less FULL-PAGE mode (no tool column, no tab
//! chip — `CenterPanel` unmounts the sidebar split while it is up), and both
//! sections lead with the web's `SectionLabel` band ([`section_band`]) —
//! "My machines" carries the "Add server" button, "Actions" the owner-only
//! "New action".
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
    switch::Switch,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::native_dialog::{self, AlertSpec};
use crate::queries;

/// The page column's width cap — the web page's `md:max-w-5xl`.
const PAGE_COLUMN_W: f32 = 1024.;

/// The web `SectionLabel` band (agent-session-row.tsx): a full-width
/// rounded-top strip — label · count · spacer · optional trailing control.
/// Shared with [`crate::machines::MachinesSection`] so both page sections
/// carry the identical header design (EXP-480).
pub(crate) fn section_band(
    label: &'static str,
    count: usize,
    trailing: Option<gpui::AnyElement>,
    cx: &App,
) -> gpui::AnyElement {
    gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .px_3()
        .py_1p5()
        .rounded_t(px(theme::tokens::radius::SM))
        .border_b_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(SharedString::from(label)),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(format!("{count}"))),
        )
        .child(div().flex_1())
        .children(trailing)
        .into_any_element()
}

/// EXP-530: the page's three tabs. **Actions** is the card grid,
/// **Automations** the dense trigger list (only actions carrying a parseable
/// trigger), **Suggestions** the curated seed cards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionsTab {
    Actions,
    Automations,
    Suggestions,
}

pub struct ActionsView {
    nav: Entity<Navigation>,
    tab: ActionsTab,
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
        // EXP-530: the Automations tab joins devices (label + online dot) and
        // coding_sessions (last / recent automated runs), so all three synced
        // collections drive this screen now.
        let watched = sync::Store::try_global(cx).map(|store| {
            let collections = store.collections();
            (
                collections.actions.clone(),
                collections.devices.clone(),
                collections.coding_sessions.clone(),
            )
        });
        if let Some((actions, devices, sessions)) = watched {
            subscriptions.push(cx.observe(&actions, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&devices, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&sessions, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            tab: ActionsTab::Actions,
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
                        .ghost().cursor_pointer()
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
            // EXP-530: an automated action says so on its card — the same
            // sentence the Automations tab and the CLI print.
            .when_some(
                crate::automation_editor::trigger_summary_for(action.trigger.as_ref()),
                |this, summary| {
                    this.child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1p5()
                            .child(
                                Icon::from(registry::ACTION_AUTOMATION)
                                    .xsmall()
                                    .text_color(muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .truncate()
                                    .text_color(muted)
                                    .child(SharedString::from(summary)),
                            ),
                    )
                },
            )
            // Spacer pins Run to the bottom on stretched (same-line) cards.
            .child(div().flex_1())
            .child(
                gpui_component::h_flex().pt_1().child(
                    Button::new(("action-run", index))
                        .outline().cursor_pointer()
                        .web_sm()
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

    /// The page's tab capsule (EXP-530) — the web segmented `TabsList`,
    /// full-width above the section (EXP-574, web parity).
    fn render_tabs(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let segment = |label: &'static str, tab: ActionsTab| {
            crate::controls::segmented_item(self.tab == tab, cx)
                .id(label)
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if this.tab != tab {
                        this.tab = tab;
                        cx.notify();
                    }
                }))
        };
        crate::controls::segmented(cx)
            .child(segment("Actions", ActionsTab::Actions))
            .child(segment("Automations", ActionsTab::Automations))
            .child(segment("Suggestions", ActionsTab::Suggestions))
            .into_any_element()
    }

    // -- Automations tab (EXP-530) ------------------------------------------

    /// One dense automation row: glyph + name, the trigger sentence, the bound
    /// device (label + online dot), the enabled toggle, and the schedule's
    /// next run / the action's last automated run.
    fn render_automation_row(
        &self,
        index: usize,
        action: &api::actions::Action,
        parsed: &coding::automations::ParsedTrigger,
        devices: &[AutomationDevice],
        sessions: &[&domain::rows::CodingSession],
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let summary = coding::automations::trigger_summary(parsed);
        // A device that isn't in this user's synced rows (a teammate's private
        // machine) keeps its raw id — the binding is still real.
        let device = devices
            .iter()
            .find(|device| device.device_id == parsed.device_id);
        let device_label = device
            .map(|device| device.label.clone())
            .unwrap_or_else(|| parsed.device_id.clone());
        let device_online = device.is_some_and(|device| device.online);

        let mut meta = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(summary))
            .child(div().child("·"))
            .when(device_online, |this| {
                this.child(
                    div()
                        .size_1p5()
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(theme::tokens::GREEN.to_hsla()),
                )
            })
            .child(SharedString::from(device_label));
        // Schedules can say when they fire next; event triggers cannot.
        if let Some(next) = crate::automation_editor::next_run_label(parsed) {
            meta = meta
                .child(div().child("·"))
                .child(SharedString::from(format!("Next {next}")));
        }
        // The most recent AUTOMATED run of this action (a manual run says
        // nothing about whether the trigger is working).
        if let Some(last) = sessions
            .iter()
            .find(|session| session.action_id.as_deref() == Some(action.id.as_str()))
        {
            meta = meta
                .child(div().child("·"))
                .child(SharedString::from(last_run_label(last)));
        }

        let toggle_id = action.id.clone();
        let toggle_trigger = action.trigger.clone();
        let enabled = parsed.enabled;
        // An automated run has nobody to type required inputs, so the server
        // refuses to ENABLE such a trigger — but a legacy row that is already
        // on must stay switchable OFF.
        let has_required = action.inputs.iter().any(|input| input.required);
        let locked = has_required && !enabled;
        gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .child(
                div().flex_shrink_0().child(
                    crate::icons::action_icon(action.icon.as_deref())
                        .xsmall()
                        .text_color(muted),
                ),
            )
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(SharedString::from(action.name.clone())),
                    )
                    .child(meta),
            )
            .child(
                // Owner-only per the permissions model: members SEE the state
                // (a disabled switch), owners flip it.
                Switch::new(("automation-enabled", index))
                    .checked(enabled)
                    .disabled(!is_owner || locked)
                    .when(locked, |this| {
                        this.tooltip(crate::automation_editor::AUTOMATION_REQUIRED_INPUTS_HINT)
                    })
                    .on_click(cx.listener(move |_, on: &bool, _, cx| {
                        let Some(trigger) = toggle_trigger.as_ref() else {
                            return;
                        };
                        spawn_trigger_enabled(cx, toggle_id.clone(), trigger, *on);
                    })),
            )
            .into_any_element()
    }

    /// The Automations tab body: one row per triggered action, then the
    /// "Recent automated runs" list.
    fn render_automations(
        &self,
        actions: &[api::actions::Action],
        team_id: Option<&str>,
        is_owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let devices = automation_devices(cx);
        let runs = automated_runs(cx, team_id);
        let run_refs: Vec<&domain::rows::CodingSession> = runs.iter().collect();

        let triggered: Vec<(usize, &api::actions::Action, coding::automations::ParsedTrigger)> =
            actions
                .iter()
                .filter_map(|action| {
                    crate::automation_editor::parsed_trigger(action.trigger.as_ref())
                        .map(|parsed| (action, parsed))
                })
                .enumerate()
                .map(|(index, (action, parsed))| (index, action, parsed))
                .collect();

        if triggered.is_empty() {
            return crate::controls::empty_state(
                Icon::from(registry::ACTION_AUTOMATION),
                "No automations yet",
                "Add a schedule or event trigger to an action.",
                cx,
            )
            .into_any_element();
        }

        let rows: Vec<gpui::AnyElement> = triggered
            .iter()
            .map(|(index, action, parsed)| {
                self.render_automation_row(
                    *index, action, parsed, &devices, &run_refs, is_owner, cx,
                )
            })
            .collect();

        let mut body = gpui_component::v_flex()
            .min_w_0()
            .gap_4()
            .child(
                gpui_component::v_flex()
                    .min_w_0()
                    .rounded(px(theme::tokens::radius::SM))
                    .border_1()
                    .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                    .children(rows),
            );

        // The cross-action run log — the answer to "did the automations fire?"
        let mut recent = gpui_component::v_flex().min_w_0().gap_2().child(section_band(
            "Recent automated runs",
            runs.len().min(RECENT_RUNS_CAP),
            None,
            cx,
        ));
        if runs.is_empty() {
            recent = recent.child(
                div()
                    .px_3()
                    .text_xs()
                    .text_color(muted)
                    .child("Nothing has fired yet."),
            );
        }
        for session in runs.iter().take(RECENT_RUNS_CAP) {
            recent = recent.child(render_run_row(session, cx));
        }
        body = body.child(recent);
        body.into_any_element()
    }

    // -- Suggestions tab (EXP-530) ------------------------------------------

    /// One suggestion card — the same grid cell shape as an action card, with
    /// "Use suggestion" opening the creator dialog prefilled.
    fn render_suggestion(
        &self,
        suggestion: &crate::action_suggestions::Suggestion,
        team_id: String,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let description = suggestion.description.to_string();
        let icon = suggestion.icon.to_string();
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        gpui_component::v_flex()
            .flex_basis(px(260.))
            .flex_grow(1.)
            .min_w(px(240.))
            .max_w(px(420.))
            .gap_2()
            .rounded(px(theme::tokens::radius::SM))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .p_3()
            .child(
                gpui_component::h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_shrink_0().child(
                            crate::icons::action_icon(Some(suggestion.icon))
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
                            .child(SharedString::from(suggestion.title)),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_xs()
                    .text_color(muted)
                    .line_clamp(4)
                    .child(SharedString::from(suggestion.description)),
            )
            .child(div().flex_1())
            .child(
                gpui_component::h_flex().pt_1().child(
                    // Keyed by the stable seed id, not the render index.
                    Button::new(SharedString::from(format!(
                        "action-suggestion-{}",
                        suggestion.id
                    )))
                        .outline().cursor_pointer()
                        .web_sm()
                        .icon(Icon::from(registry::ACTION_CREATE))
                        .label("Use suggestion")
                        .tooltip(
                            no_agent
                                .clone()
                                .unwrap_or_else(|| "Author this action".into()),
                        )
                        .disabled(no_agent.is_some())
                        .on_click(move |_: &ClickEvent, window, cx| {
                            // The brief is a SEED, not a commitment — the
                            // creator dialog opens with it in the editable
                            // Description field.
                            crate::start_coding_dialog::open_for_create_action_prefilled(
                                window,
                                cx,
                                team_id.clone(),
                                Some(description.clone()),
                                Some(icon.clone()),
                            );
                        }),
                ),
            )
            .into_any_element()
    }

    /// The web `NoCustomActionsNudge`: a dashed tile that opens the creator
    /// run, shown while every listed action is a builtin. A GRID CELL like
    /// the web's (same sizing as the cards), not a full-width strip.
    fn render_nudge(&self, team_id: String, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().list_hover;
        div()
            .id("actions-empty-nudge")
            .flex_basis(px(260.))
            .flex_grow(1.)
            .min_w(px(240.))
            .max_w(px(420.))
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
        let automation_count = actions
            .iter()
            .filter(|action| {
                crate::automation_editor::parsed_trigger(action.trigger.as_ref()).is_some()
            })
            .count();

        // Owner-only "New action" (EXP-367: disabled with the reason when no
        // agent CLI is installed, never hidden).
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let new_action = is_owner
            .then(|| team_id.clone())
            .flatten()
            .map(|new_team| {
                Button::new("actions-new")
                    .outline().cursor_pointer()
                    .web_xs()
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
                    })
                    .into_any_element()
            });
        // EXP-574 (web parity): the tab capsule spans the page above the
        // section; the band heads only the tab that has one on the web —
        // Actions ("Actions" · count · "New action"), Automations
        // ("Automations" · count). Suggestions renders its grid bare.
        let tabs = self.render_tabs(cx);
        let header = match self.tab {
            ActionsTab::Actions => Some(section_band(
                "Actions",
                actions.len(),
                new_action,
                cx,
            )),
            ActionsTab::Automations => {
                Some(section_band("Automations", automation_count, None, cx))
            }
            ActionsTab::Suggestions => None,
        };

        // The nudge is a grid CELL (web parity) — appended after the cards.
        let nudge = (!loading && is_owner && self.tab == ActionsTab::Actions && !has_custom)
            .then(|| team_id.clone())
            .flatten()
            .map(|team_id| self.render_nudge(team_id, cx));

        // The cards render as soon as rows hydrate; readiness only appends
        // the loading note (the tool-window list's behavior — never blank a
        // list that already has data).
        // NO `w_full` on the column's children (EXP-508): a percent width
        // here resolves against the UNCLAMPED ancestor available width — at
        // panels wider than the grid's unwrapped line (~1650px) the wrap
        // grid stops wrapping and runs off the window, and the machines
        // section shrink-wraps (the EXP-436 leak; EXP-179 has the same
        // drop-`w_full` fix). Auto width + the column's flex-col stretch
        // resolve the capped column width at every panel width.
        let body = match self.tab {
            ActionsTab::Actions => {
                let cards: Vec<gpui::AnyElement> = actions
                    .iter()
                    .enumerate()
                    .map(|(index, action)| self.render_card(index, action, is_owner, cx))
                    .collect();
                gpui_component::h_flex()
                    .min_w_0()
                    .flex_wrap()
                    .items_stretch()
                    .gap_3()
                    .children(cards)
                    .children(nudge)
                    .into_any_element()
            }
            ActionsTab::Automations => {
                self.render_automations(&actions, team_id.as_deref(), is_owner, cx)
            }
            ActionsTab::Suggestions => {
                let cards: Vec<gpui::AnyElement> = match team_id.clone() {
                    Some(team_id) => crate::action_suggestions::ACTION_SUGGESTIONS
                        .iter()
                        .map(|suggestion| self.render_suggestion(suggestion, team_id.clone(), cx))
                        .collect(),
                    None => Vec::new(),
                };
                gpui_component::h_flex()
                    .min_w_0()
                    .flex_wrap()
                    .items_stretch()
                    .gap_3()
                    .children(cards)
                    .into_any_element()
            }
        };
        let mut actions_section = gpui_component::v_flex()
            .min_w_0()
            .gap_2()
            .child(tabs)
            .children(header)
            .child(body);
        // Only the Actions tab is fed by the shape's readiness.
        if loading && self.tab == ActionsTab::Actions {
            actions_section = actions_section.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Loading actions…"),
            );
        }

        // Web page order: machines first, then the Actions grid — one
        // centered column (block wrapper + `mx_auto`, the EXP-179-safe
        // centering recipe) inside the one scroll pane. `min_w_0` rides
        // every flex hop so the wrap grid resolves the PAGE width, never its
        // unwrapped-line max-content (the EXP-436 class — the cards used to
        // run off the right edge in one uncut line).
        let column = gpui_component::v_flex()
            .w_full()
            .min_w_0()
            .px_4()
            .py_4()
            .gap_4()
            .child(self.machines.clone())
            .child(actions_section);

        gpui_component::v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "actions-screen-scroll",
                &self.scroll,
                div()
                    .w_full()
                    .min_w_0()
                    .child(column.w_full().max_w(px(PAGE_COLUMN_W)).mx_auto()),
            ))
    }
}

/// How many rows the "Recent automated runs" list shows before it stops.
const RECENT_RUNS_CAP: usize = 10;

/// One synced device, reduced to what an automation row shows.
struct AutomationDevice {
    device_id: String,
    label: String,
    online: bool,
}

/// The synced devices, keyed by their steer id — the Automations list resolves
/// each trigger's `deviceId` through this. Unlike the editor's picker this is
/// NOT cap-filtered: a device that stopped advertising `automations` must
/// still render its label on the trigger bound to it.
fn automation_devices(cx: &App) -> Vec<AutomationDevice> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Vec::new();
    };
    let collection = store.collections().devices.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    collection
        .read(cx)
        .iter()
        .filter_map(|row| {
            let device_id = row.device_id.clone().filter(|id| !id.is_empty())?;
            Some(AutomationDevice {
                label: row.label.clone().unwrap_or_else(|| device_id.clone()),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                device_id,
            })
        })
        .collect()
}

/// This team's AUTOMATION-started runs, newest first. `started_reason` is the
/// discriminator the server stamps — a manually started run of the same action
/// never appears here.
fn automated_runs(cx: &App, team_id: Option<&str>) -> Vec<domain::rows::CodingSession> {
    let (Some(store), Some(team_id)) = (sync::Store::try_global(cx), team_id) else {
        return Vec::new();
    };
    let collection = store.collections().coding_sessions.clone();
    let mut runs: Vec<domain::rows::CodingSession> = collection
        .read(cx)
        .iter()
        .filter(|session| session.team_id.as_deref() == Some(team_id))
        .filter(|session| session.started_reason.is_some())
        .cloned()
        .collect();
    // ISO-8601 sorts lexicographically — newest first.
    runs.sort_by(|a, b| {
        run_started_at(b)
            .cmp(&run_started_at(a))
            .then_with(|| b.id.cmp(&a.id))
    });
    runs
}

fn run_started_at(session: &domain::rows::CodingSession) -> Option<&str> {
    session
        .started_at
        .as_deref()
        .or(session.created_at.as_deref())
}

/// "Last run ended, 2 hours ago" — the status word plus when it started.
fn last_run_label(session: &domain::rows::CodingSession) -> String {
    let status = session.status.clone().unwrap_or_else(|| "unknown".to_string());
    match run_started_at(session) {
        Some(at) => {
            let when = crate::comments::relative_time(at, chrono::Utc::now().timestamp());
            format!("Last run {status}, {when}")
        }
        None => format!("Last run {status}"),
    }
}

/// One "Recent automated runs" row: the action's name snapshot, an "Automated"
/// badge, and the run's status + age.
fn render_run_row(session: &domain::rows::CodingSession, cx: &App) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let name = session
        .action_name
        .clone()
        // The snapshot survives the action's deletion; only a pre-EXP-253 row
        // could lack it.
        .unwrap_or_else(|| "Action".to_string());
    let status = session.status.clone().unwrap_or_else(|| "unknown".to_string());
    let when = run_started_at(session)
        .map(|at| crate::comments::relative_time(at, chrono::Utc::now().timestamp()))
        .unwrap_or_default();
    // `schedule` / `event` — why it fired.
    let reason = session.started_reason.clone().unwrap_or_default();
    gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .px_3()
        .py_1p5()
        .child(
            Icon::from(registry::ACTION_AUTOMATION)
                .xsmall()
                .text_color(muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .text_color(theme.foreground)
                .child(SharedString::from(name)),
        )
        .child(
            div()
                .flex_shrink_0()
                .px_1p5()
                .py_0p5()
                .rounded(px(theme::tokens::radius::SM))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(if reason.is_empty() {
                    "Automated".to_string()
                } else {
                    format!("Automated · {reason}")
                })),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(format!("{status} · {when}"))),
        )
        .into_any_element()
}

/// Flip an automation's `enabled` flag through `actions.update` (EXP-530).
/// ONLY that key moves — every other field of the trigger is round-tripped
/// verbatim, so toggling never re-seeds the host's automation state.
fn spawn_trigger_enabled(
    cx: &mut App,
    action_id: String,
    trigger: &serde_json::Value,
    enabled: bool,
) {
    let Some(next) = crate::automation_editor::with_enabled(trigger, enabled) else {
        return;
    };
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let mut input = api::actions::ActionUpdate::new(action_id);
    input.trigger = api::Patch::Set(next);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::actions::update(&trpc, &input).map(|_| ()) })
            .await;
        let _ = cx.update(|_| {
            // The synced echo repaints the switch — a failure just leaves it
            // where it was.
            if let Err(err) = result {
                log::warn!("actions: enabling/disabling the automation failed: {err}");
            }
        });
    })
    .detach();
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
