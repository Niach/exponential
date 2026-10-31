//! Getting started (EXP-1270, EXP-1259 D6) — the desktop mirror of the ONE
//! block the contract `packages/domain-contract/fixtures/getting-started.json`
//! specifies ×4: a header row (title, the muted `{done} of {total}`, a
//! progress meter, a trailing Docs button) in the group band, over flat
//! flat rows (no dividers, EXP-1320), one per visible step: done rows muted with a green
//! check, the ONE next row expanded (its `why` + a primary `cta` button),
//! later rows plain with a quiet chevron ([`GettingStartedList`]).
//!
//! [`derive_getting_started`] is the pure model (web `deriveGettingStarted`),
//! replayed against every fixture case below. The block shows under the empty
//! board's empty state and the no-board team center ([`inline_list`]), on
//! the Agent home while the team has no board, and behind the rail's
//! "Getting started" entry, which opens it in a centred dialog
//! ([`open_dialog`]). It disappears once `complete`.
//!
//! Signals: boards / coding sessions / members / invites / devices are live
//! synced collections; the GitHub install state, the owner-only widget list
//! and the MCP grant/key pair are tRPC one-shots. They live in ONE app-global
//! [`GettingStartedProgress`] entity shared by every surface, polled fast
//! while the dialog is up and slowly otherwise, and not at all once complete.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, relative, size, App, AppContext as _, ClickEvent, Entity, FontWeight, Global,
    InteractiveElement as _, IntoElement, ParentElement, Render, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon,
};
use sync::Store;
use theme::tokens as t;

use crate::controls::{ide_chrome, WebControl as _};
use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Cadence while the dialog is up: often enough that "I just connected
/// GitHub / ran the server one-liner" lands while the user is looking.
const DIALOG_POLL_INTERVAL: Duration = Duration::from_secs(15);
/// Background cadence for the rail entry and the inline block.
const RAIL_POLL_INTERVAL: Duration = Duration::from_secs(60);
/// A dialog render this recent means the dialog is up.
const VISIBLE_GRACE: Duration = Duration::from_secs(40);

/// The device-setup block's words (EXP-1169) that started life on the old
/// checklist: the desktop and server cards of [`crate::device_setup`] still
/// read them, byte-identical with web `DEVICE_CARD_COPY`.
pub(crate) mod copy {
    pub const DESKTOP_TITLE: &str = "Get the desktop app";
    pub const DESKTOP_DESCRIPTION: &str = "Runs agents on your machine and registers it as one of your devices.";
    /// The Add device dialog's desktop card labels its download button with
    /// it (EXP-1169).
    pub const DESKTOP_ACTION: &str = "Download the desktop app";

    pub const SERVER_TITLE: &str = "Set up a server";
    pub const SERVER_DESCRIPTION: &str = "Run the headless daemon on an always-on machine to take remote Start coding requests.";
}

// ---------------------------------------------------------------------------
// The contract (getting-started.json; locked by the tests below)
// ---------------------------------------------------------------------------

pub(crate) const TITLE: &str = "Getting started";
pub(crate) const DOCS_LABEL: &str = "Docs";
/// The Docs button's icon concept.
pub(crate) const DOCS_ICON: &str = "ui-docs";
pub(crate) const DOCS_URL: &str = "https://exponential.at/docs/getting-started/";
/// The progress meter's width.
pub(crate) const METER_W: f32 = 160.;
/// The small label trailing the expanded next row.
pub(crate) const NEXT_LABEL: &str = "Next";
/// The block's max width; the dialog's content width.
pub(crate) const MAX_W: f32 = 560.;
/// `row`: the min row height, the step icon, the horizontal padding and the
/// expanded row's vertical padding.
pub(crate) const ROW_H: f32 = 44.;
pub(crate) const ROW_ICON: f32 = 16.;
pub(crate) const ROW_PADDING: f32 = 16.;
pub(crate) const ROW_EXPANDED_PADDING: f32 = 14.;

/// `testIds`.
pub(crate) const TEST_ID_BLOCK: &str = "getting-started";
pub(crate) const TEST_ID_DOCS: &str = "getting-started-docs";
pub(crate) const TEST_ID_CTA: &str = "getting-started-cta";

/// `testIds.row` for `key`.
pub(crate) fn row_test_id(key: StepKey) -> String {
    format!("getting-started-row-{}", key.as_str())
}

