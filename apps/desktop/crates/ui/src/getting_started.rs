//! "Getting started" checklist page (EXP-470, aligned with the web in
//! EXP-548) — the desktop mirror of the web checklist
//! (`apps/web/src/components/getting-started/`), rendered as a tab-less
//! full-page center screen ([`crate::navigation::Screen::GettingStarted`],
//! the Actions page recipe) behind a rail entry that sits at the BOTTOM of
//! the rail (the web sidebar-footer position), right above Settings/Account.
//!
//! Same entries, same order, same role gating, same lock chain and the same
//! titles/descriptions as the web (`getting-started-model.ts` +
//! `getting-started-cards.tsx`): desktop → github → invite → board → coding →
//! action → server → widget → helpdesk → mcp. Only the CTAs differ where the
//! platform must (a dialog instead of a route; owner-only web settings pages
//! open in the browser). `derive_entries` is the byte-for-byte mirror of the
//! web `deriveEntryStates` — change both together.
//!
//! Signals: boards / coding sessions / actions / members / invites / the
//! team's helpdesk flag are live synced collections; the GitHub install
//! state, the device registry, the owner-only widget list and the MCP
//! grant/key pair are tRPC one-shots. They live in ONE app-global
//! [`GettingStartedProgress`] entity shared by the rail entry and the page,
//! polled at the MachinesSection cadence while the page is up and slowly
//! otherwise, and not at all once the checklist is complete.
//!
//! There is NO dismissal (EXP-548): the rail entry (and the page, which
//! leaves on its own) simply disappears once every visible entry is done,
//! and stays hidden while the one-shots are still unanswered — exactly the
//! web rule (`loading || complete`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, relative, App, AppContext as _, ClipboardItem, Entity, FontWeight, Global, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, set_screen, Navigation};
use crate::queries;

/// MachinesSection cadence while the page is visible: often enough that "I
/// just connected GitHub / ran the server one-liner" lands while the user is
/// looking.
const PAGE_POLL_INTERVAL: Duration = Duration::from_secs(15);
/// Background cadence for the rail entry alone (the web fires once + on
/// window focus; a slow poll is the desktop's equivalent).
const RAIL_POLL_INTERVAL: Duration = Duration::from_secs(60);
/// A page render this recent means the page is up.
const VISIBLE_GRACE: Duration = Duration::from_secs(40);

/// The docs page behind the MCP entry (web: `docsUrl('mcp')`).
const MCP_DOCS_URL: &str = "https://exponential.at/docs/mcp/";

// ---------------------------------------------------------------------------
// Pure model (mirrors web `deriveEntryStates`; unit-tested)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EntryKey {
    Desktop,
    Github,
    Invite,
    Board,
    Coding,
    Action,
    Server,
    Widget,
    Helpdesk,
    Mcp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryState {
    Done,
    Available,
    Locked,
}

/// Web `GettingStartedSignals`, field for field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Signals {
    /// devices.list (own rows) has a desktop-kind device — this IDE
    /// registers itself on sign-in, so this is normally true here.
    pub has_desktop_device: bool,
    /// devices.list (own rows) has a server-kind device.
    pub has_server_device: bool,
    /// integrations.github.status → installed.
    pub github_installed: bool,
    /// members ≥ 2 or any invite row — proof the team was shared.
    pub has_invited_team: bool,
    pub has_board: bool,
    pub has_repo_board: bool,
    /// Any coding_sessions row in the team (running or ended).
    pub has_coding_session: bool,
    /// Any synced `actions` row in the team (the builtins are not rows).
    pub has_action: bool,
    /// The team row's helpdesk switch.
    pub helpdesk_enabled: bool,
    /// widgets.list non-empty (owner-only signal).
    pub has_widget: bool,
    /// An MCP OAuth grant exists OR the user holds a personal API key.
    pub mcp_connected: bool,
}

/// Web `deriveEntryStates` options: which role-gated entries the viewer
/// sees. On the server all three are the owner role; kept as three flags so
/// the mirror stays field-for-field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Gates {
    pub can_manage_widgets: bool,
    pub is_owner: bool,
    pub can_manage_members: bool,
}

