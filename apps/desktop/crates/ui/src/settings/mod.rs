//! Team settings + account screens (masterplan-v3 §4.2 "Settings" /
//! "Account", §7.9 integrations surface).
//!
//! Web parity targets: the `routes/t/$teamSlug/settings/` pages and
//! their `components/team/*-section.tsx` cards, plus
//! `routes/_authenticated/account/notifications.tsx`. The team-settings
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav with the web's groups — **Team** (General, Members, Labels) and
//! **Boards** (Boards, Repositories) — plus the desktop-only **This
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
mod boards;
mod repositories;
mod team_general;

pub use account::AccountView;

/// EXP-282: width of the settings nav column — it REPLACES the tool column
/// while a settings screen is up (rendered by `shell::CenterPanel`), so it
/// owns a fixed width like the right detail sidebars rather than riding the
/// resizable split.
pub const SETTINGS_NAV_WIDTH: f32 = 212.;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, IconName, Sizable as _};
use sync::Store;

use crate::navigation::{
    active_team_id, nav_for_window, navigate, resolved_screen, Navigation, Screen,
};
use crate::queries;
use crate::sidebar::{rail_shared_for_window, select_settings_section, RailShared};

use labels::LabelsPane;
use local_repos::LocalReposPane;
use members::MembersPane;
use boards::BoardsPane;
use repositories::RepositoriesPane;
use self::coding::CodingPane;
use team_general::GeneralPane;

// ---------------------------------------------------------------------------
// Section nav model (EXP-146 grouped master-detail)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsSection {
    General,
    Members,
    Labels,
    Boards,
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
        label: "Boards",
        items: &[
            NavItem {
                label: "Boards",
                section: SettingsSection::Boards,
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

/// Web nav `visible` gating: General/Boards/Repositories are owner-only,
/// and General additionally hides when solo (GeneralPane renders nothing
/// there, mirroring the web section's `if (solo) return null`).
fn section_visible(section: SettingsSection, owner: bool, solo: bool) -> bool {
    match section {
        SettingsSection::General => owner && !solo,
        SettingsSection::Boards | SettingsSection::Repositories => owner,
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
// Team settings shell
// ---------------------------------------------------------------------------

/// The team-settings screen (`Screen::Settings`) — the web settings
/// pages' grouped master-detail layout (billing/widget/danger-zone
/// skipped: web-only, §4.9).
pub struct SettingsView {
    nav: Entity<Navigation>,
    general: Entity<GeneralPane>,
    members: Entity<MembersPane>,
    labels: Entity<LabelsPane>,
    boards: Entity<BoardsPane>,
    repositories: Entity<RepositoriesPane>,
    /// §7.7 desktop-only card block (launcher settings + doctor + key status)
    /// — local per-install state, so NOT owner-gated and last in the column.
    coding: Entity<CodingPane>,
    /// §4.7 desktop-only Local repositories section (clone disk usage +
    /// prune/remove) — local per-install state, un-gated, after Coding.
    local_repos: Entity<LocalReposPane>,
    /// EXP-282: the nav selection lives on the window's [`RailShared`] now —
    /// the nav column renders OUTSIDE this view (it replaces the tool column
    /// while settings are up), so both must read one value. Clamped through
    /// `effective_selection` at render time so gated sections never show for
    /// non-owners.
    shared: Entity<RailShared>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let general = cx.new(|cx| GeneralPane::new(nav.clone(), window, cx));
        let members = cx.new(|cx| MembersPane::new(nav.clone(), window, cx));
        let labels = cx.new(|cx| LabelsPane::new(nav.clone(), window, cx));
        let boards = cx.new(|cx| BoardsPane::new(nav.clone(), cx));
        let repositories = cx.new(|cx| RepositoriesPane::new(nav.clone(), cx));
        let coding = cx.new(|cx| CodingPane::new(window, cx));
        let local_repos = cx.new(LocalReposPane::new);

        // The section nav + header depend on role (owner gating) and the
        // solo heuristic — re-render when membership/team data moves.
        let shared = rail_shared_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-282: the nav column mutates the shared selection.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            general,
            members,
            labels,
            boards,
            repositories,
            coding,
            local_repos,
            shared,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        let solo = {
            let team_id = active_team_id(&self.nav, cx);
            team_id
                .as_deref()
                .map(|ws| !show_team_chrome(cx, ws))
                .unwrap_or(true)
        };
        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column. EXP-282: the nav is
        // the window's left column now ([`SettingsNavPanel`]) — this view is
        // the detail column alone.
        let effective =
            effective_selection(self.shared.read(cx).settings_section(), owner, solo);

        let pane: gpui::AnyElement = match effective {
            SettingsSection::General => self.general.clone().into_any_element(),
            SettingsSection::Members => self.members.clone().into_any_element(),
            SettingsSection::Labels => self.labels.clone().into_any_element(),
            SettingsSection::Boards => self.boards.clone().into_any_element(),
            SettingsSection::Repositories => self.repositories.clone().into_any_element(),
            SettingsSection::Coding => self.coding.clone().into_any_element(),
            SettingsSection::LocalRepos => self.local_repos.clone().into_any_element(),
        };

        // EXP-277: no screen header (the center tab already carries the
        // title). EXP-282: no nav column and no centering wrapper either —
        // the content column hugs the left edge under a 672px cap, so every
        // pane's headings line up with the nav column beside it.
        //
        // Scroll id keyed by section so each section keeps an independent
        // scroll offset.
        div()
            .id(SharedString::from(format!("settings-detail-{effective:?}")))
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .child(detail_column().child(pane))
    }
}

/// EXP-282: the shared settings detail column — left-aligned, capped, with
/// generous section spacing (the panes are flat sections now, so whitespace
/// is the only separator left). Also used by the Account screen so both
/// settings surfaces sit on the same grid.
pub(crate) fn detail_column() -> gpui::Div {
    v_flex().w_full().max_w(px(672.)).p_5().gap_6()
}

// ---------------------------------------------------------------------------
// SettingsNavPanel — the settings nav as the window's LEFT column (EXP-282)
// ---------------------------------------------------------------------------

/// The settings navigation, rendered by `shell::CenterPanel` IN PLACE of the
/// tool column while `Screen::Settings`/`Screen::Account` is up. It reads and
/// writes the window's shared [`RailShared::settings_section`], so the detail
/// view ([`SettingsView`]) always shows what this column highlights.
pub struct SettingsNavPanel {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsNavPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // The Account row's highlight follows the active screen.
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // Owner/solo gating hides rows — same data the detail view reads.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            shared,
            _subscriptions: subscriptions,
        }
    }

    /// One nav row (hand-rolled — gpui-component's `Button` centers its
    /// inner layout, and these rows must read as a left-aligned list).
    fn row(
        id: &'static str,
        label: &'static str,
        icon: Option<Icon>,
        selected: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id)
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .rounded(cx.theme().radius)
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
            })
            .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .children(icon.map(|icon| icon.xsmall().flex_shrink_0()))
            .child(label)
    }

    fn group_label(label: &'static str, cx: &App) -> impl IntoElement {
        div()
            .px_2()
            .pt_2()
            .pb_0p5()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }
}