/// `progress`: `{done} of {total}`.
pub(crate) fn progress_label(done: usize, total: usize) -> String {
    format!("{done} of {total}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StepKey {
    Board,
    Github,
    Device,
    Run,
    Invite,
    Server,
    Mcp,
    Widget,
}

impl StepKey {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            StepKey::Board => "board",
            StepKey::Github => "github",
            StepKey::Device => "device",
            StepKey::Run => "run",
            StepKey::Invite => "invite",
            StepKey::Server => "server",
            StepKey::Mcp => "mcp",
            StepKey::Widget => "widget",
        }
    }
}

/// What a row click runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    CreateBoard,
    Github,
    AddDevice,
    Agent,
    Invite,
    AddServer,
    Mcp,
    Widget,
}

impl Target {
    /// The fixture's target name (the model test compares whole objects).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Target::CreateBoard => "create-board",
            Target::Github => "github",
            Target::AddDevice => "add-device",
            Target::Agent => "agent",
            Target::Invite => "invite",
            Target::AddServer => "add-server",
            Target::Mcp => "mcp",
            Target::Widget => "widget",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Pointer,
    /// The phones' list (the fixture's phone cases; the IDE is a pointer).
    #[cfg_attr(not(test), allow(dead_code))]
    Phone,
}

/// A step's `visibleWhen`: the permission that shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Permission {
    CanManageMembers,
    CanManageWidgets,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Step {
    pub key: StepKey,
    pub title: &'static str,
    /// The icon CONCEPT (`registry::concept_by_name`).
    pub icon: &'static str,
    pub target: Target,
    pub needs_device: bool,
    pub visible_when: Option<Permission>,
    pub phones: bool,
    /// The fixture's `doneWhen`, verbatim; [`step_done`] evaluates it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub done_when: &'static str,
    /// One sentence under the expanded next row's title.
    pub why: &'static str,
    /// The expanded next row's button label.
    pub cta: &'static str,
}

/// The eight steps, in contract order.
pub(crate) const STEPS: [Step; 8] = [
    Step {
        key: StepKey::Board,
        title: "Create a board",
        icon: "ui-folder",
        target: Target::CreateBoard,
        needs_device: false,
        visible_when: None,
        phones: true,
        done_when: "hasBoard",
        why: "A board holds the issues your agents work on.",
        cta: "Create a board",
    },
    Step {
        key: StepKey::Github,
        title: "Connect GitHub",
        icon: "ui-github",
        target: Target::Github,
        needs_device: false,
        visible_when: None,
        phones: true,
        done_when: "githubInstalled",
        why: "Sessions branch, push and open pull requests on your repositories.",
        cta: "Connect GitHub",
    },
    Step {
        key: StepKey::Device,
        title: "Add a device",
        icon: "ui-device",
        target: Target::AddDevice,
        needs_device: false,
        visible_when: None,
        phones: true,
        done_when: "hasDesktopDevice || hasServerDevice",
        why: "Sessions run on your computer. The desktop app registers it.",
        cta: "Add a device",
    },
    Step {
        key: StepKey::Run,
        title: "Start a session",
        icon: "action-run",
        target: Target::Agent,
        needs_device: true,
        visible_when: None,
        phones: true,
        done_when: "hasCodingSession",
        why: "Pick an issue or type what to build, then watch it work.",
        cta: "Start a session",
    },
    Step {
        key: StepKey::Invite,
        title: "Invite your team",
        icon: "ui-invite",
        target: Target::Invite,
        needs_device: false,
        visible_when: Some(Permission::CanManageMembers),
        phones: true,
        done_when: "hasInvitedTeam",
        why: "Teammates see every board, session and review.",
        cta: "Invite your team",
    },
    Step {
        key: StepKey::Server,
        title: "Set up a server",
        icon: "ui-server",
        target: Target::AddServer,
        needs_device: false,
        visible_when: None,
        phones: true,
        done_when: "hasServerDevice",
        why: "An always-on machine takes sessions while your laptop is closed.",
        cta: "Set up a server",
    },
    Step {
        key: StepKey::Mcp,
        title: "Connect your tools",
        icon: "ui-mcp",
        target: Target::Mcp,
        needs_device: false,
        visible_when: None,
        phones: false,
        done_when: "mcpConnected",
        why: "Claude, ChatGPT or Cursor read and file issues through MCP.",
        cta: "Connect your tools",
    },
    Step {
        key: StepKey::Widget,
        title: "Feedback widget",
        icon: "ui-widget",
        target: Target::Widget,
        needs_device: false,
        visible_when: Some(Permission::CanManageWidgets),
        phones: false,
        done_when: "hasWidget",
        why: "A snippet on your site turns feedback into issues.",
        cta: "Feedback widget",
    },
];