impl Gates {
    pub(crate) fn for_role(is_owner: bool) -> Self {
        Self {
            can_manage_widgets: is_owner,
            is_owner,
            can_manage_members: is_owner,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub key: EntryKey,
    pub state: EntryState,
    /// For locked entries: the step whose completion unlocks this one.
    pub locked_by: Option<EntryKey>,
}

/// Web `deriveEntryStates`, statement for statement. Static order desktop →
/// github → invite → board → coding → action → server → widget → helpdesk →
/// mcp; completion always wins over locking (a signal that exists proves the
/// prereq was satisfiable); invite is for `can_manage_members`, action +
/// helpdesk for owners, widget for `can_manage_widgets` — the others neither
/// see those entries nor count them in the total.
pub(crate) fn derive_entries(signals: &Signals, gates: Gates) -> Vec<Entry> {
    let simple = |key, done| Entry {
        key,
        state: if done {
            EntryState::Done
        } else {
            EntryState::Available
        },
        locked_by: None,
    };
    let mut entries = Vec::with_capacity(10);

    entries.push(simple(EntryKey::Desktop, signals.has_desktop_device));
    entries.push(simple(EntryKey::Github, signals.github_installed));
    if gates.can_manage_members {
        entries.push(simple(EntryKey::Invite, signals.has_invited_team));
    }
    entries.push(simple(EntryKey::Board, signals.has_board));

    // Coding needs a repo-backed board and a machine to run on; when locked,
    // point at whichever feeder step is still missing, in display order.
    let has_device = signals.has_desktop_device || signals.has_server_device;
    entries.push(if signals.has_coding_session {
        simple(EntryKey::Coding, true)
    } else if signals.has_repo_board && has_device {
        simple(EntryKey::Coding, false)
    } else {
        Entry {
            key: EntryKey::Coding,
            state: EntryState::Locked,
            locked_by: Some(if !has_device {
                EntryKey::Desktop
            } else if signals.github_installed {
                EntryKey::Board
            } else {
                EntryKey::Github
            }),
        }
    });

    // Actions are authored by the builtin creator run, which — like any
    // coding session — needs a machine; the desktop step is the feeder.
    if gates.is_owner {
        entries.push(if signals.has_action {
            simple(EntryKey::Action, true)
        } else if has_device {
            simple(EntryKey::Action, false)
        } else {
            Entry {
                key: EntryKey::Action,
                state: EntryState::Locked,
                locked_by: Some(EntryKey::Desktop),
            }
        });
    }

    entries.push(simple(EntryKey::Server, signals.has_server_device));

    if gates.can_manage_widgets {
        entries.push(if signals.has_widget {
            simple(EntryKey::Widget, true)
        } else if signals.has_board {
            simple(EntryKey::Widget, false)
        } else {
            Entry {
                key: EntryKey::Widget,
                state: EntryState::Locked,
                locked_by: Some(EntryKey::Board),
            }
        });
    }

    if gates.is_owner {
        entries.push(simple(EntryKey::Helpdesk, signals.helpdesk_enabled));
    }

    entries.push(simple(EntryKey::Mcp, signals.mcp_connected));
    entries
}

/// Web `isGettingStartedComplete`: every visible entry done (an empty list is
/// never complete).
pub(crate) fn is_complete(entries: &[Entry]) -> bool {
    !entries.is_empty() && entries.iter().all(|entry| entry.state == EntryState::Done)
}

// ---------------------------------------------------------------------------
// Shared progress state (app-global entity)
// ---------------------------------------------------------------------------

/// The one-shot answers for one team.
#[derive(Clone, Debug, Default)]
struct TeamAnswers {
    github_installed: Option<bool>,
    /// `None` = not answered (or never asked — members never ask).
    has_widget: Option<bool>,
}

/// One derived snapshot for a team — what the rail and the page render.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    /// Some signal source has not answered yet — the rail stays hidden, the
    /// page renders neutral (web `loading`).
    pub loading: bool,
    pub entries: Vec<Entry>,
    pub done: usize,
    pub total: usize,
    /// Every visible entry done — never true while loading (web `complete`).
    pub complete: bool,
}

pub struct GettingStartedProgress {
    /// Team-keyed so a switch back is instant; a team's own answers never
    /// leak to another (the web hook has the identical guard).
    teams: HashMap<String, TeamAnswers>,
    /// `(desktop, server)` kinds in devices.list; `None` until answered.
    devices: Option<(bool, bool)>,
    /// mcpGrants.hasAny || personal keys > 0; `None` until answered.
    mcp_connected: Option<bool>,
    /// The team the poll loop refreshes — the last one a caller asked for.
    wanted_team: Option<String>,
    fetch_seq: u64,
    /// A round is in flight (its answers still pending).
    inflight: bool,
    last_fetch_at: Option<Instant>,
    polling: bool,
    /// Last render of the full page — picks the poll cadence.
    page_rendered: Option<Instant>,
    _subscriptions: Vec<Subscription>,
}

struct GettingStartedProgressGlobal(Entity<GettingStartedProgress>);
impl Global for GettingStartedProgressGlobal {}

impl GettingStartedProgress {
    pub fn global(cx: &mut App) -> Entity<GettingStartedProgress> {
        if let Some(global) = cx.try_global::<GettingStartedProgressGlobal>() {
            return global.0.clone();
        }
        let store = Store::global(cx).clone();
        let entity = cx.new(|cx| {
            // Coarse-grained on purpose: the checklist reads six collections
            // and derives a handful of booleans; observers of THIS entity
            // (rail, page) re-render on any of them.
            let subscriptions = store.observe_collections(cx);
            GettingStartedProgress {
                teams: HashMap::new(),
                devices: None,
                mcp_connected: None,
                wanted_team: None,
                fetch_seq: 0,
                inflight: false,
                last_fetch_at: None,
                polling: false,
                page_rendered: None,
                _subscriptions: subscriptions,
            }
        });
        cx.set_global(GettingStartedProgressGlobal(entity.clone()));
        entity
    }

