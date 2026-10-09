//! EXP-1133 — the shared repository picker atop the IDE surfaces.
//!
//! Files and Source Control (and with them the trunk-sync engine and the
//! `+` shell) show ONE repository per window, resolved by
//! [`crate::repo_resolver::RepoResolver::lookup_active`]: the team's explicit
//! pick, else its first board-backed repo. It used to follow the last board
//! opened, so a multi-repo team saw whichever trunk the sidebar last
//! touched. This picker is the only way to switch it; it renders only while
//! the team has MORE than one repository (one repo = nothing to pick).
//!
//! Same trigger as the Files worktree switcher beside it (ghost, xsmall,
//! caret), so the scope row reads as one "repo · branch" pair.

use gpui::{
    AnyElement, App, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    Icon, Sizable as _,
};

use crate::icons::registry;
use crate::navigation::{self, Navigation};
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup};

/// EXP-1142: what a repo-scoped surface says when the window has NO team to
/// scope to — a signed-in user before their first team, or after leaving
/// the last one.
pub(crate) const NO_TEAM_NOTICE: &str =
    "Join or create a team to browse its repositories.";

/// EXP-1142: the notice a repo-scoped surface shows INSTEAD of its loading
/// state when there is no team. `None` while the teams + boards shapes are
/// still snapshotting (the loading state is honest then) and whenever a
/// team resolves. Files used to say "Loading files…" forever here: no team
/// means the resolver never leaves `Loading`, so nothing else ends it.
pub(crate) fn no_team_notice(nav: &Entity<Navigation>, cx: &App) -> Option<&'static str> {
    (navigation::shapes_ready(cx) && navigation::active_team_id(nav, cx).is_none())
        .then_some(NO_TEAM_NOTICE)
}

/// The repo picker trigger, or `None` while the team has fewer than two
/// board-backed repositories (or they are still resolving).
pub(crate) fn repo_picker(id: &'static str, window: &Window, cx: &mut App) -> Option<AnyElement> {
    let resolver = repo_resolver_for_window(window, cx);
    let resolver = resolver.read(cx);
    let repos = resolver.scope_repos(cx)?;
    if repos.len() < 2 {
        return None;
    }
    let RepoLookup::Found(active) = resolver.lookup_active(cx) else {
        return None;
    };
    let items: Vec<(String, SharedString)> = repos
        .iter()
        .map(|repo| (repo.repository_id.clone(), repo.full_name.clone().into()))
        .collect();
    let active_id = active.repository_id.clone();
    Some(
        Button::new(id)
            .ghost()
            .cursor_pointer()
            .xsmall()
            .icon(Icon::new(registry::UI_GITHUB))
            // EXP-697: NOT `.label()` — a long name must truncate, not wrap.
            .child(crate::surface::picker_value_label(repo_short_name(
                &active.full_name,
            )))
            .dropdown_caret(true)
            .tooltip(SharedString::from(active.full_name.clone()))
            .dropdown_menu(move |mut menu, _window, _cx| {
                menu = menu.item(PopupMenuItem::label("Repository"));
                for (repository_id, full_name) in &items {
                    let repository_id = repository_id.clone();
                    menu = menu.item(
                        // No per-item icon: it would replace the check
                        // mark (the worktree switcher's menu reads the same).
                        crate::controls::pointer_label_item(full_name.clone(), false)
                            .checked(repository_id == active_id)
                            .on_click(move |_, window, cx| {
                                crate::navigation::set_active_repo(
                                    window,
                                    cx,
                                    repository_id.clone(),
                                );
                            }),
                    );
                }
                menu
            })
            .into_any_element(),
    )
}

/// `owner/name` → `name` (the trigger's label; the menu and tooltip carry
/// the full name).
fn repo_short_name(full_name: &str) -> SharedString {
    full_name
        .rsplit_once('/')
        .map_or(full_name, |(_, name)| name)
        .to_string()
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_drops_the_owner() {
        assert_eq!(
            repo_short_name("m5software/m5-support").as_ref(),
            "m5-support"
        );
        assert_eq!(repo_short_name("bare").as_ref(), "bare");
    }
}