/// A concept name's glyph (every name here is locked to the registry by
/// the tests below).
fn concept(name: &str) -> ExpIcon {
    registry::concept_by_name(name).unwrap_or(registry::UI_DOCS)
}

pub(crate) fn step(key: StepKey) -> &'static Step {
    STEPS
        .iter()
        .find(|step| step.key == key)
        .expect("every key has a step")
}

/// The fixture's `signals`, field for field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Signals {
    pub has_board: bool,
    pub github_installed: bool,
    /// The synced `devices` shape (own rows) has a desktop-kind device.
    pub has_desktop_device: bool,
    /// The synced `devices` shape (own rows) has a server-kind device.
    pub has_server_device: bool,
    /// Any session in the team (running or ended; agent roots excluded).
    pub has_coding_session: bool,
    /// members ≥ 2 or any invite row — proof the team was shared.
    pub has_invited_team: bool,
    /// An MCP OAuth grant exists OR the user holds a personal API key.
    pub mcp_connected: bool,
    /// widgets.list non-empty (owner-only signal).
    pub has_widget: bool,
}

impl Signals {
    fn has_device(&self) -> bool {
        self.has_desktop_device || self.has_server_device
    }
}

/// The fixture's `permissions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Permissions {
    pub can_manage_members: bool,
    pub can_manage_widgets: bool,
}

impl Permissions {
    /// The IDE's role gate: both surfaces are owner-only.
    pub(crate) fn for_role(is_owner: bool) -> Self {
        Self {
            can_manage_members: is_owner,
            can_manage_widgets: is_owner,
        }
    }

    fn allows(&self, permission: Permission) -> bool {
        match permission {
            Permission::CanManageMembers => self.can_manage_members,
            Permission::CanManageWidgets => self.can_manage_widgets,
        }
    }
}

