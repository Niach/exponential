//! Workspace settings + account screens (masterplan-v3 §4.2 "Settings" /
//! "Account", §7.9 integrations surface).
//!
//! Web parity targets: the `routes/t/$workspaceSlug/settings/` pages and
//! their `components/workspace/*-section.tsx` cards, plus
//! `routes/_authenticated/account/notifications.tsx`. The workspace-settings
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav with the web's groups — **Team** (General, Members, Labels) and
//! **Projects** (Projects, Repositories) — plus the desktop-only **This
//! device** group (Coding, Local repositories); the detail column shows ONE
//! selected pane with the web's `isOwner &&` gating (General additionally
//! hides when solo — the pane renders nothing there, matching the web); each
//! pane mirrors its web card field-for-field.
//!
//! Navigation INTO these screens: the sidebar footer account
//! dropdown dispatches `OpenSettings` / `OpenAccount` (see `sidebar.rs` +
//! `navigation::init`); this module only provides the screens.
//!
//! Explicit non-goals held here (§4.9): NO billing pane, NO widget-config
//! pane, NO admin surface. Plan-cap failures (HTTP 412 from `lib/billing.ts`)
//! render as a neutral "Upgrade on the web" notice — never an in-app
//! purchase/pricing UI. The GitHub App *install* is a browser hand-off
//! (§7.9); Google Calendar does not exist anywhere.

mod account;
mod coding;
mod labels;
mod local_repos;
mod members;
mod notifications_prefs;
mod projects;
mod repositories;
mod workspace_general;

pub use account::AccountView;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};
use sync::Store;

use crate::navigation::{active_workspace_id, nav_for_window, Navigation};
use crate::queries;

use labels::LabelsPane;
use local_repos::LocalReposPane;
use members::MembersPane;
use projects::ProjectsPane;
use repositories::RepositoriesPane;
use self::coding::CodingPane;
use workspace_general::GeneralPane;

// ---------------------------------------------------------------------------
// Section nav model (EXP-146 grouped master-detail)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsSection {
    General,
    Members,
    Labels,
    Projects,
    Repositories,
    Coding,
    LocalRepos,
}

struct NavItem {
    label: &'static str,
    section: SettingsSection,
}

struct NavGroup {
    label: &'static str,
    items: &'static [NavItem],
}

/// The web's `SETTINGS_NAV` groups minus the web-only Billing/Widget items,
/// plus the desktop-only "This device" group. Order defines both the nav and
/// the non-owner fallback (first visible item).
const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "Team",
        items: &[
            NavItem {
                label: "General",
                section: SettingsSection::General,
            },
            NavItem {
                label: "Members",
                section: SettingsSection::Members,
            },
            NavItem {
                label: "Labels",
                section: SettingsSection::Labels,
            },
        ],
    },
    NavGroup {
        label: "Projects",
        items: &[
            NavItem {
                label: "Projects",
                section: SettingsSection::Projects,
            },
            NavItem {
                label: "Repositories",
                section: SettingsSection::Repositories,
            },
        ],
    },
    NavGroup {
        label: "This device",
        items: &[
            NavItem {
                label: "Coding",
                section: SettingsSection::Coding,
            },
            NavItem {
                label: "Local repositories",
                section: SettingsSection::LocalRepos,
            },
        ],
    },
];

/// Web nav `visible` gating: General/Projects/Repositories are owner-only,
/// and General additionally hides when solo (GeneralPane renders nothing
/// there, mirroring the web section's `if (solo) return null`).
fn section_visible(section: SettingsSection, owner: bool, solo: bool) -> bool {
    match section {
        SettingsSection::General => owner && !solo,
        SettingsSection::Projects | SettingsSection::Repositories => owner,
        _ => true,
    }
}

/// The selected section, clamped to what's visible. Clamped at render time —
/// never mutated — so a membership change that hides the selection falls back
/// (to Members, the first never-gated item) and restores it if ownership
/// returns.
fn effective_selection(selected: SettingsSection, owner: bool, solo: bool) -> SettingsSection {
    if section_visible(selected, owner, solo) {
        return selected;
    }
    NAV_GROUPS
        .iter()
        .flat_map(|group| group.items)
        .map(|item| item.section)
        .find(|&section| section_visible(section, owner, solo))
        .expect("Members is never gated")
}

