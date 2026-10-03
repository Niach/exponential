//! SLOP-7: `picker-repository` (2 General components): the repository
//! picker's body — a search field over `owner/name` rows, each with the
//! GitHub glyph, a private lock and a trailing tag ("matches board",
//! "used by Website"), then "Add another repository from GitHub…".
//!
//! The rows are drawn with the readiness popover's own recipe
//! (`pickers::picker_row` + the GitHub glyph + the muted tag) and RANKED by
//! its real `coding_readiness::picker_rows` over the contract fixture's
//! first picker case (`coding-readiness.json`, byte-locked ×4), minus one
//! repo so the three tag kinds read at a glance — the web specimen's three
//! rows. The search field is live: typing filters through `picker_rows`.

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Context, Div, Entity,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{input::InputState, v_flex, ActiveTheme as _, Icon};

use domain::coding_readiness::copy;

use crate::coding_readiness::{picker_rows, TeamRepo};
use crate::controls::{search_field, SearchFieldSize};
use crate::icons::registry;

pub(crate) const ID: &str = "picker-repository";
pub(crate) const OWNER: &str = "SLOP-7";

/// The ONE contract fixture the four clients replay.
const FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/coding-readiness.json");

/// The board the picker is choosing for (the fixture case's).
const BOARD: (&str, &str, &str) = ("b-app", "App", "app");

/// Dropped so the list shows one of each tag kind (the web specimen's rows).
const DROPPED: &str = "r-api";

/// The one private repo, for the lock.
const PRIVATE: &str = "r-web";

/// The fixture's first picker case's repos, minus [`DROPPED`].
fn repos() -> Vec<TeamRepo> {
    let parsed = serde_json::from_str::<serde_json::Value>(FIXTURE).ok();
    let repos = parsed
        .as_ref()
        .and_then(|fixture| fixture.get("picker")?.as_array()?.first()?.get("repos").cloned());
    repos
        .and_then(|repos| serde_json::from_value::<Vec<TeamRepo>>(repos).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|repo| repo.id != DROPPED)
        .collect()
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-picker-repository", cx, RepositoryPickerDemo::new);
    div().w(px(320.)).child(demo)
}

struct RepositoryPickerDemo {
    query: Entity<InputState>,
    repos: Vec<TeamRepo>,
}

impl RepositoryPickerDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(copy::PICKER_SEARCH));
        cx.observe(&query, |_, _, cx| cx.notify()).detach();
        Self { query, repos: repos() }
    }
}

impl Render for RepositoryPickerDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let filter = self.query.read(cx).value().to_string();
        let rows = picker_rows(&self.repos, BOARD.0, BOARD.1, BOARD.2, &filter);
        let mut list = v_flex().w_full();
        for row in rows {
            let repo = &self.repos[row.index];
            let matches = row.hint.as_deref() == Some(copy::PICKER_MATCHES_BOARD);
            list = list.child(
                crate::pickers::picker_row(
                    SharedString::from(format!("sg-picker-repository-{}", repo.id)),
                    cx,
                )
                .gap_2()
                .child(Icon::new(registry::UI_GITHUB).size_3().text_color(muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_xs()
                        .child(SharedString::from(repo.full_name.clone())),
                )
                .when(repo.id == PRIVATE, |line| {
                    line.child(Icon::new(registry::UI_PRIVATE).size_3().text_color(muted))
                })
                .when_some(row.hint, |line, hint| {
                    line.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(if matches { theme.success } else { muted.opacity(0.8) })
                            .child(SharedString::from(hint)),
                    )
                }),
            );
        }
        v_flex()
            .w_full()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .overflow_hidden()
            .child(search_field(&self.query, SearchFieldSize::Sm, window, cx).appearance(false))
            .child(div().h(px(1.)).w_full().bg(theme.border.opacity(0.5)))
            .child(list)
            .child(div().h(px(1.)).w_full().bg(theme.border.opacity(0.5)))
            .child(
                crate::pickers::picker_row("sg-picker-repository-add", cx)
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(Icon::new(registry::UI_ADD).size_3())
                    .child(copy::PICKER_ADD_FROM_GITHUB),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three rows the web specimen shows, in the real ranking.
    #[test]
    fn the_fixture_rows_rank_board_match_first() {
        let repos = repos();
        let rows = picker_rows(&repos, BOARD.0, BOARD.1, BOARD.2, "");
        let drawn: Vec<(&str, Option<&str>)> = rows
            .iter()
            .map(|row| (repos[row.index].full_name.as_str(), row.hint.as_deref()))
            .collect();
        assert_eq!(
            drawn,
            vec![
                ("acme-inc/app", Some("matches board")),
                ("acme-inc/website", Some("used by Website")),
                ("acme-inc/tools", None),
            ]
        );
    }
}