impl Render for SettingsNavPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        let solo = active_team_id(&self.nav, cx)
            .as_deref()
            .map(|ws| !show_team_chrome(cx, ws))
            .unwrap_or(true);
        // The Account screen IS a settings section as far as this column is
        // concerned — while it is up no team/device row is highlighted.
        let on_account = matches!(resolved_screen(&self.nav, cx), Some(Screen::Account));
        let effective =
            effective_selection(self.shared.read(cx).settings_section(), owner, solo);

        let mut list = v_flex().p_2().gap_0p5();
        for group in NAV_GROUPS {
            let visible: Vec<&NavItem> = group
                .items
                .iter()
                .filter(|item| section_visible(item.section, owner, solo))
                .collect();
            if visible.is_empty() {
                continue;
            }
            list = list.child(Self::group_label(group.label, cx));
            for item in visible {
                let section = item.section;
                let selected = !on_account && section == effective;
                list = list.child(
                    Self::row(item.label, item.label, None, selected, cx).on_click(
                        cx.listener(move |_, _, window, cx| {
                            select_settings_section(window, cx, section);
                            // A section click from the Account screen returns
                            // to the settings detail.
                            navigate(window, cx, Screen::Settings);
                        }),
                    ),
                );
            }
        }
        // EXP-282: Account moved INTO the settings chrome (it used to be a
        // second account-dropdown entry with its own bare screen).
        list = list
            .child(Self::group_label("Personal", cx))
            .child(
                Self::row(
                    "settings-nav-account",
                    "Account",
                    Some(Icon::new(IconName::CircleUser)),
                    on_account,
                    cx,
                )
                .on_click(cx.listener(|_, _, window, cx| {
                    navigate(window, cx, Screen::Account);
                })),
            );

        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // EXP-285: like the tool column it replaces — no fill, just a
            // hairline boundary over the one page gradient.
            .border_r_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .text_color(cx.theme().sidebar_foreground)
            .child(
                div()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}