/// A step's `doneWhen`.
pub(crate) fn step_done(key: StepKey, signals: &Signals) -> bool {
    match key {
        StepKey::Board => signals.has_board,
        StepKey::Github => signals.github_installed,
        StepKey::Device => signals.has_device(),
        StepKey::Run => signals.has_coding_session,
        StepKey::Invite => signals.has_invited_team,
        StepKey::Server => signals.has_server_device,
        StepKey::Mcp => signals.mcp_connected,
        StepKey::Widget => signals.has_widget,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub key: StepKey,
    pub done: bool,
    /// The step's target, or `add-device` for a `needsDevice` step while no
    /// device exists.
    pub target: Target,
    /// The first open step: the expanded row.
    pub next: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Model {
    pub entries: Vec<Entry>,
    pub done: usize,
    pub total: usize,
    pub progress: String,
    pub complete: bool,
}

/// Web `deriveGettingStarted`: the visible steps in contract order (a
/// `visibleWhen` permission the viewer lacks or a `phones: false` step on a
/// phone drops out), each with its done flag and click target, the first
/// open one marked `next`.
pub(crate) fn derive_getting_started(
    signals: &Signals,
    permissions: Permissions,
    platform: Platform,
) -> Model {
    let mut entries: Vec<Entry> = STEPS
        .iter()
        .filter(|step| step.visible_when.is_none_or(|p| permissions.allows(p)))
        .filter(|step| platform == Platform::Pointer || step.phones)
        .map(|step| Entry {
            key: step.key,
            done: step_done(step.key, signals),
            target: if step.needs_device && !signals.has_device() {
                Target::AddDevice
            } else {
                step.target
            },
            next: false,
        })
        .collect();
    if let Some(next) = entries.iter_mut().find(|entry| !entry.done) {
        next.next = true;
    }
    let done = entries.iter().filter(|entry| entry.done).count();
    let total = entries.len();
    Model {
        entries,
        done,
        total,
        progress: progress_label(done, total),
        complete: total > 0 && done == total,
    }
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

/// One derived snapshot for a team — what every surface renders.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    /// Some signal source has not answered yet: nothing renders (an empty
    /// state must not flash a half-answered block).
    pub loading: bool,
    pub model: Model,
}

impl Snapshot {
    /// Never complete while loading.
    pub(crate) fn complete(&self) -> bool {
        !self.loading && self.model.complete
    }
}

pub struct GettingStartedProgress {
    /// Team-keyed so a switch back is instant; a team's own answers never
    /// leak to another.
    teams: HashMap<String, TeamAnswers>,
    /// mcpGrants.hasAny || personal keys > 0; `None` until answered.
    mcp_connected: Option<bool>,
    /// The team the poll loop refreshes — the last one a caller asked for.
    wanted_team: Option<String>,
    fetch_seq: u64,
    /// A round is in flight (its answers still pending).
    inflight: bool,
    last_fetch_at: Option<Instant>,
    polling: bool,
    /// Last render of the dialog — picks the poll cadence.
    dialog_rendered: Option<Instant>,
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
            // Coarse-grained on purpose: the block reads a handful of
            // collections and derives booleans; observers of THIS entity
            // re-render on any of them.
            let subscriptions = store.observe_collections(cx);
            GettingStartedProgress {
                teams: HashMap::new(),
                mcp_connected: None,
                wanted_team: None,
                fetch_seq: 0,
                inflight: false,
                last_fetch_at: None,
                polling: false,
                dialog_rendered: None,
                _subscriptions: subscriptions,
            }
        });
        cx.set_global(GettingStartedProgressGlobal(entity.clone()));
        entity
    }

    /// Ask for `team_id`'s signals: fetches right away on a team change and
    /// (re)starts the poll loop unless the block is already complete.
    /// Rendering IS the liveness signal, like MachinesSection.
    fn ensure(&mut self, team_id: &str, from_dialog: bool, cx: &mut gpui::Context<Self>) {
        if from_dialog {
            self.dialog_rendered = Some(Instant::now());
        }
        let team_changed = self.wanted_team.as_deref() != Some(team_id);
        if team_changed {
            self.wanted_team = Some(team_id.to_string());
        }
        // Nothing left to detect once every step is done; a later regression
        // (a trashed board, a removed device) re-renders through the
        // collection observers and restarts it here.
        let snapshot = self.snapshot(team_id, cx);
        if snapshot.complete() {
            return;
        }
        // Fetch now on a team change / a fresh start, and ALSO while still
        // loading with nothing in flight — the first round may have run
        // before the session or the viewer's member row was there. The
        // throttle keeps a failing query from hammering.
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
                            .dialog_rendered
                            .is_some_and(|at| at.elapsed() < VISIBLE_GRACE)
                        {
                            DIALOG_POLL_INTERVAL
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
                        .is_some_and(|team| this.snapshot(team, cx).complete());
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

    /// One seq-guarded round: the MCP pair (user-level), the wanted team's
    /// `integrations.github.status`, and — owners only — `widgets.list`.
    /// Failures keep the last answer. The device signals are NOT here: they
    /// derive from the synced `devices` shape (EXP-485).
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
                    let github = crate::github_connect::fetch_github_status(&trpc, &fetch_team);
                    let widgets = ask_widgets.then(|| api::widgets::list(&trpc, &fetch_team));
                    let grants = api::users::mcp_grants_has_any(&trpc);
                    let keys = api::users::list_personal_api_keys(&trpc);
                    (github, widgets, grants, keys)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_seq != seq {
                    return;
                }
                this.inflight = false;
                let (github, widgets, grants, keys) = result;
                // Both halves best-effort, a failure counts as "no" for that
                // half (a setup hint, not access control).
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

    /// EXP-485: the device signals come off the SYNCED `devices` shape —
    /// `(desktop, server)` among the signed-in user's OWN rows, `None` while
    /// the shape is still Waiting.
    fn device_kinds(cx: &App) -> Option<(bool, bool)> {
        let collections = Store::global(cx).collections();
        let devices = collections.devices.read(cx);
        if !devices.is_ready() {
            return None;
        }
        // `devices.user_id` = the Better Auth user id, never `Account::id`
        // (the per-instance account key).
        let me = queries::active_account(cx)?.user_id;
        let mut desktop = false;
        let mut server = false;
        for row in devices
            .iter()
            .filter(|row| row.user_id.as_deref() == Some(me.as_str()))
        {
            match row.kind.as_deref() {
                Some("desktop") => desktop = true,
                Some("server") => server = true,
                _ => {}
            }
        }
        Some((desktop, server))
    }

    fn signals(&self, team_id: &str, cx: &App) -> Signals {
        let collections = Store::global(cx).collections();
        let boards = collections.boards_in_team(team_id, cx);
        let has_board = !boards.is_empty();
        let has_coding_session = collections
            .sessions
            .read(cx)
            .iter()
            .any(|session| session.team_id.as_deref() == Some(team_id) && session.is_thread());
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
        let answers = self.teams.get(team_id);
        let (has_desktop_device, has_server_device) =
            Self::device_kinds(cx).unwrap_or((false, false));
        Signals {
            has_board,
            // A repo-backed board proves a teammate connected GitHub for the
            // team: the step is done for everyone, not only for whoever
            // linked it.
            github_installed: answers.and_then(|a| a.github_installed) == Some(true)
                || boards.iter().any(|board| board.repository_id.is_some()),
            has_desktop_device,
            has_server_device,
            has_coding_session,
            has_invited_team: members >= 2 || has_invite,
            mcp_connected: self.mcp_connected == Some(true),
            has_widget: answers.and_then(|a| a.has_widget) == Some(true),
        }
    }

    /// Derive the team's model + the `loading` flag.
    pub(crate) fn snapshot(&self, team_id: &str, cx: &App) -> Snapshot {
        let permissions = Permissions::for_role(crate::settings::is_owner(cx, team_id));
        let model = derive_getting_started(
            &self.signals(team_id, cx),
            permissions,
            Platform::Pointer,
        );
        let answers = self.teams.get(team_id);
        // Nothing until every signal source has answered — checks that pop
        // in one by one read as state changes. Membership must have synced
        // too (`is_owner` is transiently false before the viewer's own row
        // lands).
        let membership_resolved = crate::settings::my_membership(cx, team_id).is_some();
        let loading = !membership_resolved
            || Self::device_kinds(cx).is_none()
            || self.mcp_connected.is_none()
            || answers.and_then(|a| a.github_installed).is_none()
            || (permissions.can_manage_widgets && answers.and_then(|a| a.has_widget).is_none());
        Snapshot { loading, model }
    }
}

/// Whether the rail shows the Getting-started entry: signed in with an
/// active team, the signals answered and the block not complete. Rendering
/// the rail is what keeps the signals fresh.
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
        !snapshot.loading && !snapshot.complete()
    })
}