// ---------------------------------------------------------------------------
// Workspace settings shell
// ---------------------------------------------------------------------------

/// The workspace-settings screen (`Screen::Settings`) — the web settings
/// pages' grouped master-detail layout (billing/widget/danger-zone
/// skipped: web-only, §4.9).
pub struct SettingsView {
    nav: Entity<Navigation>,
    general: Entity<GeneralPane>,
    members: Entity<MembersPane>,
    labels: Entity<LabelsPane>,
    projects: Entity<ProjectsPane>,
    repositories: Entity<RepositoriesPane>,
    /// §7.7 desktop-only card block (launcher settings + doctor + key status)
    /// — local per-install state, so NOT owner-gated and last in the column.
    coding: Entity<CodingPane>,
    /// §4.7 desktop-only Local repositories section (clone disk usage +
    /// prune/remove) — local per-install state, un-gated, after Coding.
    local_repos: Entity<LocalReposPane>,
    /// The nav selection; clamped through `effective_selection` at render
    /// time so gated sections never show for non-owners.
    selected: SettingsSection,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let general = cx.new(|cx| GeneralPane::new(nav.clone(), window, cx));
        let members = cx.new(|cx| MembersPane::new(nav.clone(), cx));
        let labels = cx.new(|cx| LabelsPane::new(nav.clone(), window, cx));
        let projects = cx.new(|cx| ProjectsPane::new(nav.clone(), cx));
        let repositories = cx.new(|cx| RepositoriesPane::new(nav.clone(), cx));
        let coding = cx.new(|cx| CodingPane::new(window, cx));
        let local_repos = cx.new(LocalReposPane::new);

        // The section nav + header depend on role (owner gating) and the
        // solo heuristic — re-render when membership/workspace data moves.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.workspaces, |_, _, cx| cx.notify()),
            cx.observe(&collections.workspace_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            general,
            members,
            labels,
            projects,
            repositories,
            coding,
            local_repos,
            selected: SettingsSection::General,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_workspace_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        let solo = {
            let workspace_id = active_workspace_id(&self.nav, cx);
            workspace_id
                .as_deref()
                .map(|ws| !show_workspace_chrome(cx, ws))
                .unwrap_or(true)
        };
        let (title, subtitle): (&'static str, SharedString) = if solo {
            (
                "Settings",
                "Manage your projects, labels, and repositories.".into(),
            )
        } else {
            let name = active_workspace(cx, &self.nav)
                .map(|workspace| workspace.name)
                .unwrap_or_default();
            (
                "Team Settings",
                format!("Manage members, invites, and labels for {name}").into(),
            )
        };

        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column.
        let effective = effective_selection(self.selected, owner, solo);

        let mut nav = v_flex().p_2().gap_0p5();
        for group in NAV_GROUPS {
            let visible: Vec<&NavItem> = group
                .items
                .iter()
                .filter(|item| section_visible(item.section, owner, solo))
                .collect();
            if visible.is_empty() {
                continue;
            }
            nav = nav.child(
                div()
                    .px_2()
                    .pt_2()
                    .pb_0p5()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(group.label),
            );
            for item in visible {
                let section = item.section;
                let is_selected = section == effective;
                nav = nav.child(
                    h_flex()
                        .id(item.label)
                        .w_full()
                        .px_2()
                        .py_1()
                        .rounded(cx.theme().radius)
                        .text_sm()
                        .cursor_pointer()
                        .when(is_selected, |this| this.bg(cx.theme().accent.opacity(0.6)))
                        .hover(|this| this.bg(cx.theme().accent.opacity(0.3)))
                        .child(item.label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = section;
                            cx.notify();
                        })),
                );
            }
        }

        let pane: gpui::AnyElement = match effective {
            SettingsSection::General => self.general.clone().into_any_element(),
            SettingsSection::Members => self.members.clone().into_any_element(),
            SettingsSection::Labels => self.labels.clone().into_any_element(),
            SettingsSection::Projects => self.projects.clone().into_any_element(),
            SettingsSection::Repositories => self.repositories.clone().into_any_element(),
            SettingsSection::Coding => self.coding.clone().into_any_element(),
            SettingsSection::LocalRepos => self.local_repos.clone().into_any_element(),
        };

        v_flex()
            .size_full()
            .child(screen_header(title, cx))
            .child(
                h_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("settings-nav")
                            .w(px(200.))
                            .h_full()
                            .flex_shrink_0()
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .overflow_y_scroll()
                            .child(nav),
                    )
                    .child(
                        // Scroll id keyed by section so each section keeps an
                        // independent scroll offset.
                        div()
                            .id(SharedString::from(format!("settings-detail-{effective:?}")))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .child(
                                h_flex().w_full().justify_center().child(
                                    v_flex()
                                        .w_full()
                                        .max_w(px(672.))
                                        .p_4()
                                        .gap_4()
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(subtitle),
                                        )
                                        .child(pane),
                                ),
                            ),
                    ),
            )
    }
}


