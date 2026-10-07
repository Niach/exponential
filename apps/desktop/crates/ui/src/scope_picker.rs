//! FEED-76 — the team/board scope picker, the web `@exp/ui` `ScopePicker`
//! twin: what a credential may touch. An "Everything" switch (all teams and
//! boards, including ones created later) over the member's teams, each a
//! whole-team checkbox with its boards indented beneath — a board under a
//! ticked team reads checked and disabled because the team covers it. The
//! Create-API-key dialog (`settings::api_keys`) shows it; the MCP OAuth
//! consent screen is web-only mid-authorize, so the IDE has the one caller.
//!
//! The RAW selection lives here; what is SENT is [`effective_scope_selection`]
//! — boards a selected whole team already covers drop out, "Everything"
//! clears both lists — applied at submit, so unticking a team restores the
//! boards the user had ticked under it. The wire shape is
//! `api::users::ScopeSelection`, byte for byte the web's `scope` input.

use gpui::{
    div, prelude::FluentBuilder as _, px, App, Context, Div, ElementId, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

pub(crate) use api::users::ScopeSelection;

use crate::controls::{checkbox, web_switch, CheckState};
use crate::surface::{glass_group, glass_group_rows};

/// One board under its team.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeBoard {
    pub id: String,
    pub name: String,
    pub prefix: String,
}

/// One team with its live boards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeTeam {
    pub id: String,
    pub name: String,
    pub boards: Vec<ScopeBoard>,
}

impl ScopeTeam {
    /// The member's synced teams (name-sorted) with their boards in the
    /// sidebar's order — the trash/archive never reach the boards shape, so
    /// this is the same live tree the web's `mcpGrants.scopeTree` serves.
    /// Needs the installed `sync::Store` (never the styleguide).
    pub(crate) fn from_store(cx: &App) -> Vec<Self> {
        let collections = sync::Store::global(cx).collections();
        collections
            .teams_sorted(cx)
            .into_iter()
            .map(|team| Self {
                boards: collections
                    .boards_in_team(&team.id, cx)
                    .into_iter()
                    .map(|board| ScopeBoard {
                        id: board.id,
                        name: board.name,
                        prefix: board.prefix.unwrap_or_default(),
                    })
                    .collect(),
                id: team.id,
                name: team.name,
            })
            .collect()
    }
}

/// What to send: boards covered by a selected whole team are dropped; an
/// all-teams pick carries no ids at all (web `effectiveScopeSelection`).
pub(crate) fn effective_scope_selection(
    tree: &[ScopeTeam],
    value: &ScopeSelection,
) -> ScopeSelection {
    if value.all_teams {
        return ScopeSelection::everything();
    }
    let covered: Vec<&str> = tree
        .iter()
        .filter(|team| value.team_ids.iter().any(|id| id == &team.id))
        .flat_map(|team| team.boards.iter().map(|board| board.id.as_str()))
        .collect();
    ScopeSelection {
        all_teams: false,
        team_ids: value.team_ids.clone(),
        board_ids: value
            .board_ids
            .iter()
            .filter(|id| !covered.contains(&id.as_str()))
            .cloned()
            .collect(),
    }
}

/// The web `toggle`: add once / remove, order kept.
fn toggle(list: &mut Vec<String>, id: &str, on: bool) {
    let present = list.iter().any(|item| item == id);
    if on && !present {
        list.push(id.to_string());
    } else if !on {
        list.retain(|item| item != id);
    }
}

/// The copy, the web's byte for byte.
pub(crate) mod copy {
    pub(crate) const EVERYTHING: &str = "Everything";
    pub(crate) const EVERYTHING_HINT: &str =
        "All teams and boards, including ones created later.";
    pub(crate) const WHOLE_TEAM: &str = "whole team";
    pub(crate) const NO_TEAMS: &str = "You aren't a member of any team yet.";
    pub(crate) const NO_BOARDS: &str = "No boards yet.";
}

/// The tree's scroll cap (web `max-h-72`).
pub(crate) const TREE_MAX_HEIGHT: f32 = 288.;