// ---------------------------------------------------------------------------
// Targets
// ---------------------------------------------------------------------------

/// Run a row's target in `window`. From inside the dialog the dialog closes
/// first and the target runs in the window that opened it (dialogs never
/// nest, and a navigation needs the opener's screens).
fn run_target(target: Target, team_id: String, window: &mut Window, cx: &mut App) {
    let in_dialog =
        native_dialog::opener_of_window(window.window_handle().window_id(), cx).is_some();
    if in_dialog {
        native_dialog::close_then(window, cx, move |window, cx| {
            run_target_here(target, team_id, window, cx)
        });
    } else {
        run_target_here(target, team_id, window, cx);
    }
}

fn open_settings(section: crate::settings::SettingsSection, window: &mut Window, cx: &mut App) {
    crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
    crate::sidebar::select_settings_section(window, cx, section);
}

fn run_target_here(target: Target, team_id: String, window: &mut Window, cx: &mut App) {
    use crate::settings::SettingsSection;
    match target {
        Target::CreateBoard => crate::create_board_dialog::open(window, cx, team_id),
        Target::Github => open_settings(SettingsSection::Repositories, window, cx),
        // The ONE device-setup block (desktop card over the server card).
        Target::AddDevice | Target::AddServer => {
            crate::machines::open_add_server_dialog(window, cx)
        }
        // Home, no subject: the composer is the page.
        Target::Agent => crate::navigation::navigate_to_chat(
            window,
            cx,
            crate::navigation::ChatSeed::default(),
        ),
        // The IDE has no invite dialog: team settings' Members pane holds
        // the invite form.
        Target::Invite => open_settings(SettingsSection::Members, window, cx),
        Target::Mcp => open_settings(SettingsSection::Mcp, window, cx),
        Target::Widget => open_settings(SettingsSection::Widget, window, cx),
    }
}

// ---------------------------------------------------------------------------
// The block
// ---------------------------------------------------------------------------

/// THE block (web `GettingStartedList`): the group band (title, progress,
/// meter, Docs) over flat rows (no dividers, EXP-1320), one per entry, at most
/// [`MAX_W`] wide and centred in its column. Pure over `model`, so the
/// styleguide draws it from a fixture case with `team_id` `None` (inert).
#[derive(IntoElement)]
pub(crate) struct GettingStartedList {
    model: Model,
    team_id: Option<String>,
}

impl GettingStartedList {
    pub(crate) fn new(model: Model, team_id: Option<String>) -> Self {
        Self { model, team_id }
    }
}

impl RenderOnce for GettingStartedList {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let rows: Vec<gpui::AnyElement> = self
            .model
            .entries
            .iter()
            .map(|entry| step_row(entry, self.team_id.as_deref(), cx))
            .collect();
        v_flex()
            .id(TEST_ID_BLOCK)
            .w_full()
            .max_w(px(MAX_W))
            .mx_auto()
            .min_w_0()
            .child(band(&self.model, cx))
            .children(rows)
    }
}

