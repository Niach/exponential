//! #EXP-1715 (composer-menu.json `teamsPick`, web `team-pick.tsx`): the
//! Assistant's TEAMS pick, which teams a turn may touch (`sessions.send`
//! `scopeTeamIds`). The "+" › Teams submenu = THE search list of multi picker
//! rows over the viewer's teams in the switcher's order (the team mark + its
//! name, the check trailing); `None` = every team. While the pick is a
//! SUBSET, the picked teams ride as removable pill chips in the composer's
//! strip.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, SharedString, Window};
use gpui_component::{h_flex, ActiveTheme as _, Disableable as _};

use crate::icons::registry;
use crate::picker::search_list;
use crate::surface::{glass_pill, PillMode, PillSize};

/// `teamsPick.copy.all`.
pub(crate) const COPY_ALL: &str = "All teams";
/// `teamsPick.copy.many` (`{n}` = the count).
pub(crate) const COPY_MANY: &str = "{n} teams";

/// A subject chip's cap (web `max-w-[16rem]`).
const CHIP_MAX_W: f32 = 256.;

/// One team the pick lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickTeam {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon_url: Option<String>,
}

/// The stored pick: `None` = every team.
pub(crate) type TeamsPick = Option<Vec<String>>;

/// A changed pick handed back to the composer.
pub(crate) type OnTeamsPick = Rc<dyn Fn(TeamsPick, &mut Window, &mut App)>;

/// The viewer's teams in the switcher's order (`teams_sorted`; a personal
/// team never syncs).
pub(crate) fn pick_teams(cx: &App) -> Vec<PickTeam> {
    sync::Store::try_global(cx)
        .map(|store| {
            store
                .collections()
                .teams_sorted(cx)
                .into_iter()
                .map(|team| PickTeam { id: team.id, name: team.name, icon_url: team.icon_url })
                .collect()
        })
        .unwrap_or_default()
}

/// The `teams` condition: the viewer belongs to more than one team.
pub(crate) fn several_teams(cx: &App) -> bool {
    sync::Store::try_global(cx).is_some_and(|store| store.collections().teams.read(cx).iter().nth(1).is_some())
}

/// The `assistant` condition: `agent` is the viewer's own Assistant.
pub(crate) fn is_my_assistant(agent: &domain::rows::AgentDefRow, cx: &App) -> bool {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    agent.is_assistant() && me.is_some() && agent.owner_id == me
}

/// The "+" › Teams row: the value (`teams_pick_value`) and THE search list
/// submenu over [`teams_source`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn teams_submenu(
    menu: gpui_component::menu::PopupMenu,
    id: &'static str,
    state: Rc<dyn Fn(&App) -> (Vec<PickTeam>, TeamsPick)>,
    on_change: OnTeamsPick,
    root: gpui::WeakEntity<gpui_component::menu::PopupMenu>,
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    let (teams, picked) = state(cx);
    let value = teams_pick_value(picked.as_deref(), &teams);
    menu.submenu_with_value(
        Some(gpui_component::Icon::new(registry::UI_TEAM)),
        crate::chat_screen::MENU_TEAMS,
        Some(value),
        window,
        cx,
        move |sub, window, cx| {
            search_list::menu_search_list_submenu(
                id,
                teams_source(state.clone(), on_change.clone()),
                sub,
                root.clone(),
                window,
                cx,
            )
        },
    )
}

/// The teams a pick holds (`None` = every team), in the teams' order.
pub(crate) fn picked_team_ids(picked: Option<&[String]>, teams: &[PickTeam]) -> Vec<String> {
    teams
        .iter()
        .filter(|team| picked.is_none_or(|picked| picked.contains(&team.id)))
        .map(|team| team.id.clone())
        .collect()
}

/// TS `nextTeamsPick`: a changed set back to the stored pick: every team (or
/// none) = `None`.
pub(crate) fn next_teams_pick(next: &[String], teams: &[PickTeam]) -> TeamsPick {
    let kept: Vec<String> = teams.iter().filter(|team| next.contains(&team.id)).map(|team| team.id.clone()).collect();
    (!kept.is_empty() && kept.len() != teams.len()).then_some(kept)
}