    /// Ask for `team_id`'s signals: fetches right away on a team change and
    /// (re)starts the poll loop unless the checklist is already complete.
    /// Called from every rail render (cheap: a HashMap lookup) — rendering IS
    /// the liveness signal, like MachinesSection.
    fn ensure(&mut self, team_id: &str, from_page: bool, cx: &mut gpui::Context<Self>) {
        if from_page {
            self.page_rendered = Some(Instant::now());
        }
        let team_changed = self.wanted_team.as_deref() != Some(team_id);
        if team_changed {
            self.wanted_team = Some(team_id.to_string());
        }
        // Nothing left to detect once every entry is done — and every
        // signal is one-way in practice, so the loop simply ends. A later
        // regression (a trashed board, a removed device) re-renders the rail
        // through the collection observers and restarts it here.
        let snapshot = self.snapshot(team_id, cx);
        if snapshot.complete {
            return;
        }
        // Fetch now on a team change / a fresh start, and ALSO while still
        // loading with nothing in flight — the first round may have run
        // before the session or the viewer's member row was there (owners
        // only get `widgets.list` once their role is known), and waiting a
        // whole rail tick would keep the entry hidden that long. Renders
        // arrive on every collection change, so the throttle keeps this from
        // hammering while a query legitimately fails.
        let stale = self
            .last_fetch_at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(3));
        if team_changed || !self.polling || (snapshot.loading && !self.inflight && stale) {
            self.fetch(cx);
        }
        self.ensure_polling(cx);
    }