/// The group band: title, `{done} of {total}`, the meter, Docs trailing.
fn band(model: &Model, cx: &App) -> gpui::AnyElement {
    let theme = cx.theme();
    let chrome = ide_chrome();
    let fill = (model.done as f32 / model.total.max(1) as f32).min(1.);
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_3()
        .px_3()
        .py_1p5()
        .mb_1()
        .rounded(px(chrome.radius_md))
        .bg(chrome.fill_section)
        .child(
            div()
                .flex_none()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .child(TITLE),
        )
        .child(
            div()
                .flex_none()
                .text_sm()
                .whitespace_nowrap()
                .text_color(theme.muted_foreground)
                .child(SharedString::from(model.progress.clone())),
        )
        .child(
            // Shrinks before the title does on a narrow column.
            div()
                .min_w_0()
                .h(px(4.))
                .w(px(METER_W))
                .rounded_full()
                .bg(theme.border)
                .child(div().h_full().w(relative(fill)).rounded_full().bg(theme.primary)),
        )
        .child(div().flex_1())
        .child(
            Button::new(TEST_ID_DOCS)
                .outline()
                .web_sm()
                .icon(Icon::new(concept(DOCS_ICON)))
                .label(DOCS_LABEL)
                .on_click(|_, _, cx| crate::settings::open_url(cx, DOCS_URL.to_string())),
        )
        .into_any_element()
}

/// One step row: flat, no hairline (EXP-1320).
fn step_row(
    entry: &Entry,
    team_id: Option<&str>,
    cx: &App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let hover = theme.list_hover;
    let chrome = ide_chrome();
    let step = step(entry.key);
    let target = entry.target;
    let team = team_id.map(str::to_string);
    let icon = Icon::new(concept(step.icon))
        .size(px(ROW_ICON))
        .flex_none()
        .text_color(if entry.done { muted } else { foreground });
    let title = div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_sm()
        .when(entry.next, |this| this.font_weight(FontWeight::MEDIUM))
        .text_color(if entry.done { muted } else { foreground })
        .child(step.title);
    let row = if entry.next {
        let next = div()
            .flex_none()
            .px_1p5()
            .rounded(px(4.))
            .bg(chrome.fill_section)
            .text_xs()
            .text_color(muted)
            .child(NEXT_LABEL);
        let cta_team = team.clone();
        let cta = Button::new(TEST_ID_CTA)
            .primary()
            .web_sm()
            .label(step.cta)
            .when_some(cta_team, |this, team| {
                this.on_click(move |_: &ClickEvent, window, cx| {
                    // The row runs the same target: once per click.
                    cx.stop_propagation();
                    run_target(target, team.clone(), window, cx);
                })
            });
        v_flex()
            .id(SharedString::from(row_test_id(entry.key)))
            .w_full()
            .min_w_0()
            .px(px(ROW_PADDING))
            .py(px(ROW_EXPANDED_PADDING))
            .gap_1()
            .bg(chrome.fill_active)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .child(icon)
                    .child(title)
                    .child(next),
            )
            .child(
                // Under the title: the icon's width and the gap in.
                v_flex()
                    .w_full()
                    .min_w_0()
                    .pl(px(ROW_ICON + 12.))
                    .items_start()
                    .gap_2p5()
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_size(px(13.))
                            .text_color(muted)
                            .child(step.why),
                    )
                    .child(cta),
            )
    } else {
        let trailing: gpui::AnyElement = if entry.done {
            Icon::new(registry::UI_CHECK)
                .size(px(ROW_ICON))
                .text_color(t::GREEN.to_hsla())
                .into_any_element()
        } else {
            Icon::new(registry::UI_CHEVRON_RIGHT)
                .size(px(14.))
                .text_color(muted.opacity(0.45))
                .into_any_element()
        };
        let row = h_flex()
            .id(SharedString::from(row_test_id(entry.key)))
            .w_full()
            .min_w_0()
            .min_h(px(ROW_H))
            .px(px(ROW_PADDING))
            .gap_3()
            .items_center()
            .child(icon)
            .child(title)
            .child(trailing)
            .when(team.is_some(), |this| this.hover(move |this| this.bg(hover)));
        row
    };
    row.when_some(team, |this, team| {
        this.cursor_pointer()
            .on_click(move |_: &ClickEvent, window, cx| {
                run_target(target, team.clone(), window, cx);
            })
    })
    .into_any_element()
}

/// THE block's entry point for a team: under the empty board's empty state,
/// the no-board team center and the Agent home while the team has no board.
/// `None` while the signals are still loading and once complete.
pub(crate) fn inline_list(team_id: &str, cx: &mut App) -> Option<gpui::AnyElement> {
    let progress = GettingStartedProgress::global(cx);
    // The slow cadence: an empty board can sit on screen all day.
    let snapshot = progress.update(cx, |progress, cx| {
        progress.ensure(team_id, false, cx);
        progress.snapshot(team_id, cx)
    });
    if snapshot.loading || snapshot.complete() {
        return None;
    }
    Some(GettingStartedList::new(snapshot.model, Some(team_id.to_string())).into_any_element())
}