/// A row's toggle; `None` = refused (the last checked team stays).
pub(crate) fn toggle_team(picked: Option<&[String]>, teams: &[PickTeam], id: &str) -> Option<TeamsPick> {
    let mut next = picked_team_ids(picked, teams);
    match next.iter().position(|team| team == id) {
        Some(at) => {
            next.remove(at);
        }
        None => next.push(id.to_string()),
    }
    (!next.is_empty()).then(|| next_teams_pick(&next, teams))
}

/// TS `teamsPickValue`: the row's value.
pub(crate) fn teams_pick_value(picked: Option<&[String]>, teams: &[PickTeam]) -> String {
    let chosen = chosen_teams(picked, teams);
    match chosen.len() {
        0 => COPY_ALL.to_string(),
        n if n == teams.len() => COPY_ALL.to_string(),
        1 => chosen[0].name.clone(),
        n => COPY_MANY.replace("{n}", &n.to_string()),
    }
}

fn chosen_teams<'a>(picked: Option<&[String]>, teams: &'a [PickTeam]) -> Vec<&'a PickTeam> {
    teams.iter().filter(|team| picked.is_none_or(|picked| picked.contains(&team.id))).collect()
}

/// The chips the pick shows: its teams while it is a SUBSET, else none.
pub(crate) fn chip_teams<'a>(picked: Option<&[String]>, teams: &'a [PickTeam]) -> Vec<&'a PickTeam> {
    if picked.is_none() {
        return Vec::new();
    }
    let chosen = chosen_teams(picked, teams);
    if chosen.len() == teams.len() {
        return Vec::new();
    }
    chosen
}

/// The Teams › submenu's source: THE multi picker rows (the team mark +
/// name, the check trailing); a pick toggles and keeps the menu open.
/// `state` reads the live teams and pick (the rows repaint after a pick).
pub(crate) fn teams_source(
    state: Rc<dyn Fn(&App) -> (Vec<PickTeam>, TeamsPick)>,
    on_change: OnTeamsPick,
) -> search_list::MenuListSource {
    let rows_state = state.clone();
    search_list::MenuListSource {
        placeholder: "Search",
        rows: Rc::new(move |query: &str, _window: &mut Window, cx: &mut App| {
            let (teams, picked) = rows_state(cx);
            let checked = picked_team_ids(picked.as_deref(), &teams);
            let query = query.trim().to_lowercase();
            teams
                .iter()
                .filter(|team| query.is_empty() || team.name.to_lowercase().contains(&query))
                .map(|team| {
                    let on = checked.contains(&team.id);
                    let mark = crate::user_avatar::team_avatar(team.icon_url.as_deref(), 16., cx);
                    search_list::MenuListRow {
                        id: team.id.clone(),
                        body: h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(div().flex_shrink_0().child(mark))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(cx.theme().foreground)
                                    .child(SharedString::from(team.name.clone())),
                            )
                            .when(on, |row| row.child(search_list::row_check(cx)))
                            .into_any_element(),
                        disabled: false,
                    }
                })
                .collect()
        }),
        pick: Rc::new(move |id: &str, window: &mut Window, cx: &mut App| {
            let (teams, picked) = state(cx);
            if let Some(next) = toggle_team(picked.as_deref(), &teams, id) {
                on_change(next, window, cx);
            }
            false
        }),
        empty: Rc::new(|_query: &str, _cx: &App| "No results".into()),
        loading: Rc::new(|_: &App| false),
        footer: None,
    }
}