    fn ensure_polling(&mut self, cx: &mut gpui::Context<Self>) {
        if self.polling {
            return;
        }
        self.polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                let interval = this
                    .read_with(cx, |this, _| {
                        if this
                            .page_rendered
                            .is_some_and(|at| at.elapsed() < VISIBLE_GRACE)
                        {
                            PAGE_POLL_INTERVAL
                        } else {
                            RAIL_POLL_INTERVAL
                        }
                    })
                    .unwrap_or(RAIL_POLL_INTERVAL);
                cx.background_executor().timer(interval).await;
                let keep_going = this.update(cx, |this, cx| {
                    let team = this.wanted_team.clone();
                    let complete = team
                        .as_deref()
                        .is_some_and(|team| this.snapshot(team, cx).complete);
                    if complete || queries::active_account(cx).is_none() {
                        this.polling = false;
                        return false;
                    }
                    this.fetch(cx);
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// One seq-guarded round: `devices.list` + the MCP pair (user-level), the
    /// wanted team's `integrations.github.status`, and — owners only —
    /// `widgets.list`. Failures keep the last answer.
    fn fetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(team_id) = self.wanted_team.clone() else {
            return;
        };
        let ask_widgets = crate::settings::is_owner(cx, &team_id);
        self.fetch_seq += 1;
        self.inflight = true;
        self.last_fetch_at = Some(Instant::now());
        let seq = self.fetch_seq;
        cx.spawn(async move |this, cx| {
            let fetch_team = team_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let devices = api::devices::list(&trpc);
                    let github = crate::github_connect::fetch_github_status(&trpc, &fetch_team);
                    let widgets = ask_widgets.then(|| api::widgets::list(&trpc, &fetch_team));
                    let grants = api::users::mcp_grants_has_any(&trpc);
                    let keys = api::users::list_personal_api_keys(&trpc);
                    (devices, github, widgets, grants, keys)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_seq != seq {
                    return;
                }
                this.inflight = false;
                let (devices, github, widgets, grants, keys) = result;
                match devices {
                    Ok(list) => {
                        let desktop = list.devices.iter().any(|device| device.kind == "desktop");
                        let server = list.devices.iter().any(api::devices::DeviceEntry::is_server);
                        this.devices = Some((desktop, server));
                    }
                    Err(err) => log::warn!("[ui] getting-started devices.list failed: {err}"),
                }
                // Web parity: both halves best-effort, a failure counts as
                // "no" for that half (checklist hint, not access control).
                let has_grant = grants.unwrap_or_else(|err| {
                    log::warn!("[ui] getting-started mcpGrants.hasAny failed: {err}");
                    false
                });
                let has_key = keys
                    .map(|keys| !keys.is_empty())
                    .unwrap_or_else(|err| {
                        log::warn!("[ui] getting-started listPersonalApiKeys failed: {err}");
                        false
                    });
                this.mcp_connected = Some(has_grant || has_key);

                let answers = this.teams.entry(team_id).or_default();
                match github {
                    Ok(status) => answers.github_installed = Some(status.installed),
                    Err(err) => log::warn!("[ui] getting-started github.status failed: {err}"),
                }
                match widgets {
                    Some(Ok(list)) => answers.has_widget = Some(!list.is_empty()),
                    Some(Err(err)) => {
                        log::warn!("[ui] getting-started widgets.list failed: {err}")
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn signals(&self, team_id: &str, cx: &App) -> Signals {
        let collections = Store::global(cx).collections();
        let boards = collections.boards_in_team(team_id, cx);
        let has_coding_session = collections
            .coding_sessions
            .read(cx)
            .iter()
            .any(|session| session.team_id.as_deref() == Some(team_id));
        let has_action = collections
            .actions
            .read(cx)
            .iter()
            .any(|action| action.team_id.as_deref() == Some(team_id));
        let members = collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .count();
        let has_invite = collections
            .team_invites
            .read(cx)
            .iter()
            .any(|invite| invite.team_id == team_id);
        let helpdesk_enabled = collections
            .teams
            .read(cx)
            .get(team_id)
            .and_then(|team| team.helpdesk_enabled)
            == Some(true);
        let answers = self.teams.get(team_id);
        let (has_desktop_device, has_server_device) = self.devices.unwrap_or((false, false));
        Signals {
            has_desktop_device,
            has_server_device,
            github_installed: answers.and_then(|a| a.github_installed) == Some(true),
            has_invited_team: members >= 2 || has_invite,
            has_board: !boards.is_empty(),
            has_repo_board: boards.iter().any(|board| board.repository_id.is_some()),
            has_coding_session,
            has_action,
            helpdesk_enabled,
            has_widget: answers.and_then(|a| a.has_widget) == Some(true),
            mcp_connected: self.mcp_connected == Some(true),
        }
    }

    /// Derive the team's entries + the web `loading`/`complete` pair.
    pub(crate) fn snapshot(&self, team_id: &str, cx: &App) -> Snapshot {
        let is_owner = crate::settings::is_owner(cx, team_id);
        let gates = Gates::for_role(is_owner);
        let signals = self.signals(team_id, cx);
        let entries = derive_entries(&signals, gates);
        let answers = self.teams.get(team_id);
        // Neutral until every signal source has answered — checks/locks that
        // pop in one by one read as state changes, not loading. Membership
        // must have synced too (`is_owner` is transiently false before the
        // viewer's own row lands, exactly the web `resolved` guard).
        let membership_resolved = crate::settings::my_membership(cx, team_id).is_some();
        let loading = !membership_resolved
            || self.devices.is_none()
            || self.mcp_connected.is_none()
            || answers.and_then(|a| a.github_installed).is_none()
            || (gates.can_manage_widgets && answers.and_then(|a| a.has_widget).is_none());
        let done = entries
            .iter()
            .filter(|entry| entry.state == EntryState::Done)
            .count();
        let total = entries.len();
        Snapshot {
            loading,
            complete: !loading && is_complete(&entries),
            entries,
            done,
            total,
        }
    }
}

/// Whether the rail shows the Getting-started entry (EXP-548, the web
/// sidebar rule): signed in with an active team, signals answered, and NOT
/// every entry done. Rendering the rail is what keeps the signals fresh.
pub(crate) fn getting_started_visible(nav: &Entity<Navigation>, cx: &mut App) -> bool {
    if queries::active_account(cx).is_none() {
        return false;
    }
    let Some(team_id) = active_team_id(nav, cx) else {
        return false;
    };
    let progress = GettingStartedProgress::global(cx);
    progress.update(cx, |progress, cx| {
        progress.ensure(&team_id, false, cx);
        let snapshot = progress.snapshot(&team_id, cx);
        !snapshot.loading && !snapshot.complete
    })
}

// ---------------------------------------------------------------------------
// The page view
// ---------------------------------------------------------------------------

pub struct GettingStartedView {
    nav: Entity<Navigation>,
    progress: Entity<GettingStartedProgress>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl GettingStartedView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let progress = GettingStartedProgress::global(cx);
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The progress entity already re-notifies on every collection
            // change, so this one observer covers live + one-shot signals.
            cx.observe(&progress, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            progress,
            scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn entry_card(
        &self,
        index: usize,
        entry: &Entry,
        team_id: &str,
        loading: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // Titles + descriptions: byte-equal to the web ENTRY_TITLES /
        // ENTRY_DESCRIPTIONS (getting-started-cards.tsx).
        let (icon, title, description) = match entry.key {
            EntryKey::Desktop => (
                registry::UI_DEVICE,
                "Get the desktop app",
                "The desktop app is a full git IDE and the client that runs coding sessions \
                 on your machine. Signing in registers it as one of your machines.",
            ),
            EntryKey::Github => (
                registry::UI_GITHUB,
                "Connect a GitHub repo",
                "Link a GitHub account to your team so boards can attach repositories. \
                 Pull requests and coding sessions flow back into their issues.",
            ),
            EntryKey::Invite => (
                registry::UI_INVITE,
                "Invite your team",
                "Teammates share boards, reviews, and the support inbox. Send an \
                 invite by email or hand out an invite link.",
            ),
            EntryKey::Board => (
                registry::NAV_BOARDS,
                "Create a board",
                "Boards hold your issues. Connect a repository to code on a board; \
                 without one it works as a plain board.",
            ),
            EntryKey::Coding => (
                registry::NAV_TERMINAL,
                "Start coding with an agent",
                "\"Start coding\" on any issue hands it to your coding agent on your \
                 machine. It plans first, implements, then commits, pushes, and opens \
                 the pull request linked back to the issue. You just need git and your \
                 agent CLI (claude, codex or pi) on your PATH.",
            ),
            EntryKey::Action => (
                registry::ACTION_CREATE,
                "Create an action",
                "Actions are reusable agent runs for your team — describe one and your \
                 agent writes it. Run them from Agents on any device, or wire them to \
                 automations.",
            ),
            EntryKey::Server => (
                registry::UI_SERVER,
                "Set up a server",
                "Run the headless agent daemon on an always-on machine. One command \
                 installs it; the server then shows up under My machines and can take \
                 remote \"Start coding\" requests.",
            ),
            EntryKey::Widget => (
                registry::SETTINGS_WIDGET,
                "Set up the feedback widget",
                "Embed a feedback button on any website. Visitors report bugs with an \
                 annotated screenshot, and each lands here as an issue with reporter \
                 email and page context.",
            ),
            EntryKey::Helpdesk => (
                registry::NAV_SUPPORT,
                "Enable the helpdesk",
                "Flip the switch in Settings → Feedback widget and every member shares \
                 the Support inbox. Support tickets from the widget land there, with \
                 replies emailed to the reporter.",
            ),
            EntryKey::Mcp => (
                registry::UI_MCP,
                "Connect your tools via MCP",
                "This instance exposes an MCP server at /api/mcp. Connect Claude, \
                 ChatGPT, Cursor, or any MCP client to work with issues, boards, and \
                 comments from your tools.",
            ),
        };
        let locked = !loading && entry.state == EntryState::Locked;
        // Web `lockedHint`, case for case.
        let hint: Option<&'static str> = match (entry.key, entry.locked_by) {
            _ if !locked => None,
            (EntryKey::Coding, Some(EntryKey::Desktop)) => Some(
                "Connect a machine first — coding sessions run on the desktop app or a \
                 registered server.",
            ),
            (EntryKey::Coding, Some(EntryKey::Github)) => {
                Some("Connect a GitHub repo first. Coding sessions need a repo-backed board.")
            }
            (EntryKey::Coding, Some(EntryKey::Board)) => {
                Some("Create a board with a repository first.")
            }
            (EntryKey::Action, _) => Some(
                "Connect a machine first — the action creator runs on the desktop app or \
                 a registered server.",
            ),
            (EntryKey::Widget, _) => {
                Some("Create a board first. Widget feedback lands there as issues.")
            }
            _ => None,
        };

        // State glyph: green check / lock / bordered step number (the web
        // card's exact vocabulary; `loading` renders every card neutral).
        let glyph: gpui::AnyElement = if !loading && entry.state == EntryState::Done {
            Icon::new(registry::UI_SELECTED)
                .small()
                .text_color(theme::tokens::GREEN.to_hsla())
                .into_any_element()
        } else if locked {
            Icon::new(registry::UI_PRIVATE).small().text_color(muted).into_any_element()
        } else {
            div()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(format!("{}", index + 1)))
                .into_any_element()
        };

        let cta: Option<gpui::AnyElement> = if locked || (!loading && entry.state == EntryState::Done)
        {
            None
        } else {
            self.entry_cta(index, entry.key, team_id, cx)
        };

        crate::surface::glass_card()
            .w_full()
            .min_w_0()
            .p_4()
            .gap_2()
            .when(locked, |this| this.opacity(0.6))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_shrink_0().child(glyph))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(Icon::new(icon).small().text_color(muted)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(hint.unwrap_or(description)),
            )
            .children(cta.map(|cta| h_flex().mt_1().gap_2().flex_wrap().items_center().child(cta)))
            .into_any_element()
    }

    /// The per-entry call to action. Where the web links a route, the desktop
    /// opens the matching dialog/settings section; the owner-only web-only
    /// settings pages (widget, helpdesk) open in the browser.
    fn entry_cta(
        &self,
        index: usize,
        key: EntryKey,
        team_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let team = team_id.to_string();
        let muted = cx.theme().muted_foreground;
        Some(match key {
            // This IDE is the desktop app: nothing to download, the step
            // completes on its own once the device registration lands.
            EntryKey::Desktop => return None,
            EntryKey::Github => Button::new(("gs-cta-github", index))
                .primary().web_sm()
                .label("Connect GitHub")
                .on_click(|_, window, cx| {
                    crate::navigation::navigate(
                        window,
                        cx,
                        crate::navigation::Screen::Settings,
                    );
                    crate::sidebar::select_settings_section(
                        window,
                        cx,
                        crate::settings::SettingsSection::Repositories,
                    );
                })
                .into_any_element(),
            EntryKey::Invite => Button::new(("gs-cta-invite", index))
                .primary().web_sm()
                .label("Invite in team settings")
                .on_click(|_, window, cx| {
                    crate::navigation::navigate(
                        window,
                        cx,
                        crate::navigation::Screen::Settings,
                    );
                    crate::sidebar::select_settings_section(
                        window,
                        cx,
                        crate::settings::SettingsSection::Members,
                    );
                })
                .into_any_element(),
            EntryKey::Board => Button::new(("gs-cta-board", index))
                .primary().web_sm()
                .label("Create a board")
                .on_click(move |_, window, cx| {
                    crate::create_board_dialog::open(window, cx, team.clone());
                })
                .into_any_element(),
            EntryKey::Coding => Button::new(("gs-cta-coding", index))
                .primary().web_sm()
                .label("Start coding")
                .on_click(move |_, window, cx| {
                    crate::start_coding_dialog::open_for_selection(
                        window,
                        cx,
                        team.clone(),
                        Vec::new(),
                        None,
                    );
                })
                .into_any_element(),
            // The builtin creator run — the Actions page's "New action".
            EntryKey::Action => Button::new(("gs-cta-action", index))
                .primary().web_sm()
                .icon(Icon::new(registry::ACTION_CREATE))
                .label("New action")
                .on_click(move |_, window, cx| {
                    crate::start_coding_dialog::open_for_create_action(window, cx, team.clone());
                })
                .into_any_element(),
            EntryKey::Server => Button::new(("gs-cta-server", index))
                .primary().web_sm()
                .label("Add a server")
                .on_click(|_, window, cx| {
                    crate::machines::open_add_server_dialog(window, cx);
                })
                .into_any_element(),
            EntryKey::Widget => {
                let url = team_settings_url(&team, "widget", cx)?;
                Button::new(("gs-cta-widget", index))
                    .primary().web_sm()
                    .icon(Icon::new(registry::UI_EXTERNAL_LINK))
                    .label("Set up in team settings")
                    .on_click(move |_, _, cx| crate::settings::open_url(cx, url.clone()))
                    .into_any_element()
            }
            EntryKey::Helpdesk => {
                let url = team_settings_url(&team, "widget", cx)?;
                Button::new(("gs-cta-helpdesk", index))
                    .primary().web_sm()
                    .icon(Icon::new(registry::UI_EXTERNAL_LINK))
                    .label("Enable in team settings")
                    .on_click(move |_, _, cx| crate::settings::open_url(cx, url.clone()))
                    .into_any_element()
            }
            // The web renders per-client setup tabs; the desktop hands out
            // the endpoint and the docs walkthrough.
            EntryKey::Mcp => {
                let endpoint = queries::active_account(cx)
                    .map(|account| format!("{}/api/mcp", account.instance_url.trim_end_matches('/')));
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .children(endpoint.clone().map(|endpoint| {
                        Button::new(("gs-cta-mcp-copy", index))
                            .primary().web_sm()
                            .icon(Icon::new(registry::UI_COPY))
                            .label("Copy MCP URL")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(endpoint.clone()));
                            })
                    }))
                    .child(
                        Button::new(("gs-cta-mcp-docs", index))
                            .ghost().web_sm()
                            .icon(Icon::new(registry::UI_EXTERNAL_LINK))
                            .label("Setup guide")
                            .on_click(|_, _, cx| {
                                crate::settings::open_url(cx, MCP_DOCS_URL.to_string())
                            }),
                    )
                    .children(endpoint.map(|endpoint| {
                        div().text_xs().text_color(muted).child(SharedString::from(endpoint))
                    }))
                    .into_any_element()
            }
        })
    }
}

/// `{instance}/t/{slug}/settings/{section}` for the active account — the
/// billing-settings handoff recipe (`team_general.rs`). `None` until the team
/// row's slug has synced.
fn team_settings_url(team_id: &str, section: &str, cx: &App) -> Option<String> {
    let slug = Store::global(cx)
        .collections()
        .teams
        .read(cx)
        .get(team_id)
        .and_then(|team| team.slug.clone())?;
    let account = queries::active_account(cx)?;
    Some(format!(
        "{}/t/{slug}/settings/{section}",
        account.instance_url.trim_end_matches('/')
    ))
}

impl Render for GettingStartedView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let team_id = active_team_id(&self.nav, cx);

        let body: gpui::AnyElement = match team_id {
            None => div()
                .text_sm()
                .text_color(muted)
                .child("Waiting for your team to sync…")
                .into_any_element(),
            Some(team_id) => {
                // Rendering the page IS its visibility signal (fast cadence).
                let snapshot = self.progress.update(cx, |progress, cx| {
                    progress.ensure(&team_id, true, cx);
                    progress.snapshot(&team_id, cx)
                });
                let Snapshot {
                    loading,
                    entries,
                    done,
                    total,
                    complete,
                } = snapshot;

                // EXP-548: no dismissal — the moment the last entry completes
                // the rail entry is gone, so the page leaves too (the web
                // sheet unmounts with its button). Deferred: never mutate the
                // navigation from inside a render pass.
                if complete {
                    window.defer(cx, |window, cx| set_screen(window, cx, None));
                }

                let progress: gpui::AnyElement = if loading {
                    div().into_any_element()
                } else {
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .text_color(muted)
                                .child(SharedString::from(format!("{done}/{total} done"))),
                        )
                        .child(
                            div()
                                .h(px(6.))
                                .w(px(192.))
                                .rounded_full()
                                .bg(cx.theme().border)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative((done as f32 / total.max(1) as f32).min(1.)))
                                        .rounded_full()
                                        .bg(cx.theme().primary),
                                ),
                        )
                        .into_any_element()
                };