// ---------------------------------------------------------------------------
// The dialog (the rail entry)
// ---------------------------------------------------------------------------

/// The expanded next row's height: its padding, the title line, the `why`
/// line and the button.
const EXPANDED_ROW_H: f32 = 2. * ROW_EXPANDED_PADDING + 20. + 4. + 18. + 10. + 28.;
/// The band and its gap to the rows.
const BAND_H: f32 = 44.;

/// The dialog's content height for `entries` rows, one of them expanded
/// while the list is not complete. The shell scrolls it vertically when the
/// window is shorter.
fn dialog_content_h(entries: usize, has_next: bool) -> f32 {
    let collapsed = entries.saturating_sub(usize::from(has_next)) as f32;
    BAND_H + collapsed * ROW_H + if has_next { EXPANDED_ROW_H } else { 0. }
}

/// The rail entry's target: the block in a centred dialog over `window`,
/// [`MAX_W`] wide.
pub(crate) fn open_dialog(window: &mut Window, cx: &mut App) {
    let nav = nav_for_window(window, cx);
    let Some(team_id) = active_team_id(&nav, cx) else {
        return;
    };
    let model = GettingStartedProgress::global(cx)
        .read(cx)
        .snapshot(&team_id, cx)
        .model;
    let has_next = model.entries.iter().any(|entry| entry.next);
    let viewport = window.viewport_size();
    // The shell pads the content by 16 on each side.
    let width = (MAX_W + 32.).min(f32::from(viewport.width) * 0.9);
    let height = (dialog_content_h(model.entries.len(), has_next) + 32.)
        .min(f32::from(viewport.height) * 0.9);
    let spec = DialogSpec::new(TITLE, size(px(width), px(height)));
    native_dialog::open_dialog_window(window, cx, spec, move |_window, cx| {
        let view = cx.new(|cx| GettingStartedDialog::new(team_id, cx));
        DialogContent::new(view)
    });
}

struct GettingStartedDialog {
    team_id: String,
    progress: Entity<GettingStartedProgress>,
    _subscriptions: Vec<Subscription>,
}

impl GettingStartedDialog {
    fn new(team_id: String, cx: &mut gpui::Context<Self>) -> Self {
        let progress = GettingStartedProgress::global(cx);
        // The progress entity re-notifies on every collection change, so
        // this one observer covers live + one-shot signals.
        let subscriptions = vec![cx.observe(&progress, |_, _, cx| cx.notify())];
        Self {
            team_id,
            progress,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for GettingStartedDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = self.team_id.clone();
        let snapshot = self.progress.update(cx, |progress, cx| {
            progress.ensure(&team_id, true, cx);
            progress.snapshot(&team_id, cx)
        });
        div()
            .w_full()
            .min_w_0()
            .text_color(cx.theme().foreground)
            .when(!snapshot.loading, |this| {
                this.child(GettingStartedList::new(snapshot.model, Some(team_id)))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/getting-started.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).expect("getting-started.json parses")
    }

    fn flag(object: &Value, key: &str) -> bool {
        object[key]
            .as_bool()
            .unwrap_or_else(|| panic!("{key} is a bool"))
    }

    fn model_json(model: &Model) -> Value {
        json!({
            "entries": model.entries.iter().map(|entry| json!({
                "key": entry.key.as_str(),
                "done": entry.done,
                "target": entry.target.as_str(),
                "next": entry.next,
            })).collect::<Vec<_>>(),
            "done": model.done,
            "total": model.total,
            "progress": model.progress,
            "complete": model.complete,
        })
    }

    /// Every fixture case, the WHOLE expected object.
    #[test]
    fn replays_every_fixture_case() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let s = &case["signals"];
            let signals = Signals {
                has_board: flag(s, "hasBoard"),
                github_installed: flag(s, "githubInstalled"),
                has_desktop_device: flag(s, "hasDesktopDevice"),
                has_server_device: flag(s, "hasServerDevice"),
                has_coding_session: flag(s, "hasCodingSession"),
                has_invited_team: flag(s, "hasInvitedTeam"),
                mcp_connected: flag(s, "mcpConnected"),
                has_widget: flag(s, "hasWidget"),
            };
            let p = &case["permissions"];
            let permissions = Permissions {
                can_manage_members: flag(p, "canManageMembers"),
                can_manage_widgets: flag(p, "canManageWidgets"),
            };
            let platform = match case["platform"].as_str() {
                Some("pointer") => Platform::Pointer,
                Some("phone") => Platform::Phone,
                other => panic!("{name}: unknown platform {other:?}"),
            };
            let model = derive_getting_started(&signals, permissions, platform);
            assert_eq!(model_json(&model), case["expected"], "{name}");
        }
    }

