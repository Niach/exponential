//! Workspace settings + account screens (masterplan-v3 §4.2 "Settings" /
//! "Account", §7.9 integrations surface, EXP-1 #9/#11, EXP-4).
//!
//! Web parity targets: `routes/w/$workspaceSlug/settings/index.tsx` and its
//! `components/workspace/*-section.tsx` cards, plus
//! `routes/_authenticated/account/{integrations,notifications}.tsx`. The
//! workspace-settings screen mirrors the web route's structure: **one
//! scrolling column of stacked section cards** (`mx-auto max-w-2xl space-y-6
//! p-6` — no master-detail nav), in the web's card order with the web's
//! `isOwner &&` gating; each pane mirrors its web card field-for-field.
//!
//! Navigation INTO these screens is EXP-1 #11: the sidebar footer account
//! dropdown dispatches `OpenSettings` / `OpenAccount` (see `sidebar.rs` +
//! `navigation::init`); this module only provides the screens.
//!
//! Explicit non-goals held here (§4.9): NO billing pane, NO widget-config
//! pane, NO admin surface. Plan-cap failures (HTTP 412 from `lib/billing.ts`)
//! render as a neutral "Upgrade on the web" notice — never an in-app
//! purchase/pricing UI. The GitHub App *install* is a browser hand-off
//! (§7.9); Google Calendar does not exist anywhere (EXP-1 #9).

mod account;
mod coding;
mod labels;
mod members;
mod notifications_prefs;
mod projects;
mod repositories;
mod workspace_general;

pub use account::AccountView;

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};
use sync::Store;

use crate::navigation::{active_workspace_id, nav_for_window, Navigation};
use crate::queries;

use labels::LabelsPane;
use members::MembersPane;
use projects::ProjectsPane;
use repositories::RepositoriesPane;
use self::coding::CodingPane;
use workspace_general::GeneralPane;

// ---------------------------------------------------------------------------
// Workspace settings shell
// ---------------------------------------------------------------------------

/// The workspace-settings screen (`Screen::Settings`) — the web route's
/// single-scroll stacked-cards page (billing/widget/danger-zone cards
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
                "Workspace Settings",
                format!("Manage members, invites, and labels for {name}").into(),
            )
        };

        // Web `routes/w/$workspaceSlug/settings/index.tsx`: ONE scrolling
        // centered column (`mx-auto max-w-2xl space-y-6 p-6`) of stacked
        // section cards — subtitle, Separator, then the cards in web order
        // with the web's `isOwner &&` gating: General · Projects ·
        // Repositories (owner-only) · Members (always), Separator, Labels
        // (always). Billing/widget/danger-zone are web-only (§4.9).
        let mut sections = v_flex()
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
            .child(separator(cx));
        if owner {
            sections = sections
                .child(self.general.clone())
                .child(self.projects.clone())
                .child(self.repositories.clone());
        }
        sections = sections
            .child(self.members.clone())
            .child(separator(cx))
            .child(self.labels.clone())
            // Desktop-only §7.7 block: launcher settings + tooling doctor +
            // the §7.2 personal-key status row. Local per-install state — no
            // owner gate, no web-parity counterpart.
            .child(separator(cx))
            .child(self.coding.clone());

        v_flex()
            .size_full()
            .child(screen_header(title, cx))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(h_flex().w_full().justify_center().child(sections)),
            )
    }
}

/// Web `Separator`: a 1px full-width border line.
fn separator(cx: &App) -> gpui::Div {
    div().h_px().w_full().flex_shrink_0().bg(cx.theme().border)
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

/// Web `useIsSolo`: true while data loads (bias hidden), false for a public
/// workspace, else "≤1 human member" (agents excluded).
pub(crate) fn is_solo_workspace(cx: &App, workspace_id: &str) -> bool {
    let collections = Store::global(cx).collections();
    if !collections.workspace_members.read(cx).is_ready()
        || !collections.workspaces.read(cx).is_ready()
    {
        return true;
    }
    let Some(workspace) = collections.workspaces.read(cx).get(workspace_id) else {
        return true;
    };
    if workspace.is_public == Some(true) {
        return false;
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

/// Open a URL through the EXP-5-robust opener chain (never a raw xdg-open),
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