// ---------------------------------------------------------------------------
// Shared query helpers (settings-scoped; the general chrome helper moves to
// `queries.rs` with the §4.8 sidebar solo rule)
// ---------------------------------------------------------------------------

/// The window's active synced team row.
pub(crate) fn active_team(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<domain::rows::Team> {
    let team_id = active_team_id(nav, cx)?;
    Store::global(cx)
        .collections()
        .teams
        .read(cx)
        .get(&team_id)
        .cloned()
}

/// My membership row in `team_id` (id + role), from the synced
/// collections.
pub(crate) fn my_membership(cx: &App, team_id: &str) -> Option<(String, String)> {
    let me = queries::active_account(cx)?;
    Store::global(cx)
        .collections()
        .team_members
        .read(cx)
        .iter()
        .find(|member| member.team_id == team_id && member.user_id == me.user_id)
        .map(|member| {
            (
                member.id.clone(),
                member.role.clone().unwrap_or_else(|| "member".to_string()),
            )
        })
}

/// Web `isOwner` gate (settings route: `currentMember?.role === 'owner'`).
pub(crate) fn is_owner(cx: &App, team_id: &str) -> bool {
    my_membership(cx, team_id)
        .map(|(_, role)| role == domain::contract::TEAM_ROLE_OWNER)
        .unwrap_or(false)
}

/// Web `useIsSolo`: true while data loads (bias hidden), else "≤1 human
/// member" (agents excluded).
pub(crate) fn is_solo_team(cx: &App, team_id: &str) -> bool {
    let collections = Store::global(cx).collections();
    if !collections.team_members.read(cx).is_ready()
        || !collections.teams.read(cx).is_ready()
    {
        return true;
    }
    if collections.teams.read(cx).get(team_id).is_none() {
        return true;
    }
    let member_count = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.team_id == team_id)
        .count();
    member_count <= 1
}

/// Web `useShowTeamChrome`: revealed when the team stops being solo
/// OR the user explicitly reasons about 2+ teams.
pub(crate) fn show_team_chrome(cx: &App, team_id: &str) -> bool {
    let is_solo = is_solo_team(cx, team_id);
    let Some(me) = queries::active_account(cx) else {
        return !is_solo;
    };
    let collections = Store::global(cx).collections();
    let membership_ids: std::collections::HashSet<String> = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.user_id == me.user_id)
        .map(|member| member.team_id.clone())
        .collect();
    // Web parity: the count reduces to "teams I have a membership row
    // in".
    let teams = collections.teams.read(cx);
    let explicit_count = teams
        .iter()
        .filter(|team| membership_ids.contains(&team.id))
        .count();
    !is_solo || explicit_count > 1
}

// ---------------------------------------------------------------------------
// Shared chrome bits (web Card + notices at compact density)
// ---------------------------------------------------------------------------

/// EXP-282: the settings panes' section container — FLAT. It used to be the
/// glass card (EXP-269); stacked cards inside an already-glass column read as
/// boxes-in-boxes, so a section is now just a left-aligned block whose
/// heading ([`card_header`]) carries the structure. Renamed `card` → `section`
/// across the panes.
pub(crate) fn section(_cx: &App) -> gpui::Div {
    v_flex().w_full().gap_3()
}

/// EXP-282: the hairline the panes' rows/chips draw — the glass row stroke
/// instead of the heavier `theme.border`, now that no card frames them.
pub(crate) fn row_stroke(_cx: &App) -> gpui::Hsla {
    theme::tokens::glass::STROKE_ROW.to_hsla()
}

/// EXP-285: one settings preference row — label + hint column left (the hint
/// wraps at a readable measure instead of sprawling the full pane), the
/// control pinned right, hairline separators BETWEEN rows carrying the
/// rhythm (`first` rows draw none).
pub(crate) fn pref_row(
    label: impl IntoElement,
    hint: impl Into<SharedString>,
    control: impl IntoElement,
    first: bool,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .w_full()
        .items_center()
        .gap_4()
        .py_2p5()
        .when(!first, |row| row.border_t_1().border_color(row_stroke(cx)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(label)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .max_w(px(460.))
                        .child(hint.into()),
                ),
        )
        .child(div().flex_none().child(control))
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

/// `#rrggbb` → Hsla (label/board colors are stored as hex strings).
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
            SettingsSection::Boards,
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
        assert!(section_visible(SettingsSection::Boards, true, true));
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