/// The picked teams as the composer strip's removable pill chips (a subset
/// only): the team mark + name + ×; removing the last = every team again.
pub(crate) fn team_chips(
    prefix: &'static str,
    teams: &[PickTeam],
    picked: Option<&[String]>,
    disabled: bool,
    on_change: OnTeamsPick,
    cx: &mut App,
) -> Vec<AnyElement> {
    let chosen: Vec<PickTeam> = chip_teams(picked, teams).into_iter().cloned().collect();
    let muted = cx.theme().muted_foreground;
    let mut chips = Vec::new();
    for team in &chosen {
        let rest: Vec<String> = chosen.iter().filter(|row| row.id != team.id).map(|row| row.id.clone()).collect();
        let next = if rest.is_empty() { None } else { next_teams_pick(&rest, teams) };
        let on_change = on_change.clone();
        let mark = crate::user_avatar::team_avatar(team.icon_url.as_deref(), 12., cx);
        chips.push(
            glass_pill(SharedString::from(format!("{prefix}-team-{}", team.id)), PillSize::Sm, PillMode::Readonly, cx)
                .max_w(px(CHIP_MAX_W))
                .pr_1()
                .gap_1()
                .child(div().flex_shrink_0().child(mark))
                .child(div().min_w_0().truncate().text_xs().child(SharedString::from(team.name.clone())))
                .child(
                    crate::composer::composer_tool(
                        SharedString::from(format!("{prefix}-team-{}-remove", team.id)),
                        registry::UI_CLOSE,
                        cx,
                    )
                    .tooltip(SharedString::from(format!("Remove {}", team.name)))
                    .text_color(muted)
                    .flex_shrink_0()
                    .disabled(disabled)
                    .on_click(move |_, window, cx| on_change(next.clone(), window, cx)),
                )
                .into_any_element(),
        );
    }
    chips
}

/// The chips as one wrapping row (the strip above the pending files), or
/// `None` when the pick holds every team.
pub(crate) fn chips_row(
    prefix: &'static str,
    teams: &[PickTeam],
    picked: Option<&[String]>,
    disabled: bool,
    on_change: OnTeamsPick,
    cx: &mut App,
) -> Option<AnyElement> {
    let chips = team_chips(prefix, teams, picked, disabled, on_change, cx);
    (!chips.is_empty()).then(|| h_flex().w_full().flex_wrap().items_center().gap_2().children(chips).into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/composer-menu.json")).unwrap()
    }

    fn teams_of(value: &serde_json::Value) -> Vec<PickTeam> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|team| PickTeam {
                id: team["id"].as_str().unwrap().into(),
                name: team["name"].as_str().unwrap().into(),
                icon_url: None,
            })
            .collect()
    }

    /// composer-menu.json `teamsPick`: the copy and every `valueCases` entry.
    #[test]
    fn the_teams_pick_replays_the_fixture() {
        let fixture = fixture();
        let pick = &fixture["teamsPick"];
        assert_eq!(pick["copy"]["all"], COPY_ALL);
        assert_eq!(pick["copy"]["many"], COPY_MANY);
        let cases = pick["valueCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let teams = teams_of(&case["teams"]);
            let picked: TeamsPick = case["picked"]
                .as_array()
                .map(|ids| ids.iter().map(|id| id.as_str().unwrap().to_string()).collect());
            assert_eq!(teams_pick_value(picked.as_deref(), &teams), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    /// The rows' rules: unchecking narrows, the last checked team stays,
    /// checking every team again = every team (`None`).
    #[test]
    fn the_teams_pick_toggles() {
        let teams: Vec<PickTeam> = ["t1", "t2", "t3"]
            .iter()
            .map(|id| PickTeam { id: (*id).into(), name: (*id).into(), icon_url: None })
            .collect();
        let narrowed = toggle_team(None, &teams, "t2").unwrap();
        assert_eq!(narrowed, Some(vec!["t1".to_string(), "t3".to_string()]));
        let one = toggle_team(narrowed.as_deref(), &teams, "t1").unwrap();
        assert_eq!(one, Some(vec!["t3".to_string()]));
        assert_eq!(toggle_team(one.as_deref(), &teams, "t3"), None, "the last checked team stays");
        let two = toggle_team(one.as_deref(), &teams, "t2").unwrap();
        assert_eq!(toggle_team(two.as_deref(), &teams, "t1").unwrap(), None, "all checked = every team");
        assert_eq!(chip_teams(None, &teams).len(), 0);
        assert_eq!(chip_teams(one.as_deref(), &teams).len(), 1);
    }
}