pub(crate) struct ScopePicker {
    /// Prefix for the element ids when two pickers share a window.
    id_prefix: &'static str,
    tree: Vec<ScopeTeam>,
    value: ScopeSelection,
    /// A caller's line under the tree while a scope is being picked (the
    /// key dialog's "MCP endpoint only" note); hidden on Everything.
    scoped_note: Option<SharedString>,
    /// A refusal to show under the control (an empty pick on submit);
    /// cleared by the next change.
    error: Option<SharedString>,
    tree_max_height: Pixels,
    scroll: ScrollHandle,
}

impl ScopePicker {
    pub(crate) fn new(id_prefix: &'static str, tree: Vec<ScopeTeam>, value: ScopeSelection) -> Self {
        Self {
            id_prefix,
            tree,
            value,
            scoped_note: None,
            error: None,
            tree_max_height: px(TREE_MAX_HEIGHT),
            scroll: ScrollHandle::new(),
        }
    }

    pub(crate) fn scoped_note(mut self, note: impl Into<SharedString>) -> Self {
        self.scoped_note = Some(note.into());
        self
    }

    pub(crate) fn tree_max_height(mut self, height: Pixels) -> Self {
        self.tree_max_height = height;
        self
    }

    /// The pick to SEND (see [`effective_scope_selection`]).
    pub(crate) fn effective(&self) -> ScopeSelection {
        effective_scope_selection(&self.tree, &self.value)
    }

    pub(crate) fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    fn set_all_teams(&mut self, on: bool, cx: &mut Context<Self>) {
        self.value.all_teams = on;
        self.error = None;
        cx.notify();
    }

    fn set_team(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        toggle(&mut self.value.team_ids, id, on);
        self.error = None;
        cx.notify();
    }

    fn set_board(&mut self, id: &str, on: bool, cx: &mut Context<Self>) {
        toggle(&mut self.value.board_ids, id, on);
        self.error = None;
        cx.notify();
    }

    fn element_id(&self, kind: &str, id: &str) -> ElementId {
        ElementId::from(SharedString::from(format!("{}-{kind}-{id}", self.id_prefix)))
    }

    /// The "Everything" row: label + hint, the switch trailing.
    fn render_everything(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let row = h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .justify_between()
            .gap_3()
            .px_3()
            .py_2p5()
            .child(
                v_flex()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(copy::EVERYTHING))
                    .child(div().text_xs().text_color(muted).child(copy::EVERYTHING_HINT)),
            )
            .child(
                web_switch(self.element_id("all-teams", "switch"))
                    .checked(self.value.all_teams)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.set_all_teams(*checked, cx);
                    })),
            );
        glass_group_rows(vec![row])
    }

    /// One checkbox line: the whole row is the click target (the web
    /// `Label htmlFor`), a disabled one is inert.
    fn check_line(
        &self,
        kind: &str,
        key: &str,
        state: CheckState,
        disabled: bool,
        on_toggle: impl Fn(&mut Self, bool, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let next = state != CheckState::Checked;
        let box_id = self.element_id(&format!("{kind}-box"), key);
        div()
            .id(self.element_id(kind, key))
            .flex()
            .items_center()
            .gap_2()
            .when(!disabled, |line| {
                line.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                    on_toggle(this, next, cx);
                }))
            })
            .child(checkbox(box_id, state, disabled, cx))
    }

    fn render_team(&self, team: &ScopeTeam, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let whole_team = self.value.team_ids.iter().any(|id| id == &team.id);
        let team_id = team.id.clone();
        let header = self
            .check_line(
                "team",
                &team.id,
                whole_team.into(),
                false,
                move |this, on, cx| this.set_team(&team_id, on, cx),
                cx,
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(team.name.clone())),
            )
            .child(div().text_xs().text_color(muted).child(copy::WHOLE_TEAM));

        let mut boards = v_flex().ml_6().gap_1();
        for board in &team.boards {
            let selected = self.value.board_ids.iter().any(|id| id == &board.id);
            let board_id = board.id.clone();
            boards = boards.child(
                self.check_line(
                    "board",
                    &board.id,
                    (whole_team || selected).into(),
                    whole_team,
                    move |this, on, cx| this.set_board(&board_id, on, cx),
                    cx,
                )
                .child(
                    h_flex()
                        .items_baseline()
                        .gap_1p5()
                        .child(div().text_sm().child(SharedString::from(board.name.clone())))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(board.prefix.clone())),
                        ),
                ),
            );
        }
        if team.boards.is_empty() {
            boards = boards.child(div().text_xs().text_color(muted).child(copy::NO_BOARDS));
        }
        v_flex().gap_1p5().child(header).child(boards)
    }

    /// The team list under a capped scroll — only while Everything is off.
    fn render_tree(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut list = v_flex().w_full().gap_3().p_3();
        if self.tree.is_empty() {
            list = list.child(div().text_sm().text_color(muted).child(copy::NO_TEAMS));
        }
        for team in &self.tree {
            list = list.child(self.render_team(team, cx));
        }
        glass_group().child(crate::scroll_pane::capped_v_scroll(
            self.element_id("tree", "scroll"),
            &self.scroll,
            self.tree_max_height,
            list,
        ))
    }
}