                let cards: Vec<gpui::AnyElement> = entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| self.entry_card(index, entry, &team_id, loading, cx))
                    .collect();

                // NO `w_full` (EXP-508): a percent width on the centered
                // column's direct child resolves against the UNCLAMPED
                // ancestor available width at wide windows (the EXP-436
                // leak that broke the Actions page); auto width + the
                // column's flex-col stretch size it to the capped column.
                v_flex()
                    .min_w_0()
                    .gap_4()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Getting started"),
                                    )
                                    .child(div().text_sm().text_color(muted).child(
                                        "Set up the coding loop, collect feedback from your \
                                         site, and connect your tools.",
                                    )),
                            )
                            .child(progress),
                    )
                    .children(cards)
                    .into_any_element()
            }
        };

        let column = v_flex().w_full().min_w_0().px_4().py_4().gap_4().child(body);

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .text_color(cx.theme().foreground)
            .child(crate::scroll_pane::v_scroll_pane(
                "getting-started-scroll",
                &self.scroll,
                div()
                    .w_full()
                    .min_w_0()
                    .child(column.w_full().max_w(px(720.)).mx_auto()),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: Gates = Gates {
        can_manage_widgets: true,
        is_owner: true,
        can_manage_members: true,
    };
    const MEMBER: Gates = Gates {
        can_manage_widgets: false,
        is_owner: false,
        can_manage_members: false,
    };

    fn entry(signals: &Signals, gates: Gates, key: EntryKey) -> Option<Entry> {
        derive_entries(signals, gates)
            .into_iter()
            .find(|entry| entry.key == key)
    }

    fn all_done() -> Signals {
        Signals {
            has_desktop_device: true,
            has_server_device: true,
            github_installed: true,
            has_invited_team: true,
            has_board: true,
            has_repo_board: true,
            has_coding_session: true,
            has_action: true,
            helpdesk_enabled: true,
            has_widget: true,
            mcp_connected: true,
        }
    }

    // Web `getting-started-cards.test.ts`, case for case.

    #[test]
    fn owner_order_is_the_web_order() {
        let keys: Vec<EntryKey> = derive_entries(&Signals::default(), OWNER)
            .iter()
            .map(|entry| entry.key)
            .collect();
        assert_eq!(
            keys,
            vec![
                EntryKey::Desktop,
                EntryKey::Github,
                EntryKey::Invite,
                EntryKey::Board,
                EntryKey::Coding,
                EntryKey::Action,
                EntryKey::Server,
                EntryKey::Widget,
                EntryKey::Helpdesk,
                EntryKey::Mcp,
            ]
        );
    }

    #[test]
    fn members_get_six_entries() {
        let keys: Vec<EntryKey> = derive_entries(&Signals::default(), MEMBER)
            .iter()
            .map(|entry| entry.key)
            .collect();
        assert_eq!(
            keys,
            vec![
                EntryKey::Desktop,
                EntryKey::Github,
                EntryKey::Board,
                EntryKey::Coding,
                EntryKey::Server,
                EntryKey::Mcp,
            ]
        );
    }

    #[test]
    fn everything_starts_undone_with_the_web_locks() {
        let none = Signals::default();
        let entries = derive_entries(&none, OWNER);
        assert_eq!(entries.len(), 10);
        assert!(entries.iter().all(|entry| entry.state != EntryState::Done));
        let coding = entry(&none, OWNER, EntryKey::Coding).unwrap();
        assert_eq!((coding.state, coding.locked_by), (EntryState::Locked, Some(EntryKey::Desktop)));
        let action = entry(&none, OWNER, EntryKey::Action).unwrap();
        assert_eq!((action.state, action.locked_by), (EntryState::Locked, Some(EntryKey::Desktop)));
        let widget = entry(&none, OWNER, EntryKey::Widget).unwrap();
        assert_eq!((widget.state, widget.locked_by), (EntryState::Locked, Some(EntryKey::Board)));
        assert_eq!(entry(&none, OWNER, EntryKey::Helpdesk).unwrap().state, EntryState::Available);
    }

    #[test]
    fn coding_lock_chain_walks_desktop_then_github_then_board() {
        let with_device = Signals {
            has_desktop_device: true,
            ..Signals::default()
        };
        assert_eq!(
            entry(&with_device, OWNER, EntryKey::Coding).unwrap().locked_by,
            Some(EntryKey::Github)
        );
        let server_only = Signals {
            has_server_device: true,
            ..Signals::default()
        };
        assert_eq!(
            entry(&server_only, OWNER, EntryKey::Coding).unwrap().locked_by,
            Some(EntryKey::Github)
        );
        let github = Signals {
            has_desktop_device: true,
            github_installed: true,
            has_board: true,
            ..Signals::default()
        };
        assert_eq!(
            entry(&github, OWNER, EntryKey::Coding).unwrap().locked_by,
            Some(EntryKey::Board)
        );
        let ready = Signals {
            has_repo_board: true,
            ..github
        };
        let coding = entry(&ready, OWNER, EntryKey::Coding).unwrap();
        assert_eq!((coding.state, coding.locked_by), (EntryState::Available, None));
        // Repo board but no machine: still the desktop feeder.
        let no_machine = Signals {
            github_installed: true,
            has_board: true,
            has_repo_board: true,
            ..Signals::default()
        };
        assert_eq!(
            entry(&no_machine, OWNER, EntryKey::Coding).unwrap().locked_by,
            Some(EntryKey::Desktop)
        );
    }

    #[test]
    fn action_unlocks_with_any_machine_and_completes_from_a_row() {
        let server = Signals {
            has_server_device: true,
            ..Signals::default()
        };
        assert_eq!(entry(&server, OWNER, EntryKey::Action).unwrap().state, EntryState::Available);
        let desktop = Signals {
            has_desktop_device: true,
            ..Signals::default()
        };
        assert_eq!(entry(&desktop, OWNER, EntryKey::Action).unwrap().state, EntryState::Available);
        let done = Signals {
            has_action: true,
            ..Signals::default()
        };
        let action = entry(&done, OWNER, EntryKey::Action).unwrap();
        assert_eq!((action.state, action.locked_by), (EntryState::Done, None));
    }

    #[test]
    fn done_beats_locks() {
        let signals = Signals {
            has_coding_session: true,
            has_widget: true,
            ..Signals::default()
        };
        assert_eq!(entry(&signals, OWNER, EntryKey::Coding).unwrap().state, EntryState::Done);
        assert_eq!(entry(&signals, OWNER, EntryKey::Widget).unwrap().state, EntryState::Done);
    }

    #[test]
    fn counts_done_against_the_viewers_own_total() {
        let signals = Signals {
            has_desktop_device: true,
            github_installed: true,
            has_invited_team: true,
            has_board: true,
            has_repo_board: true,
            has_coding_session: true,
            helpdesk_enabled: true,
            mcp_connected: true,
            ..Signals::default()
        };
        let count = |gates| {
            let entries = derive_entries(&signals, gates);
            (
                entries.iter().filter(|e| e.state == EntryState::Done).count(),
                entries.len(),
            )
        };
        // Owner: action + server + widget open → 7/10; member: server open → 5/6.
        assert_eq!(count(OWNER), (7, 10));
        assert_eq!(count(MEMBER), (5, 6));
    }

    #[test]
    fn complete_only_when_every_visible_entry_is_done() {
        assert!(is_complete(&derive_entries(&all_done(), OWNER)));
        assert!(is_complete(&derive_entries(&all_done(), MEMBER)));
        let no_action = Signals {
            has_action: false,
            ..all_done()
        };
        assert!(!is_complete(&derive_entries(&no_action, OWNER)));
        // A member never sees the action entry, so its signal cannot hold them.
        assert!(is_complete(&derive_entries(&no_action, MEMBER)));
        assert!(!is_complete(&[]));
    }
}