// ---------------------------------------------------------------------------
// Shared query helpers (settings-scoped; the general chrome helper moves to
// `queries.rs` with the §4.8 sidebar solo rule)
// ---------------------------------------------------------------------------

/// The window's active synced workspace row.
pub(crate) fn active_workspace(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<domain::rows::Workspace> {
    let workspace_id = active_workspace_id(nav, cx)?;
    Store::global(cx)
        .collections()
        .workspaces
        .read(cx)
        .get(&workspace_id)
        .cloned()
}

/// My membership row in `workspace_id` (id + role), from the synced
/// collections.
pub(crate) fn my_membership(cx: &App, workspace_id: &str) -> Option<(String, String)> {
    let me = queries::active_account(cx)?;
    Store::global(cx)
        .collections()
        .workspace_members
        .read(cx)
        .iter()
        .find(|member| member.workspace_id == workspace_id && member.user_id == me.user_id)
        .map(|member| {
            (
                member.id.clone(),
                member.role.clone().unwrap_or_else(|| "member".to_string()),
            )
        })
}

/// Web `isOwner` gate (settings route: `currentMember?.role === 'owner'`).
pub(crate) fn is_owner(cx: &App, workspace_id: &str) -> bool {
    my_membership(cx, workspace_id)
        .map(|(_, role)| role == domain::contract::WORKSPACE_ROLE_OWNER)
        .unwrap_or(false)
}

/// Web `useIsSolo`: true while data loads (bias hidden), else "≤1 human
/// member" (agents excluded).
pub(crate) fn is_solo_workspace(cx: &App, workspace_id: &str) -> bool {
    let collections = Store::global(cx).collections();
    if !collections.workspace_members.read(cx).is_ready()
        || !collections.workspaces.read(cx).is_ready()
    {
        return true;
    }
    if collections.workspaces.read(cx).get(workspace_id).is_none() {
        return true;
    }
    let users = collections.users.read(cx);
    let human_members = collections
        .workspace_members
        .read(cx)
        .iter()
        .filter(|member| member.workspace_id == workspace_id)
        .filter(|member| {
            users
                .get(&member.user_id)
                .map(|user| user.is_agent != Some(true))
                .unwrap_or(true)
        })
        .count();
    human_members <= 1
}

/// Web `useShowWorkspaceChrome`: revealed when the workspace stops being solo
/// OR the user explicitly reasons about 2+ workspaces (public counts only
/// with a membership row).
pub(crate) fn show_workspace_chrome(cx: &App, workspace_id: &str) -> bool {
    let is_solo = is_solo_workspace(cx, workspace_id);
    let Some(me) = queries::active_account(cx) else {
        return !is_solo;
    };
    let collections = Store::global(cx).collections();
    let membership_ids: std::collections::HashSet<String> = collections
        .workspace_members
        .read(cx)
        .iter()
        .filter(|member| member.user_id == me.user_id)
        .map(|member| member.workspace_id.clone())
        .collect();
    // Web: `myWorkspaces` = membership workspaces + the public workspace
    // appended; `explicitCount` keeps only `!isPublic || membership` — so the
    // appended-without-membership public row contributes 0 and the count
    // reduces to "workspaces I have a membership row in".
    let workspaces = collections.workspaces.read(cx);
    let explicit_count = workspaces
        .iter()
        .filter(|workspace| membership_ids.contains(&workspace.id))
        .count();
    !is_solo || explicit_count > 1
}

// ---------------------------------------------------------------------------
// Shared chrome bits (web Card + notices at compact density)
// ---------------------------------------------------------------------------

/// Compact screen header (same 34px bar the other screens use).
pub(crate) fn screen_header(title: &'static str, cx: &App) -> impl IntoElement {
    h_flex()
        .px_3()
        .h(px(34.))
        .items_center()
        .flex_shrink_0()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
}

/// Web `Card`: bordered rounded section (§4.3 — the `list_head` surface; no
/// `card` token exists).
pub(crate) fn card(cx: &App) -> gpui::Div {
    v_flex()
        .w_full()
        .gap_3()
        .p_4()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius_lg)
        .bg(cx.theme().colors.list_head)
}