impl Render for ScopePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .child(self.render_everything(cx))
            .when(!self.value.all_teams, |this| {
                this.child(self.render_tree(cx)).children(
                    self.scoped_note
                        .clone()
                        .map(|note| div().text_xs().text_color(muted).child(note)),
                )
            })
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_sm().text_color(danger).child(error)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web test's tree.
    pub(crate) fn tree() -> Vec<ScopeTeam> {
        vec![
            ScopeTeam {
                id: "t-acme".into(),
                name: "Acme".into(),
                boards: vec![
                    ScopeBoard { id: "b-web".into(), name: "Web".into(), prefix: "WEB".into() },
                    ScopeBoard { id: "b-mob".into(), name: "Mobile".into(), prefix: "MOB".into() },
                ],
            },
            ScopeTeam { id: "t-lab".into(), name: "Lab".into(), boards: vec![] },
        ]
    }

    fn scoped(team_ids: &[&str], board_ids: &[&str]) -> ScopeSelection {
        ScopeSelection {
            all_teams: false,
            team_ids: team_ids.iter().map(|id| id.to_string()).collect(),
            board_ids: board_ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn effective_drops_boards_a_selected_whole_team_already_covers() {
        assert_eq!(
            effective_scope_selection(&tree(), &scoped(&["t-acme"], &["b-web"])),
            scoped(&["t-acme"], &[])
        );
    }

    #[test]
    fn effective_keeps_boards_outside_the_selected_teams() {
        assert_eq!(
            effective_scope_selection(&tree(), &scoped(&["t-lab"], &["b-web"])),
            scoped(&["t-lab"], &["b-web"])
        );
    }

    #[test]
    fn everything_carries_no_ids_at_all() {
        let value = ScopeSelection {
            all_teams: true,
            team_ids: vec!["t-acme".into()],
            board_ids: vec!["b-web".into()],
        };
        assert_eq!(effective_scope_selection(&tree(), &value), ScopeSelection::everything());
    }

    #[test]
    fn toggle_adds_once_and_removes() {
        let mut list = Vec::new();
        toggle(&mut list, "a", true);
        toggle(&mut list, "a", true);
        toggle(&mut list, "b", true);
        assert_eq!(list, vec!["a".to_string(), "b".to_string()]);
        toggle(&mut list, "a", false);
        assert_eq!(list, vec!["b".to_string()]);
        toggle(&mut list, "zzz", false);
        assert_eq!(list, vec!["b".to_string()]);
    }

    #[test]
    fn unticking_a_team_restores_the_boards_ticked_under_it() {
        let mut picker = ScopePicker::new("t", tree(), scoped(&["t-acme"], &["b-web"]));
        // Raw keeps the covered board; the effective pick drops it …
        assert_eq!(picker.effective(), scoped(&["t-acme"], &[]));
        // … and un-ticking the team brings it back into the pick.
        toggle(&mut picker.value.team_ids, "t-acme", false);
        assert_eq!(picker.effective(), scoped(&[], &["b-web"]));
        assert!(picker.effective().has_selection());
        toggle(&mut picker.value.board_ids, "b-web", false);
        assert!(!picker.effective().has_selection());
    }
}