    /// The copy, the geometry, the test ids and the eight steps (with their
    /// `why` + `cta`), locked to the contract.
    #[test]
    fn constants_and_steps_match_the_contract() {
        let fixture = fixture();
        assert_eq!(fixture["title"], TITLE);
        assert_eq!(
            fixture["progress"].as_str().unwrap(),
            progress_label(usize::MAX, usize::MAX)
                .replacen(&usize::MAX.to_string(), "{done}", 1)
                .replacen(&usize::MAX.to_string(), "{total}", 1)
        );
        assert_eq!(fixture["meter"].as_f64().unwrap() as f32, METER_W);
        assert_eq!(fixture["docs"]["label"], DOCS_LABEL);
        assert_eq!(fixture["docs"]["url"], DOCS_URL);
        assert_eq!(fixture["docs"]["icon"], DOCS_ICON);
        assert!(registry::concept_by_name(DOCS_ICON).is_some());
        assert_eq!(fixture["nextLabel"], NEXT_LABEL);
        assert_eq!(fixture["maxWidth"].as_f64().unwrap() as f32, MAX_W);
        let row = &fixture["row"];
        for (key, value) in [
            ("height", ROW_H),
            ("iconSize", ROW_ICON),
            ("padding", ROW_PADDING),
            ("expandedPadding", ROW_EXPANDED_PADDING),
        ] {
            assert_eq!(row[key].as_f64().unwrap() as f32, value, "row.{key}");
        }
        let ids = &fixture["testIds"];
        assert_eq!(ids["block"], TEST_ID_BLOCK);
        assert_eq!(ids["docs"], TEST_ID_DOCS);
        assert_eq!(ids["cta"], TEST_ID_CTA);
        for step in STEPS {
            assert_eq!(
                ids["row"].as_str().unwrap().replace("{key}", step.key.as_str()),
                row_test_id(step.key)
            );
        }
        let steps = fixture["steps"].as_array().unwrap();
        assert_eq!(steps.len(), STEPS.len());
        for (expected, step) in steps.iter().zip(STEPS.iter()) {
            let key = step.key.as_str();
            assert_eq!(expected["key"], key);
            assert_eq!(expected["title"], step.title, "{key}");
            assert_eq!(expected["icon"], step.icon, "{key}");
            assert!(registry::concept_by_name(step.icon).is_some(), "{key}");
            assert_eq!(expected["target"], step.target.as_str(), "{key}");
            assert_eq!(
                expected["needsDevice"].as_bool().unwrap_or(false),
                step.needs_device,
                "{key}"
            );
            let visible_when = step.visible_when.map(|p| match p {
                Permission::CanManageMembers => "canManageMembers",
                Permission::CanManageWidgets => "canManageWidgets",
            });
            assert_eq!(expected["visibleWhen"].as_str(), visible_when, "{key}");
            assert_eq!(expected["phones"].as_bool(), Some(step.phones), "{key}");
            assert_eq!(expected["doneWhen"], step.done_when, "{key}");
            assert_eq!(expected["why"], step.why, "{key}");
            assert_eq!(expected["cta"], step.cta, "{key}");
        }
    }

    /// `doneWhen` evaluates exactly the signals its text names.
    #[test]
    fn done_when_reads_the_named_signals() {
        for step in STEPS {
            assert!(!step_done(step.key, &Signals::default()), "{:?}", step.key);
        }
        let server = Signals {
            has_server_device: true,
            ..Signals::default()
        };
        assert!(step_done(StepKey::Device, &server));
        assert!(step_done(StepKey::Server, &server));
        let desktop = Signals {
            has_desktop_device: true,
            ..Signals::default()
        };
        assert!(step_done(StepKey::Device, &desktop));
        assert!(!step_done(StepKey::Server, &desktop));
    }

    /// The dialog fits the band, the collapsed rows and the one expanded row.
    #[test]
    fn dialog_fits_the_list() {
        assert_eq!(dialog_content_h(8, true), BAND_H + 7. * ROW_H + EXPANDED_ROW_H);
        assert_eq!(dialog_content_h(6, false), BAND_H + 6. * ROW_H);
        assert!(EXPANDED_ROW_H > ROW_H);
    }
}