/// Web `CardTitle` + `CardDescription`.
pub(crate) fn card_header(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}

/// Inline destructive error box (web `text-destructive` / bordered error).
pub(crate) fn error_notice(message: SharedString, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().danger.opacity(0.5))
        .bg(cx.theme().danger.opacity(0.1))
        .text_sm()
        .text_color(cx.theme().danger)
        .child(message)
}

/// §4.9 plan-cap surface: a neutral "Upgrade on the web" notice — never an
/// in-app purchase/pricing UI.
pub(crate) fn upgrade_notice(message: SharedString, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().primary.opacity(0.4))
        .bg(cx.theme().primary.opacity(0.05))
        .text_sm()
        .child(message)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Upgrade on the web to raise this limit."),
        )
}

/// `#rrggbb` → Hsla (label/project colors are stored as hex strings).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(
        gpui::Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: 1.0,
        }
        .into(),
    )
}

// ---------------------------------------------------------------------------
// Shared mutation plumbing
// ---------------------------------------------------------------------------

/// §4.1 un-gated fire-and-forget mutation: run the blocking tRPC call on a
/// background thread; the UI updates via the Electric echo. Errors are
/// logged (the web's inline sections behave the same for these calls).
pub(crate) fn spawn_trpc<T, F>(cx: &mut App, what: &'static str, call: F)
where
    T: Send + 'static,
    F: FnOnce(&api::TrpcClient) -> Result<T, api::ApiError> + Send + 'static,
{
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] {what} skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = call(&trpc) {
                log::warn!("[ui] {what} failed: {err}");
            }
        })
        .detach();
}

/// Open a URL through the robust opener chain (never a raw xdg-open),
/// off the foreground thread.
pub(crate) fn open_url(cx: &mut App, url: String) {
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(&url) {
                log::warn!("[ui] open-in-browser failed: {err}");
            }
        })
        .detach();
}

/// A plan-cap rejection (`assertWithinPlanLimits` → PRECONDITION_FAILED /
/// HTTP 412). Drives the §4.9 "Upgrade on the web" notice.
pub(crate) fn is_plan_limit(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status: 412, .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_defaults_to_general() {
        assert_eq!(
            effective_selection(SettingsSection::General, true, false),
            SettingsSection::General
        );
    }

    #[test]
    fn non_owner_falls_back_to_members() {
        for gated in [
            SettingsSection::General,
            SettingsSection::Projects,
            SettingsSection::Repositories,
        ] {
            assert_eq!(
                effective_selection(gated, false, false),
                SettingsSection::Members
            );
        }
    }

    #[test]
    fn solo_owner_hides_general() {
        // GeneralPane renders nothing when solo (web parity), so the nav must
        // hide it and the default selection must fall through to Members.
        assert!(!section_visible(SettingsSection::General, true, true));
        assert_eq!(
            effective_selection(SettingsSection::General, true, true),
            SettingsSection::Members
        );
        // Solo does NOT gate the other owner sections.
        assert!(section_visible(SettingsSection::Projects, true, true));
    }

    #[test]
    fn device_sections_never_gated() {
        for section in [SettingsSection::Coding, SettingsSection::LocalRepos] {
            assert!(section_visible(section, false, true));
            assert_eq!(effective_selection(section, false, true), section);
        }
    }

    #[test]
    fn ungated_selection_is_kept() {
        assert_eq!(
            effective_selection(SettingsSection::Labels, false, false),
            SettingsSection::Labels
        );
    }
}
