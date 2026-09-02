//! EXP-481: the Device settings dialog — the desktop twin of the web's
//! `device-settings-dialog.tsx`, opened from a machines row's "Edit…".
//!
//! EXP-694: the dialog AUTOSAVES, mirroring the web one — there is no Save
//! button anywhere. The name settles for [`NAME_SAVE_DEBOUNCE`] (or commits
//! on blur/Enter); every picker and switch writes on change, the way the
//! default-device toggle always has. One write runs at a time, so a change
//! made mid-flight parks in `queued` and replays off the CONTROLS when the
//! executor frees up. Each section keeps its own write path:
//!
//! | Section        | Write path                                            |
//! |----------------|-------------------------------------------------------|
//! | Name           | `devices.rename` (registry row — works offline)       |
//! | Default        | `devices.setDefault` (EXP-622, own devices only)     |
//! | Sharing        | `devices.setShared` (server-kind own devices only)    |
//! | Agent defaults | `setLaunchDefaults` (UNCONDITIONAL — a UI edit is     |
//! |                | last-write-wins on both); the OWN device ALSO saves   |
//! |                | settings.json first through [`CodingHub::save_settings`] |
//! | Worktrees      | `devices.createCommand` (worktree_remove / _prune) —  |
//! |                | a DURABLE queue: an offline machine runs it on return |
//!
//! Data comes from the SYNCED `devices` + `device_worktrees` collections
//! (never relay presence): defaults stay editable while the machine is
//! offline ("Applies when the device comes online."), and the worktree rows
//! reflect the machine's last report. EXP-490: the dialog mirrors the LIVE
//! baseline while open — every section (name, sharing and, since EXP-696,
//! the defaults controls) re-seeds only while the user has NOT diverged from
//! the previous baseline, so a background delta never stomps a draft.
//! Queued commands
//! are polled (`devices.getCommand`) until terminal — a failure renders its
//! device-reported message inline; success shows up as the row vanishing
//! when the machine re-reports.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    div, prelude::FluentBuilder as _, px, size, App, AppContext as _, Div, Entity, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    select::Select,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use coding::CodingAgent;

use crate::coding_flow::CodingHub;
use crate::controls::WebControl as _;
use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
    AGENT_CHOICES,
};
use crate::icons::registry;
use crate::launch_options::{AgentDefaultsGroup, AgentPill, DefaultsToggle};
use crate::native_dialog::{self, AlertSpec, DialogContent, DialogSpec};
use crate::queries;
use crate::surface;

/// "Not shared" sentinel in the sharing select (the web dialog's Radix
/// sentinel twin — a select row needs a non-empty value).
const NOT_SHARED: &str = "__not_shared__";

/// Queued-command poll cadence while the dialog is open (offline machines
/// keep their commands queued server-side — poll slowly).
const COMMAND_POLL_ONLINE: Duration = Duration::from_secs(2);
const COMMAND_POLL_OFFLINE: Duration = Duration::from_secs(8);

/// EXP-694: the dialog AUTOSAVES — there is no Save button anywhere. Typing
/// settles for this long before the rename goes out (the web dialog's 800ms
/// twin); a blur commits it immediately. Everything else (the pickers, the
/// switches, the sharing row) writes straight through on change.
const NAME_SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

/// Whether a synced devices row reads ONLINE: `last_seen_at` within the
/// contract window of `now_ms`. Negative ages (clock skew — the server
/// stamped ahead of this client's clock) clamp online; unparseable stamps
/// read OFFLINE (fail-closed — an unstartable claim is the safe direction).
pub(crate) fn row_is_online(last_seen_at: Option<&str>, now_ms: i64) -> bool {
    let Some(seen) = last_seen_at.and_then(crate::comments::parse_epoch) else {
        return false;
    };
    now_ms - seen * 1_000 < domain::contract::DEVICE_ONLINE_WINDOW_MS
}

/// The agents the defaults editor covers: runnable ∪ signed-out ∪
/// already-configured, in `CodingAgent::ALL` order; an offline/quiet machine
/// falls back to the full set so its defaults stay editable (web parity).
pub(crate) fn editor_agents(
    agents: &[String],
    unauthed: &[String],
    configured: &[String],
) -> Vec<CodingAgent> {
    let known: Vec<CodingAgent> = CodingAgent::ALL
        .into_iter()
        .filter(|agent| {
            agents.iter().any(|id| id == agent.id())
                || unauthed.iter().any(|id| id == agent.id())
                || configured.iter().any(|id| id == agent.id())
        })
        .collect();
    if known.is_empty() {
        CodingAgent::ALL.to_vec()
    } else {
        known
    }
}

/// Open the dialog for a synced devices row (own devices only — the
/// machines menu never offers Edit on teammates' rows).
pub fn open(window: &mut Window, cx: &mut App, device_row_id: String) {
    let height = (window.viewport_size().height * 0.85).min(px(560.));
    let spec = DialogSpec::new("Device settings", size(px(440.), height))
        .resizable(size(px(400.), px(420.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| DeviceSettingsView::new(device_row_id, window, cx));
        DialogContent::new(view)
    });
}

/// One queued command the dialog tracks until terminal.
struct TrackedCommand {
    id: String,
    /// `prune`, `"{repo} {branch}"` for a removal, or `"login {agent}"` for
    /// an EXP-484 sign-in — the inline error/result slot.
    key: String,
}

/// EXP-484: the key an agent's queued login is tracked under.
fn login_key(agent: CodingAgent) -> String {
    format!("login {}", agent.id())
}

/// EXP-484: what a finished `agent_login` command handed back — the CLI's
/// own sign-in URL (open it anywhere) plus, for Codex's device-code flow,
/// the code to type on the machine.
struct LoginNote {
    url: String,
    code: Option<String>,
}

/// EXP-694: what a section's OPTIMISTIC dedupe advance has to be undone to
/// when its write never lands. The autosave advances the baseline before the
/// tRPC call resolves (so a pick made mid-flight still writes last-wins); a
/// failure has to put it back, or re-committing the SAME value dedupes itself
/// away and the write is never retried. One slot is enough: `run_section`
/// runs exactly one write at a time.
enum SectionRollback {
    Label(String),
    Share(String),
    Defaults(Box<coding::Settings>),
}

/// EXP-484: what a machine last reported about its agent CLIs, resolved for
/// rendering — the accounts, the usage snapshots, when the usage was taken,
/// and the device caps that decide whether a sign-in may be started here.
#[derive(Default)]
struct DeviceAgentStatus {
    accounts: coding::agent_accounts::AgentAccounts,
    usage: coding::agent_usage::AgentUsageMap,
    usage_at: Option<String>,
    caps: Vec<String>,
}

impl DeviceAgentStatus {
    /// The agents that have something to say (an account or usage), in
    /// `CodingAgent::ALL` order.
    fn reporting(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| {
                self.accounts.contains_key(agent.id()) || self.usage.contains_key(agent.id())
            })
            .collect()
    }
}

/// EXP-484: the Login / Switch-account affordance for one agent row, or
/// `None` when this client cannot start that machine's sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoginAffordance {
    pub label: &'static str,
    /// Whether the run signs OUT first (an account SWITCH).
    pub switch: bool,
}

/// Who may start a sign-in, and how it is labelled.
///
/// The OWN machine always can — the login runs in a terminal tab right here,
/// online or not. A REMOTE machine needs to be online (the command rides its
/// heartbeat), to run a build that executes `agent_login` (the cap), and not
/// to be pi: pi's sign-in is an interactive prompt inside its TUI with no
/// device-code flow to hand back, so remote sign-in refuses it outright (the
/// server does too). Mirrors the web `canLogin` rule exactly.
pub(crate) fn login_affordance(
    agent: CodingAgent,
    own: bool,
    online: bool,
    caps: &[String],
    signed_in: bool,
) -> Option<LoginAffordance> {
    let allowed = own
        || (online
            && agent != CodingAgent::Pi
            && caps.iter().any(|cap| cap == "agent-login"));
    allowed.then_some(LoginAffordance {
        label: if signed_in { "Switch account" } else { "Login" },
        switch: signed_in,
    })
}

/// EXP-484/694: what one agent's OWN tab says above the login pill, where the
/// agent is already the heading — just the ADDRESS: no `<agent> ·` prefix, no
/// `signed in as` and no ` · <plan>` tail (the plan is the agent app's
/// business, not this row's). An account with no email (pi reports a provider,
/// never an address) falls back to the bare plan, and the two negative cases
/// read as sentences. Byte-identical to the web `accountLine`
/// (`lib/agent-usage.ts`) and its iOS/Android twins.
pub(crate) fn account_line(account: Option<&coding::agent_accounts::AgentAccount>) -> String {
    match account {
        // Never probed is NOT "not signed in".
        None => "Sign-in status unknown".to_string(),
        Some(account) if !account.signed_in => "Not signed in".to_string(),
        Some(account) => {
            let email = account.email.as_deref().filter(|value| !value.is_empty());
            let plan = account.plan.as_deref().filter(|value| !value.is_empty());
            match (email, plan) {
                (Some(email), _) => email.to_string(),
                // pi reports a provider, never an address.
                (None, Some(plan)) => plan.to_string(),
                (None, None) => "signed in".to_string(),
            }
        }
    }
}

/// EXP-484/694: what THIS install's agent CLIs last reported — the hub's live
/// snapshot, with the doctor's probe standing in before the first collection
/// (a machine with agents installed is never blank). Shared with Settings →
/// Agents, which renders the same account/usage rows for the local machine.
pub(crate) fn own_agent_status(
    cx: &mut App,
) -> (
    coding::agent_accounts::AgentAccounts,
    coding::agent_usage::AgentUsageMap,
) {
    let hub = CodingHub::global(cx);
    let hub = hub.read(cx);
    let status = hub.agent_status.clone().unwrap_or_default();
    let mut accounts = status.accounts;
    if accounts.is_empty() {
        if let Some(report) = hub.doctor.report.as_ref() {
            accounts = report.agent_accounts(&coding::agent_accounts::now_iso());
        }
    }
    (accounts, status.usage)
}

/// EXP-694: the account + usage ROWS of one agent's grouped defaults stack —
/// the same two rows in this dialog and in Settings → Agents, so the two
/// panes cannot drift.
///
/// Row 1 is the account line ([`account_line`] — the address alone) with the
/// Login / Switch-account pill when this client may start one; row 2 is the
/// usage windows while the numbers are FRESH — stale limits beside a live
/// machine read as current ones, so they degrade to an "as of …" line
/// instead. Both are flat: the group around them draws the surface.
#[allow(clippy::too_many_arguments)] // two call sites, one row set
pub(crate) fn agent_account_rows<V: Render>(
    agent: CodingAgent,
    account: Option<&coding::agent_accounts::AgentAccount>,
    usage: Option<&coding::agent_usage::AgentUsage>,
    usage_at: Option<&str>,
    affordance: Option<LoginAffordance>,
    pending: bool,
    on_login: impl Fn(&mut V, bool, &mut gpui::Context<V>) + 'static,
    cx: &mut gpui::Context<V>,
) -> Vec<Div> {
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let now = chrono::Utc::now().timestamp();

    let mut account_row = surface::glass_row_shell().child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_sm()
            .text_color(foreground)
            .child(SharedString::from(account_line(account))),
    );
    if let Some(affordance) = affordance {
        // EXP-698: ONE pill, not a ghost button inside a pill wrapper.
        let icon = if affordance.switch {
            registry::UI_SWAP
        } else {
            registry::UI_SIGN_IN
        };
        account_row = account_row.child(
            surface::glass_pill(
                SharedString::from(format!("agent-login-{}", agent.id())),
                surface::PillSize::Sm,
                // A pending login has nothing to click: READONLY is what
                // takes the pointer cursor and the hover lift away, so the
                // dimmed pill does not keep advertising a target.
                if pending {
                    surface::PillMode::Readonly
                } else {
                    surface::PillMode::Action
                },
                cx,
            )
            .when(pending, |pill| pill.opacity(0.5))
            .child(Icon::new(icon).with_size(px(surface::PillSize::Sm.glyph())))
            .child(SharedString::from(affordance.label))
            .on_click(cx.listener(move |view: &mut V, _, _, cx| {
                if !pending {
                    on_login(view, affordance.switch, cx);
                }
            })),
        );
    }
    let mut rows = vec![account_row];

    let fresh = usage.filter(|usage| {
        !usage.windows.is_empty() && crate::usage_bar::is_fresh(&usage.fetched_at, now)
    });
    if let Some(usage) = fresh {
        // EXP-694: inside the agent's own group the windows are plain rows —
        // the group already draws the surface.
        let cards = crate::usage_bar::render_usage_cards(agent, usage, now, true, cx);
        rows.push(surface::glass_row_shell().child(div().flex_1().min_w_0().child(cards)));
    } else if usage.is_some() || account.is_some() {
        // Say when the numbers were taken instead of pretending they are
        // current.
        let stamp = account
            .map(|account| account.checked_at.clone())
            .or_else(|| usage_at.map(str::to_string))
            .unwrap_or_default();
        let as_of = crate::usage_bar::as_of_label(&stamp, now);
        if !as_of.is_empty() {
            rows.push(
                surface::glass_row_shell().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(as_of)),
                ),
            );
        }
    }
    rows
}

/// EXP-484: tolerant parse of a `{ agent: T }` jsonb column. Entries that do
/// not parse are DROPPED — a client must never brick on a newer (or a
/// corrupt) device's payload.
pub(crate) fn parse_agent_map<T: serde::de::DeserializeOwned>(
    value: Option<&serde_json::Value>,
) -> std::collections::BTreeMap<String, T> {
    let mut out = std::collections::BTreeMap::new();
    let Some(object) = value.and_then(|value| value.as_object()) else {
        return out;
    };
    for (agent, entry) in object {
        if let Ok(parsed) = serde_json::from_value::<T>(entry.clone()) {
            out.insert(agent.clone(), parsed);
        }
    }
    out
}

pub struct DeviceSettingsView {
    device_row_id: String,
    /// The steer device id (the tRPC target) — snapshotted at seed.
    device_id: String,
    /// This install's own device row (defaults edit locally through the hub).
    own: bool,
    name_input: Entity<InputState>,
    share_select: ChoiceSelect,
    /// Team (id, name) pairs behind the share select's rows, sentinel-first.
    share_teams: Vec<(String, String)>,
    // -- per-agent defaults drafts (the AgentsPane control set, minus paths) --
    agent_select: ChoiceSelect,
    model_select: ChoiceSelect,
    effort_select: ChoiceSelect,
    codex_model_select: ChoiceSelect,
    codex_effort_select: ChoiceSelect,
    pi_model_select: ChoiceSelect,
    pi_thinking_select: ChoiceSelect,
    claude_ultracode: bool,
    claude_plan_mode: bool,
    pi_plan_mode: bool,
    agent_tab: CodingAgent,
    editor_agents: Vec<CodingAgent>,
    /// The current baseline as a Settings value (drafts overlay it): the
    /// clamped launch_defaults column, kept LIVE by [`Self::resync`]. EXP-694
    /// autosave keys off drafted != seeded — a programmatic control rewrite
    /// (a resync, a rebuilt select) can therefore never echo back as a write.
    seeded: coding::Settings,
    /// The row label/share value at the last (re)seed — the "has the user
    /// diverged?" reference for the two non-defaults inputs.
    seeded_label: String,
    seeded_share: String,
    /// EXP-694: the label/share values the autosave last SENT — the dedupe
    /// that keeps a debounce landing next to a blur (or a resync echo) from
    /// writing the same value twice.
    last_saved_label: String,
    last_saved_share: String,
    /// The pending debounced rename (dropping it cancels — the shell's
    /// `queue_save_*` idiom).
    name_save: Option<Task<()>>,
    // -- section state --
    busy_section: Option<&'static str>,
    /// EXP-694: the dedupe baseline the in-flight section advanced past, under
    /// the section it belongs to — restored when THAT section's write fails,
    /// so the same value stays retryable.
    rollback: Option<(&'static str, SectionRollback)>,
    /// EXP-694: sections whose autosave arrived while another write was in
    /// flight ([`Self::run_section`] runs one at a time) — replayed when it
    /// lands, so no change is ever silently dropped.
    queued: Vec<&'static str>,
    /// EXP-696 (OWN device only): the `defaults_wire` value the row write
    /// still owes the server after the local save. Held here rather than
    /// re-derived on replay because the own-device baseline is adopted
    /// immediately — a replay reading the controls would short-circuit as
    /// "nothing drafted" and drop the write.
    pending_push: Option<serde_json::Value>,
    section_errors: HashMap<String, SharedString>,
    tracked: Vec<TrackedCommand>,
    polling: bool,
    /// EXP-484: the sign-in links finished logins handed back (keyed by
    /// agent id).
    login_notes: HashMap<String, LoginNote>,
    _subscriptions: Vec<Subscription>,
}

impl DeviceSettingsView {
    fn new(device_row_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let row = collections
            .devices
            .read(cx)
            .get(&device_row_id)
            .cloned()
            .unwrap_or_else(|| domain::rows::DeviceRow {
                id: device_row_id.clone(),
                user_id: None,
                device_id: None,
                label: None,
                kind: None,
                platform: None,
                version: None,
                agents: None,
                caps: None,
                unauthed_agents: None,
                launch_defaults: None,
                launch_defaults_updated_at: None,
                agent_accounts: None,
                agent_usage: None,
                agent_usage_at: None,
                active_sessions: None,
                last_seen_at: None,
                shared_team_id: None,
                is_default: None,
                update_requested_at: None,
                created_at: None,
                updated_at: None,
            });
        let device_id = row.device_id.clone().unwrap_or_default();
        let own = device_id
            == steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);

        let (seeded, editor_agents) = Self::baseline_for(&row, own, cx);

        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Machine name");
            state.set_value(row.label.clone().unwrap_or_default(), window, cx);
            state
        });

        // Sharing rows: "Not shared" + the caller's teams.
        let share_teams: Vec<(String, String)> = collections
            .teams_sorted(cx)
            .into_iter()
            .map(|team| (team.id, team.name))
            .collect();
        let share_choices: Vec<(String, String)> =
            std::iter::once(("Not shared".to_string(), NOT_SHARED.to_string()))
                .chain(
                    share_teams
                        .iter()
                        .map(|(id, name)| (name.clone(), id.clone())),
                )
                .collect();
        let share_refs: Vec<(&str, &str)> = share_choices
            .iter()
            .map(|(label, value)| (label.as_str(), value.as_str()))
            .collect();
        let share_select = choice_select(
            &share_refs,
            row.shared_team_id.as_deref().unwrap_or(NOT_SHARED),
            window,
            cx,
        );

        let agent_select = choice_select(&AGENT_CHOICES, seeded.default_agent.id(), window, cx);
        let model_select = choice_select(
            model_choices_for(CodingAgent::Claude),
            &seeded.claude_model,
            window,
            cx,
        );
        let effort_select = choice_select(
            effort_choices_for(CodingAgent::Claude),
            &seeded.claude_effort,
            window,
            cx,
        );
        let codex_model_select = choice_select(
            model_choices_for(CodingAgent::Codex),
            &seeded.codex_model,
            window,
            cx,
        );
        let codex_effort_select = choice_select(
            effort_choices_for(CodingAgent::Codex),
            &seeded.codex_effort,
            window,
            cx,
        );
        let pi_model_select = choice_select(
            model_choices_for(CodingAgent::Pi),
            &seeded.pi_model,
            window,
            cx,
        );
        let pi_thinking_select = choice_select(
            effort_choices_for(CodingAgent::Pi),
            &seeded.pi_thinking,
            window,
            cx,
        );

        let mut subscriptions = vec![
            // EXP-490: a devices delta re-renders AND mirrors the new
            // baseline into the controls while the dialog is open.
            cx.observe_in(&collections.devices, window, |this: &mut Self, _, window, cx| {
                this.resync(window, cx);
                cx.notify();
            }),
            cx.observe(&collections.device_worktrees, |_, _, cx| cx.notify()),
        ];
        if own {
            // The own device's baseline is the hub settings (fresher than the
            // row) — and device_sync now converges those in ~a tick, so this
            // is the live-update feed for the own-device dialog.
            let hub = CodingHub::global(cx);
            subscriptions.push(cx.observe_in(
                &hub,
                window,
                |this: &mut Self, _, window, cx| {
                    this.resync(window, cx);
                    cx.notify();
                },
            ));
        }
        for select in [
            &agent_select,
            &model_select,
            &effort_select,
            &codex_model_select,
            &codex_effort_select,
            &pi_model_select,
            &pi_thinking_select,
        ] {
            // EXP-694 autosave: a picked value IS the save (the guard in
            // `save_defaults` swallows the programmatic rewrites).
            subscriptions.push(cx.observe(select, |this: &mut Self, _, cx| {
                this.save_defaults(cx);
                cx.notify();
            }));
        }
        subscriptions.push(cx.observe(&share_select, |this: &mut Self, _, cx| {
            this.save_sharing(cx);
            cx.notify();
        }));
        // EXP-694: the name saves debounced while typing and immediately on
        // blur / Enter — no Save button.
        subscriptions.push(cx.subscribe(&name_input, |this: &mut Self, _, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => this.queue_name_save(cx),
                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                    this.name_save = None;
                    this.save_name(cx);
                }
                InputEvent::Focus => {}
            }
        }));
        // EXP-694: the debounce's other half. The dialog is a native window —
        // Escape, the titlebar ✕ and the opener-died sweep all just
        // `remove_window`, and none of them fires a blur — so a rename typed
        // inside the 800ms would be dropped on the floor. Flush it as the view
        // is released (the web dialog's unmount flush / iOS's `.onDisappear`).
        cx.on_release(|this, cx| this.flush_pending_name(cx)).detach();

        Self {
            device_row_id,
            device_id,
            own,
            name_input,
            share_select,
            share_teams,
            agent_select,
            model_select,
            effort_select,
            codex_model_select,
            codex_effort_select,
            pi_model_select,
            pi_thinking_select,
            claude_ultracode: seeded.claude_ultracode,
            claude_plan_mode: seeded.claude_plan_mode,
            pi_plan_mode: seeded.pi_plan_mode,
            agent_tab: seeded.default_agent,
            editor_agents,
            seeded_label: row.label.clone().unwrap_or_default(),
            seeded_share: row
                .shared_team_id
                .clone()
                .unwrap_or_else(|| NOT_SHARED.to_string()),
            last_saved_label: row.label.clone().unwrap_or_default(),
            last_saved_share: row
                .shared_team_id
                .clone()
                .unwrap_or_else(|| NOT_SHARED.to_string()),
            name_save: None,
            seeded,
            busy_section: None,
            rollback: None,
            queued: Vec::new(),
            pending_push: None,
            section_errors: HashMap::new(),
            tracked: Vec::new(),
            polling: false,
            login_notes: HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    // -- data ------------------------------------------------------------------

    fn row(&self, cx: &App) -> Option<domain::rows::DeviceRow> {
        Store::global(cx)
            .collections()
            .devices
            .read(cx)
            .get(&self.device_row_id)
            .cloned()
    }

    fn worktrees(&self, cx: &App) -> Vec<domain::rows::DeviceWorktreeRow> {
        let mut rows: Vec<domain::rows::DeviceWorktreeRow> = Store::global(cx)
            .collections()
            .device_worktrees
            .read(cx)
            .iter()
            .filter(|row| row.device_row_id.as_deref() == Some(self.device_row_id.as_str()))
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            (a.repo_full_name.as_deref(), a.branch.as_deref())
                .cmp(&(b.repo_full_name.as_deref(), b.branch.as_deref()))
        });
        rows
    }

    fn online(&self, cx: &App) -> bool {
        self.row(cx)
            .map(|row| {
                row_is_online(
                    row.last_seen_at.as_deref(),
                    chrono::Utc::now().timestamp_millis(),
                )
            })
            .unwrap_or(false)
    }

    /// The dialog's defaults baseline for a row: the server-authoritative
    /// launch_defaults clamped onto a default Settings (remote_admin's apply,
    /// the same clamp the device itself runs). Own devices read the LIVE hub
    /// settings instead: the file is right here and fresher than the row.
    fn baseline_for(
        row: &domain::rows::DeviceRow,
        own: bool,
        cx: &mut App,
    ) -> (coding::Settings, Vec<CodingAgent>) {
        let mut seeded = if own {
            CodingHub::global(cx).read(cx).settings.clone()
        } else {
            let mut seeded = coding::Settings::default();
            if let Some(value) = row.launch_defaults.as_ref() {
                if let Ok(patch) =
                    serde_json::from_value::<coding::DefaultsPatch>(value.clone())
                {
                    coding::apply_defaults_patch(&mut seeded, &patch);
                }
            }
            seeded
        };
        let configured: Vec<String> = row
            .launch_defaults
            .as_ref()
            .and_then(|value| value.get("agents"))
            .and_then(|agents| agents.as_object())
            .map(|agents| agents.keys().cloned().collect())
            .unwrap_or_default();
        let editor_agents =
            editor_agents(&row.agent_ids(), &row.unauthed_agent_ids(), &configured);
        if !editor_agents.contains(&seeded.default_agent) {
            seeded.default_agent = editor_agents[0];
        }
        (seeded, editor_agents)
    }

    /// EXP-490: mirror the live baseline into the open dialog — the name
    /// input, the share select and (EXP-696) the defaults controls all
    /// re-seed only while the user has NOT diverged from the previous
    /// baseline, so a background delta can never stomp a draft.
    ///
    /// EXP-696: the defaults half used to rewrite UNCONDITIONALLY. On the
    /// OWN device that fires on every `CodingHub` notify — a doctor re-run,
    /// an agent-usage probe, a settings save from another pane — and each
    /// one replayed the old baseline over the user's in-flight pick, so an
    /// edit to the machine you are sitting at appeared not to stick. The
    /// divergence guard here is the same one the two sections above have
    /// always had.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(row) = self.row(cx) else {
            return; // row deleted — nothing to mirror
        };

        let label = row.label.clone().unwrap_or_default();
        if label != self.seeded_label {
            if self.name_input.read(cx).value().trim() == self.seeded_label.trim() {
                self.name_input
                    .update(cx, |input, cx| input.set_value(label.clone(), window, cx));
                self.last_saved_label = label.clone();
            }
            self.seeded_label = label;
        }

        let share = row
            .shared_team_id
            .clone()
            .unwrap_or_else(|| NOT_SHARED.to_string());
        if share != self.seeded_share {
            if selected(&self.share_select, cx) == self.seeded_share {
                self.share_select.update(cx, |select, cx| {
                    select.set_selected_value(&SharedString::from(share.clone()), window, cx)
                });
                self.last_saved_share = share.clone();
            }
            self.seeded_share = share;
        }

        let (baseline, editor_agents) = Self::baseline_for(&row, self.own, cx);
        if baseline == self.seeded && editor_agents == self.editor_agents {
            return;
        }
        // The tab strip follows the machine's advertisement either way — it
        // is presentation, not a draft.
        self.editor_agents = editor_agents;
        // EXP-696: the user has an unsaved (or failed-and-retryable) pick on
        // screen that the incoming baseline does not already match — leave
        // it, exactly as the name input and the share select do.
        // `self.seeded` is deliberately NOT advanced: the standing draft
        // stays different from the baseline, so its next commit still goes
        // out. (A draft that HAPPENS to equal the new baseline falls through
        // and simply adopts it.)
        let drafted = self.drafted(cx);
        if drafted != self.seeded && drafted != baseline {
            let status = self.agent_status(cx);
            if !self.tab_agents(&status).contains(&self.agent_tab) {
                self.agent_tab = drafted.default_agent;
            }
            cx.notify();
            return;
        }
        self.agent_select.update(cx, |select, cx| {
            select.set_selected_value(
                &SharedString::from(baseline.default_agent.id()),
                window,
                cx,
            )
        });
        for (select, value) in [
            (&self.model_select, baseline.claude_model.clone()),
            (&self.effort_select, baseline.claude_effort.clone()),
            (&self.codex_model_select, baseline.codex_model.clone()),
            (&self.codex_effort_select, baseline.codex_effort.clone()),
            (&self.pi_model_select, baseline.pi_model.clone()),
            (&self.pi_thinking_select, baseline.pi_thinking.clone()),
        ] {
            select.update(cx, |select, cx| {
                select.set_selected_value(&SharedString::from(value), window, cx)
            });
        }
        self.claude_ultracode = baseline.claude_ultracode;
        self.claude_plan_mode = baseline.claude_plan_mode;
        self.pi_plan_mode = baseline.pi_plan_mode;
        let status = self.agent_status(cx);
        if !self.tab_agents(&status).contains(&self.agent_tab) {
            self.agent_tab = baseline.default_agent;
        }
        self.seeded = baseline;
        cx.notify();
    }

    /// The drafted launch defaults: the seed baseline with the control
    /// values overlaid (only launch-default fields matter downstream).
    fn drafted(&self, cx: &App) -> coding::Settings {
        let mut drafted = self.seeded.clone();
        drafted.default_agent = CodingAgent::parse(&selected(&self.agent_select, cx))
            .unwrap_or(drafted.default_agent);
        drafted.claude_model = selected(&self.model_select, cx);
        drafted.claude_effort = selected(&self.effort_select, cx);
        drafted.codex_model = selected(&self.codex_model_select, cx);
        drafted.codex_effort = selected(&self.codex_effort_select, cx);
        drafted.pi_model = selected(&self.pi_model_select, cx);
        drafted.pi_thinking = selected(&self.pi_thinking_select, cx);
        drafted.claude_ultracode = self.claude_ultracode;
        drafted.claude_plan_mode = self.claude_plan_mode;
        drafted.pi_plan_mode = self.pi_plan_mode;
        drafted
    }

    fn set_error(&mut self, key: impl Into<String>, message: Option<SharedString>) {
        let key = key.into();
        match message {
            Some(message) => {
                self.section_errors.insert(key, message);
            }
            None => {
                self.section_errors.remove(&key);
            }
        }
    }

    /// Put back the dedupe baseline this section advanced optimistically — a
    /// write that never went out (or came back failed) must leave the value
    /// retryable (the web `runSection`'s catch twin). A slot belonging to
    /// another section is left alone: its own write is still in flight.
    fn roll_back(&mut self, section: &'static str) {
        if self.rollback.as_ref().map(|(owner, _)| *owner) != Some(section) {
            return;
        }
        match self.rollback.take() {
            Some((_, SectionRollback::Label(label))) => self.last_saved_label = label,
            Some((_, SectionRollback::Share(share))) => self.last_saved_share = share,
            Some((_, SectionRollback::Defaults(seeded))) => self.seeded = *seeded,
            None => {}
        }
    }

    /// Run one section's mutation on the background executor; inline error
    /// on failure (per-section commit — the web `runSection` twin).
    fn run_section(
        &mut self,
        section: &'static str,
        op: impl FnOnce(&api::TrpcClient) -> Result<(), api::ApiError> + Send + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.busy_section.is_some() {
            self.roll_back(section);
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.roll_back(section);
            return;
        };
        self.busy_section = Some(section);
        self.set_error(section, None);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { op(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy_section = None;
                match result {
                    // Landed — the optimistic baseline IS the truth now.
                    Ok(()) => {
                        if this.rollback.as_ref().map(|(owner, _)| *owner) == Some(section) {
                            this.rollback = None;
                        }
                    }
                    Err(err) => {
                        this.set_error(section, Some(err.user_message().into()));
                        this.roll_back(section);
                    }
                }
                this.drain_queued(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-694: park a section whose autosave arrived mid-write (once — the
    /// replay reads the CONTROLS, so the latest value goes out either way).
    fn queue_section(&mut self, section: &'static str) {
        if !self.queued.contains(&section) {
            self.queued.push(section);
        }
    }

    /// Replay the oldest parked section now that the executor is free; its
    /// own completion drains the next one.
    fn drain_queued(&mut self, cx: &mut gpui::Context<Self>) {
        if self.queued.is_empty() {
            return;
        }
        match self.queued.remove(0) {
            "name" => self.save_name(cx),
            "sharing" => self.save_sharing(cx),
            "defaults" => self.save_defaults(cx),
            _ => {}
        }
    }

    // -- section commits -------------------------------------------------------

    /// EXP-694: hold the rename until the typing settles ([`NAME_SAVE_DEBOUNCE`]);
    /// a newer keystroke replaces (and so cancels) the pending task.
    fn queue_name_save(&mut self, cx: &mut gpui::Context<Self>) {
        self.name_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NAME_SAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.save_name(cx));
        }));
    }

    /// EXP-694: pay out what the debounce still owes as the dialog goes away.
    /// TAKING the task first is what keeps this single-shot and race-free: a
    /// blur/Enter commit already cleared it, a landed debounce already
    /// advanced `last_saved_label`, and either way nothing is written twice.
    /// There is no view left to report to, so the write goes out bare.
    fn flush_pending_name(&mut self, cx: &mut App) {
        if self.name_save.take().is_none() {
            return;
        }
        let label = self.name_input.read(cx).value().trim().to_string();
        if label.is_empty() || label == self.last_saved_label {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let device_id = self.device_id.clone();
        cx.background_executor()
            .spawn(async move {
                let _ = api::devices::rename(&trpc, &device_id, &label);
            })
            .detach();
    }

    fn save_name(&mut self, cx: &mut gpui::Context<Self>) {
        let label = self.name_input.read(cx).value().trim().to_string();
        // A blank name is not a rename, and neither is the value already sent
        // (a blur landing right behind the debounce).
        if label.is_empty() || label == self.last_saved_label {
            return;
        }
        if self.busy_section.is_some() {
            self.queue_section("name");
            return;
        }
        // Advance the dedupe now (a pick made mid-flight still writes
        // last-wins) but keep what it replaced: a failed write puts it back.
        let previous = std::mem::replace(&mut self.last_saved_label, label.clone());
        self.rollback = Some(("name", SectionRollback::Label(previous)));
        let device_id = self.device_id.clone();
        self.run_section(
            "name",
            move |trpc| api::devices::rename(trpc, &device_id, &label),
            cx,
        );
    }

    fn save_sharing(&mut self, cx: &mut gpui::Context<Self>) {
        let picked = selected(&self.share_select, cx);
        if picked == self.last_saved_share {
            return; // a resync echo, not a pick
        }
        if self.busy_section.is_some() {
            self.queue_section("sharing");
            return;
        }
        let previous = std::mem::replace(&mut self.last_saved_share, picked.clone());
        self.rollback = Some(("sharing", SectionRollback::Share(previous)));
        let team_id = (picked != NOT_SHARED).then_some(picked);
        let device_id = self.device_id.clone();
        self.run_section(
            "sharing",
            move |trpc| api::devices::set_shared(trpc, &device_id, team_id.as_deref()),
            cx,
        );
    }

    /// EXP-622: flag/unflag this machine as the caller's default — the row
    /// every device picker prefills. Written straight through (a single
    /// toggle, no draft); the server clears the flag on the caller's other
    /// machines and the switch re-renders off the row's Electric echo.
    fn save_default(&mut self, is_default: bool, cx: &mut gpui::Context<Self>) {
        let device_id = self.device_id.clone();
        self.run_section(
            "default",
            move |trpc| api::devices::set_default(trpc, &device_id, is_default),
            cx,
        );
    }

    /// EXP-694: the defaults AUTOSAVE — every select and switch writes
    /// straight through, the way the default-device toggle always has. The
    /// baseline guard is what makes that safe: a rewrite the dialog itself
    /// performed (a resync, a rebuilt select) drafts back to the baseline and
    /// writes nothing.
    fn save_defaults(&mut self, cx: &mut gpui::Context<Self>) {
        let drafted = self.drafted(cx);
        // A failed write stays retryable: the own-device path's hub already
        // holds the value in memory (so the baseline reads clean), which
        // makes the standing error the thing that re-arms the next commit.
        if drafted == self.seeded && !self.section_errors.contains_key("defaults") {
            // EXP-696: nothing new drafted, but the own-device row write may
            // still be parked behind another section — pay it out.
            self.flush_defaults_push(cx);
            return;
        }
        if self.own {
            // Adopt the draft as the baseline right away: the hub observer
            // confirms it a tick later.
            self.seeded = drafted.clone();
            // Own device: the file is right here — save through the hub
            // (which re-runs the doctor, re-advertises, and pushes the
            // server copy via device_sync).
            let hub = CodingHub::global(cx);
            let mut settings = hub.read(cx).settings.clone();
            settings.default_agent = drafted.default_agent;
            settings.claude_model = drafted.claude_model.clone();
            settings.claude_effort = drafted.claude_effort.clone();
            settings.codex_model = drafted.codex_model.clone();
            settings.codex_effort = drafted.codex_effort.clone();
            settings.pi_model = drafted.pi_model.clone();
            settings.pi_thinking = drafted.pi_thinking.clone();
            settings.claude_ultracode = drafted.claude_ultracode;
            settings.claude_plan_mode = drafted.claude_plan_mode;
            settings.pi_plan_mode = drafted.pi_plan_mode;
            self.set_error(
                "defaults",
                CodingHub::save_settings(&hub, settings, cx)
                    .err()
                    .map(SharedString::from),
            );
            // EXP-696: the devices ROW is authoritative (settings.json is the
            // copy that converges to it), and the local save alone only
            // queues a CAS push through the heartbeat — one whose
            // `expectedUpdatedAt` a concurrent web/mobile edit turns into a
            // conflict the device resolves by ADOPTING the server copy, i.e.
            // by silently reverting what was just typed here. This is a UI
            // edit like the remote branch below, so it takes the same
            // unconditional last-write-wins path. It cannot fight
            // `device_sync::apply_server_defaults`: it writes exactly what
            // settings.json now holds, so a later apply is a value-identical
            // no-op that only restamps the marker.
            self.pending_push = Some(
                serde_json::to_value(coding::defaults_wire(&drafted))
                    .expect("defaults serialize cannot fail"),
            );
            self.flush_defaults_push(cx);
            cx.notify();
            return;
        }
        // Remote device: one write at a time — a pick made mid-flight parks
        // and replays off the CONTROLS, so the last one always lands.
        if self.busy_section.is_some() {
            self.queue_section("defaults");
            return;
        }
        let previous = std::mem::replace(&mut self.seeded, drafted.clone());
        self.rollback = Some(("defaults", SectionRollback::Defaults(Box::new(previous))));
        // Remote device: unconditional owner edit of the server copy — the
        // machine converges on its next heartbeat/nudge.
        let wire = serde_json::to_value(coding::defaults_wire(&drafted))
            .expect("defaults serialize cannot fail");
        let device_id = self.device_id.clone();
        self.run_section(
            "defaults",
            move |trpc| {
                api::devices::set_launch_defaults(
                    trpc,
                    &device_id,
                    &wire,
                    api::devices::ExpectedStamp::Unconditional,
                )
                .map(|_| ())
            },
            cx,
        );
    }

    /// EXP-696: send the OWN device's launch defaults up to its (authoritative)
    /// row. Serialized through [`Self::run_section`] like every other write —
    /// a value drafted mid-flight parks in `pending_push` and replays when the
    /// executor frees up, so two quick picks can never land out of order.
    fn flush_defaults_push(&mut self, cx: &mut gpui::Context<Self>) {
        if self.pending_push.is_none() {
            return;
        }
        if self.busy_section.is_some() {
            self.queue_section("defaults");
            return;
        }
        let Some(wire) = self.pending_push.take() else {
            return;
        };
        let device_id = self.device_id.clone();
        self.run_section(
            "defaults",
            move |trpc| {
                api::devices::set_launch_defaults(
                    trpc,
                    &device_id,
                    &wire,
                    api::devices::ExpectedStamp::Unconditional,
                )
                .map(|_| ())
            },
            cx,
        );
    }

    // -- worktree commands -----------------------------------------------------

    fn queue_command(
        &mut self,
        key: String,
        kind: &'static str,
        repo: Option<String>,
        branch: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let device_id = self.device_id.clone();
        self.set_error(key.clone(), None);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_command(
                        &trpc,
                        &device_id,
                        kind,
                        repo.as_deref(),
                        branch.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(created) => {
                        this.tracked.push(TrackedCommand {
                            id: created.id,
                            key,
                        });
                        this.ensure_polling(cx);
                    }
                    Err(err) => this.set_error(key, Some(err.user_message().into())),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-484: queue a REMOTE sign-in on this machine. The device opens the
    /// agent's own login command and completes the row the moment the
    /// sign-in URL is up, which the poll below renders as a link.
    fn queue_agent_login(&mut self, agent: CodingAgent, switch: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = login_key(agent);
        let device_id = self.device_id.clone();
        self.set_error(key.clone(), None);
        self.login_notes.remove(agent.id());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_login_command(&trpc, &device_id, agent.id(), switch)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(created) => {
                        this.tracked.push(TrackedCommand {
                            id: created.id,
                            key,
                        });
                        this.ensure_polling(cx);
                    }
                    Err(err) => this.set_error(key, Some(err.user_message().into())),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Poll queued commands until terminal — the durable outcome (a worktree
    /// vanishing) additionally streams in via sync when the machine
    /// re-reports. Retires itself when nothing is tracked; dialog close
    /// drops the entity and ends the loop.
    fn ensure_polling(&mut self, cx: &mut gpui::Context<Self>) {
        if self.polling {
            return;
        }
        self.polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(delay) = this.update(cx, |this, cx| {
                    if this.online(cx) {
                        COMMAND_POLL_ONLINE
                    } else {
                        COMMAND_POLL_OFFLINE
                    }
                }) else {
                    return;
                };
                cx.background_executor().timer(delay).await;
                let Ok(Some((trpc, ids))) = this.update(cx, |this, cx| {
                    if this.tracked.is_empty() {
                        this.polling = false;
                        return None;
                    }
                    let ids: Vec<(String, String)> = this
                        .tracked
                        .iter()
                        .map(|command| (command.id.clone(), command.key.clone()))
                        .collect();
                    queries::trpc_client(cx).map(|trpc| (trpc, ids))
                }) else {
                    return;
                };
                let results = cx
                    .background_executor()
                    .spawn(async move {
                        ids.into_iter()
                            .map(|(id, key)| {
                                let row = api::devices::get_command(&trpc, &id);
                                (id, key, row)
                            })
                            .collect::<Vec<_>>()
                    })
                    .await;
                let live = this.update(cx, |this, cx| {
                    for (id, key, row) in results {
                        match row {
                            Ok(row) if row.is_terminal() => {
                                this.tracked.retain(|command| command.id != id);
                                if row.status == "failed" {
                                    this.set_error(
                                        key,
                                        Some(SharedString::from(row.result.unwrap_or_else(
                                            || "The machine reported a failure.".to_string(),
                                        ))),
                                    );
                                } else if let Some(agent) = key.strip_prefix("login ") {
                                    // EXP-484: a login completes EARLY, the
                                    // moment its sign-in URL is on the
                                    // machine's grid — that link IS the
                                    // result. The signed-in flip follows on
                                    // the synced row after the re-probe.
                                    let progress = row
                                        .result
                                        .as_deref()
                                        .and_then(coding::LoginProgress::parse);
                                    match progress {
                                        Some(progress) => match progress.url {
                                            Some(url) => {
                                                this.login_notes.insert(
                                                    agent.to_string(),
                                                    LoginNote {
                                                        url,
                                                        code: progress.code,
                                                    },
                                                );
                                            }
                                            None => this.set_error(
                                                key.clone(),
                                                progress.message.map(SharedString::from),
                                            ),
                                        },
                                        None => this.set_error(
                                            key.clone(),
                                            row.result.map(SharedString::from),
                                        ),
                                    }
                                }
                            }
                            // Pending / transient error — keep polling.
                            _ => {}
                        }
                    }
                    cx.notify();
                });
                if live.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    fn command_pending(&self, key: &str) -> bool {
        self.tracked.iter().any(|command| command.key == key)
    }

    fn prompt_remove_worktree(
        &mut self,
        repo: String,
        branch: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            format!("Remove the {branch} worktree?"),
            "Removes the worktree on the machine (the branch is kept). \
             Uncommitted changes refuse remotely.",
            "Remove",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                let repo = repo.clone();
                let branch = branch.clone();
                view.update(cx, |this, cx| {
                    let key = format!("{repo} {branch}");
                    this.queue_command(key, "worktree_remove", Some(repo), Some(branch), cx);
                });
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    // -- render pieces ---------------------------------------------------------

    fn section_title(label: &'static str, cx: &App) -> gpui::Div {
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    fn error_line(&self, key: &str, cx: &App) -> Option<gpui::Div> {
        self.section_errors.get(key).map(|message| {
            div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(message.clone())
        })
    }

    /// EXP-694: one switch row of a grouped stack — the label leading, the
    /// `Switch` trailing, on the shared 16/12 row rhythm.
    fn toggle_row(
        id: &'static str,
        label: &'static str,
        checked: bool,
        on_click: impl Fn(&mut Self, &bool, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        surface::glass_toggle_row(
            label,
            None,
            Switch::new(id)
                .checked(checked)
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    on_click(this, checked, cx);
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        )
    }

    /// EXP-694: a [`ChoiceSelect`] as a grouped picker row — the label
    /// leading, the value trailing behind the select's own caret, no field
    /// chrome (the group IS the field).
    fn picker_row(
        label: &'static str,
        description: Option<SharedString>,
        select: &ChoiceSelect,
        cx: &App,
    ) -> Div {
        surface::glass_picker_row(
            label,
            description,
            surface::glass_picker_select(Select::new(select)).into_any_element(),
            cx,
        )
    }

    /// The per-agent defaults as the SHARED grouped picker
    /// ([`AgentDefaultsGroup`], the same component the Start-coding cluster
    /// and Settings → Agents render): the embedded agent tabs row, Model, the
    /// agent's effort label, its toggles, then that agent's account + usage
    /// rows — one inset-grouped stack, no capsule strip, no Save button
    /// (EXP-694: every control writes through).
    fn render_defaults_section(
        &mut self,
        online: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let status = self.agent_status(cx);
        let tab_agents = self.tab_agents(&status);
        // The picked tab can drop out of `tab_agents` between heartbeats (a
        // probe failed, a CLI went away): fall back to the first tab for the
        // PILL AND THE BODY together, never one without the other.
        let agent_tab = if tab_agents.contains(&self.agent_tab) {
            self.agent_tab
        } else {
            tab_agents.first().copied().unwrap_or(self.agent_tab)
        };
        let active_ix = tab_agents
            .iter()
            .position(|agent| *agent == agent_tab)
            .unwrap_or(0);
        let clicked = tab_agents.clone();
        let pills: Vec<AgentPill> = tab_agents
            .iter()
            .map(|agent| AgentPill {
                label: SharedString::from(agent.label()),
                icon: Some(agent_icon(*agent)),
                dimmed: false,
                note: None,
            })
            .collect();
        // EXP-688: the agent's account + usage ride INSIDE the agent's own
        // group, under its toggles.
        let account_rows = self.render_agent_account(agent_tab, online, &status, cx);
        let (model, effort) = match agent_tab {
            CodingAgent::Claude => (self.model_select.clone(), self.effort_select.clone()),
            CodingAgent::Codex => (
                self.codex_model_select.clone(),
                self.codex_effort_select.clone(),
            ),
            CodingAgent::Pi => (
                self.pi_model_select.clone(),
                self.pi_thinking_select.clone(),
            ),
        };
        let mut group = AgentDefaultsGroup::new(
            "device-defaults",
            agent_tab,
            pills,
            Some(active_ix),
            move |this: &mut Self, ix, _window, cx| {
                if let Some(agent) = clicked.get(ix).copied() {
                    this.agent_tab = agent;
                    cx.notify();
                }
            },
            model,
            effort,
        )
        .effort_disabled(agent_tab == CodingAgent::Claude && self.claude_ultracode)
        .trailing(account_rows);
        if agent_tab == CodingAgent::Claude {
            group = group
                .toggle(DefaultsToggle::new(
                    "device-claude-ultracode",
                    "Ultracode",
                    self.claude_ultracode,
                    |this: &mut Self, on, cx| {
                        this.claude_ultracode = on;
                        this.save_defaults(cx);
                    },
                ))
                .toggle(DefaultsToggle::new(
                    "device-claude-plan",
                    "Plan mode",
                    self.claude_plan_mode,
                    |this: &mut Self, on, cx| {
                        this.claude_plan_mode = on;
                        this.save_defaults(cx);
                    },
                ));
        }
        if agent_tab == CodingAgent::Pi {
            group = group.toggle(DefaultsToggle::new(
                "device-pi-plan",
                "Plan mode",
                self.pi_plan_mode,
                |this: &mut Self, on, cx| {
                    this.pi_plan_mode = on;
                    this.save_defaults(cx);
                },
            ));
        }

        // EXP-686: no section title — the "Default agent" row already names
        // what the block is.
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![Self::picker_row(
                "Default agent",
                None,
                &self.agent_select,
                cx,
            )]))
            .child(group.render(cx));
        if !online {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Applies when the device comes online."),
            );
        }
        if self.busy_section == Some("defaults") {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Saving…"),
            );
        }
        if let Some(error) = self.error_line("defaults", cx) {
            body = body.child(error);
        }
        body
    }

    /// The agent tabs the dialog offers: the defaults editor's set ∪ every
    /// agent that reported an account or usage (EXP-688 — the account block
    /// lives in the tab now, so an agent that only has something to SAY
    /// still needs one), in `CodingAgent::ALL` order.
    fn tab_agents(&self, status: &DeviceAgentStatus) -> Vec<CodingAgent> {
        let reporting = status.reporting();
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.editor_agents.contains(agent) || reporting.contains(agent))
            .collect()
    }

    /// EXP-484: what this machine last reported about its agent CLIs.
    ///
    /// The OWN device reads the LIVE hub snapshot instead of the synced row:
    /// the collector's numbers are right here and fresher, and before the
    /// first beat the doctor probe already knows who is signed in, so a
    /// machine with agents installed is never blank.
    fn agent_status(&self, cx: &mut App) -> DeviceAgentStatus {
        let row = self.row(cx);
        let (accounts, usage) = if self.own {
            own_agent_status(cx)
        } else {
            let accounts = parse_agent_map::<coding::agent_accounts::AgentAccount>(
                row.as_ref().and_then(|row| row.agent_accounts.as_ref()),
            );
            let usage = row
                .as_ref()
                .and_then(|row| row.agent_usage.as_ref())
                .and_then(|value| value.as_object().cloned())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(agent, entry)| {
                            crate::usage_bar::parse_agent_usage(entry)
                                .map(|usage| (agent.clone(), usage))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (accounts, usage)
        };
        DeviceAgentStatus {
            accounts,
            usage,
            usage_at: row.as_ref().and_then(|row| row.agent_usage_at.clone()),
            caps: row.as_ref().map(|row| row.cap_ids()).unwrap_or_default(),
        }
    }

    /// EXP-688: the per-agent Account block. It lives INSIDE that agent's
    /// defaults tab — the standalone "Agents" section is gone, because it
    /// repeated the same three agents one screen further down.
    ///
    /// EXP-694: it is no longer a block but the LAST ROWS of the agent's own
    /// group ([`agent_account_rows`], shared with Settings → Agents), plus
    /// this dialog's remote-sign-in extras: the waiting note, whatever the
    /// last sign-in handed back, and the inline error.
    fn render_agent_account(
        &mut self,
        agent: CodingAgent,
        online: bool,
        status: &DeviceAgentStatus,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<Div> {
        let muted = cx.theme().muted_foreground;
        let account = status.accounts.get(agent.id());
        let signed_in = account.map(|account| account.signed_in).unwrap_or(false);
        let affordance = login_affordance(agent, self.own, online, &status.caps, signed_in);
        let key = login_key(agent);
        let pending = self.command_pending(&key);

        let mut rows = agent_account_rows(
            agent,
            account,
            status.usage.get(agent.id()),
            status.usage_at.as_deref(),
            affordance,
            pending,
            move |this: &mut Self, switch, cx| this.start_login(agent, switch, cx),
            cx,
        );

        if pending {
            rows.push(
                surface::glass_row_shell().child(
                    div().text_xs().text_color(muted).child(if online {
                        "Waiting for the sign-in link…"
                    } else {
                        "This machine is offline — the sign-in runs when it comes online."
                    }),
                ),
            );
        }
        if let Some(note) = self.login_notes.get(agent.id()) {
            let note = self.render_login_note(agent, note, cx);
            rows.push(surface::glass_row_shell().child(div().flex_1().min_w_0().child(note)));
        }
        if let Some(error) = self.error_line(&key, cx) {
            rows.push(surface::glass_row_shell().child(error));
        }
        rows
    }

    /// The link a finished login handed back (plus Codex's device code).
    fn render_login_note(
        &self,
        agent: CodingAgent,
        note: &LoginNote,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let url = note.url.clone();
        let copy = url.clone();
        let caption = if note.code.is_some() {
            "Open the link on any device and enter the code on the machine."
        } else {
            "Open the link on any device."
        };
        let mut line = h_flex().w_full().items_center().gap_2().child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(url)),
        );
        if let Some(code) = note.code.clone() {
            line = line.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(format!("· code {code}"))),
            );
        }
        line = line.child(
            surface::glass_pill_button(
                SharedString::from(format!("device-login-copy-{}", agent.id())),
                surface::PillSize::Sm,
                cx,
            )
                .icon(registry::UI_COPY)
                .label("Copy link")
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()));
                }),
        );
        v_flex()
            .gap_0p5()
            .child(line)
            .child(div().text_xs().text_color(muted).child(caption))
    }

    /// Start a sign-in for `agent`: locally in a terminal tab on the OWN
    /// machine, remotely as an `agent_login` device command otherwise. A
    /// switch confirms first where the sign-out is destructive (codex).
    fn start_login(&mut self, agent: CodingAgent, switch: bool, cx: &mut gpui::Context<Self>) {
        if self.own {
            crate::agent_login::open_login_tab(agent, switch, cx);
            return;
        }
        if !switch {
            self.queue_agent_login(agent, false, cx);
            return;
        }
        let view = cx.entity().downgrade();
        crate::agent_login::confirm_switch_then(agent, cx, move |cx| {
            let _ = view.update(cx, |this, cx| this.queue_agent_login(agent, true, cx));
        });
    }

    fn render_worktrees_section(
        &mut self,
        online: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let worktrees = self.worktrees(cx);
        let muted = cx.theme().muted_foreground;
        let prune_pending = self.command_pending("prune");
        let header = h_flex()
            .items_center()
            .justify_between()
            .child(Self::section_title("Worktrees", cx))
            .child(
                // EXP-688: icon-only — the label was the widest thing in the
                // section header and said what the broom already says.
                // EXP-698: the one 32px glass chrome every trailing action wears;
                // `loading` swaps the glyph for the spinner.
                crate::controls::glass_icon_button(
                    "device-worktrees-prune",
                    Icon::new(registry::UI_CLEAN),
                    cx,
                )
                    .loading(prune_pending)
                    .tooltip("Prune merged worktrees")
                    .disabled(prune_pending || worktrees.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.queue_command("prune".to_string(), "worktree_prune", None, None, cx);
                    })),
            );

        let mut body = v_flex().w_full().gap_2().child(header);
        if !online && (!worktrees.is_empty() || prune_pending) {
            body = body.child(div().text_xs().text_color(muted).child(
                "This machine is offline — queued changes run when it comes online.",
            ));
        }
        if let Some(error) = self.error_line("prune", cx) {
            body = body.child(error);
        }
        if worktrees.is_empty() {
            return body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("No worktrees reported by this machine."),
            );
        }
        // EXP-694: the hairline-underlined list became ONE grouped stack —
        // the same card the defaults wear, one worktree per row.
        let mut rows: Vec<Div> = Vec::new();
        for (index, worktree) in worktrees.iter().enumerate() {
            let repo = worktree.repo_full_name.clone().unwrap_or_default();
            let branch = worktree.branch.clone().unwrap_or_default();
            let key = format!("{repo} {branch}");
            let removing = self.command_pending(&key);
            let busy = worktree.busy.unwrap_or(false);
            let dirty = match worktree.dirty.as_deref() {
                Some("tracked") => Some("uncommitted changes"),
                Some("untracked") => Some("untracked files"),
                _ => None,
            };
            let mut row = surface::glass_row_shell()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .flex_shrink_0()
                        .child(Icon::new(registry::UI_BRANCH).xsmall().text_color(muted)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_xs()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .whitespace_nowrap()
                        .child(
                            h_flex()
                                .gap_1()
                                .child(div().text_color(muted).child(SharedString::from(repo.clone())))
                                .child(SharedString::from(branch.clone())),
                        ),
                );
            if let Some(identifier) = worktree.issue_identifier.clone() {
                row = row.child(
                    div()
                        .flex_shrink_0()
                        .px_1()
                        .rounded(px(theme::tokens::radius::SM))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(identifier)),
                );
            }
            if let Some(dirty) = dirty {
                row = row.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_0p5()
                        .items_center()
                        .text_xs()
                        .text_color(theme::tokens::YELLOW.to_hsla())
                        .child(Icon::new(registry::UI_WARNING).xsmall())
                        .child(dirty),
                );
            }
            if busy {
                row = row.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme::tokens::GREEN.to_hsla())
                        .child("in use"),
                );
            }
            let remove_repo = repo.clone();
            let remove_branch = branch.clone();
            row = row.child(
                Button::new(("device-worktree-remove", index))
                    .ghost()
                    .web_icon_xs()
                    .icon(registry::UI_DELETE)
                    .disabled(busy || removing)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.prompt_remove_worktree(
                            remove_repo.clone(),
                            remove_branch.clone(),
                            window,
                            cx,
                        );
                    })),
            );
            // A failed removal reports under its own row, inside the group.
            match self.error_line(&key, cx) {
                Some(error) => rows.push(
                    v_flex()
                        .w_full()
                        .child(row)
                        .child(div().px_4().pb_3().child(error)),
                ),
                None => rows.push(row),
            }
        }
        body.child(surface::glass_group_rows(rows))
    }
}

impl Render for DeviceSettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let row = self.row(cx);
        let online = self.online(cx);
        let muted = cx.theme().muted_foreground;
        let server = row
            .as_ref()
            .map(|row| row.is_server())
            .unwrap_or(false);

        // EXP-694: the device's own fields as THREE inset-grouped cards on the
        // shared 8px group rhythm — the name typed into its row (autosaved, no
        // Save button), the default-device switch (EXP-622: a straight-through
        // write, which is now what every control here does), and the sharing
        // picker. One card per section, exactly as the Android/iOS/web dialogs
        // render them.
        let is_default = row.as_ref().and_then(|row| row.is_default).unwrap_or(false);
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![surface::glass_input_row(
                "Name",
                surface::glass_row_input(Input::new(&self.name_input)).into_any_element(),
                cx,
            )]))
            .child(surface::glass_group_rows(vec![Self::toggle_row(
                "device-default",
                "Default device",
                is_default,
                move |this, checked, cx| this.save_default(*checked, cx),
                cx,
            )]));
        if server {
            body = body.child(surface::glass_group_rows(vec![surface::glass_picker_row(
                "Shared with",
                Some(SharedString::from(
                    "Teammates of that team can start coding sessions on this machine.",
                )),
                surface::glass_picker_select(Select::new(&self.share_select))
                    .disabled(self.share_teams.is_empty())
                    .into_any_element(),
                cx,
            )]));
        }
        // The autosave says nothing while it succeeds; a write in flight or a
        // failed one reports under the group it belongs to.
        for section in ["name", "default", "sharing"] {
            if self.busy_section == Some(section) {
                body = body.child(div().text_xs().text_color(muted).child("Saving…"));
            }
            if let Some(error) = self.error_line(section, cx) {
                body = body.child(error);
            }
        }

        let defaults_section = self.render_defaults_section(online, cx);
        let worktrees_section = self.render_worktrees_section(online, cx);
        // Every group in the dialog sits on the SAME 8px rhythm (the ×4
        // parity look) — the worktrees section included.
        body.child(defaults_section).child(worktrees_section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_window_clamps_negative_ages_and_fails_closed() {
        let now_ms: i64 = 1_754_900_000_000; // arbitrary fixed instant
        let iso = |offset_secs: i64| {
            chrono::DateTime::from_timestamp(now_ms / 1_000 + offset_secs, 0)
                .unwrap()
                .to_rfc3339()
        };
        // Fresh beat = online; a stamp past the window = offline.
        assert!(row_is_online(Some(&iso(-30)), now_ms));
        assert!(!row_is_online(Some(&iso(-120)), now_ms));
        // The boundary sits at the contract window.
        let window_secs = domain::contract::DEVICE_ONLINE_WINDOW_MS / 1_000;
        assert!(row_is_online(Some(&iso(-(window_secs - 1))), now_ms));
        assert!(!row_is_online(Some(&iso(-(window_secs + 1))), now_ms));
        // Server clock ahead of ours (negative age) clamps ONLINE.
        assert!(row_is_online(Some(&iso(300)), now_ms));
        // Unparseable / absent stamps fail closed.
        assert!(!row_is_online(Some("garbage"), now_ms));
        assert!(!row_is_online(None, now_ms));
    }

    /// EXP-484/694: the line, byte-identical to the web `accountLine` — the
    /// address alone, with no `signed in as` prefix and no ` · <plan>` tail.
    #[test]
    fn account_label_variants() {
        let account = |signed_in: bool, email: Option<&str>, plan: Option<&str>| {
            coding::agent_accounts::AgentAccount {
                signed_in,
                email: email.map(str::to_string),
                plan: plan.map(str::to_string),
                checked_at: "2026-08-28T10:00:00.000Z".to_string(),
            }
        };
        // The plan is dropped even when the machine reported one.
        assert_eq!(
            account_line(Some(&account(true, Some("a@b.c"), Some("max")))),
            "a@b.c"
        );
        assert_eq!(
            account_line(Some(&account(true, Some("a@b.c"), None))),
            "a@b.c"
        );
        assert_eq!(
            account_line(Some(&account(false, None, None))),
            "Not signed in"
        );
        // pi names a provider, never an address — it keeps the bare plan.
        assert_eq!(
            account_line(Some(&account(true, None, Some("anthropic (oauth)")))),
            "anthropic (oauth)"
        );
        // A signed-in account naming neither.
        assert_eq!(account_line(Some(&account(true, None, None))), "signed in");
        // Never probed is NOT signed out.
        assert_eq!(account_line(None), "Sign-in status unknown");
    }

    /// EXP-484: who may start a sign-in. The own machine always can; a
    /// remote one needs to be online, to run a build with the cap, and not
    /// to be pi.
    #[test]
    fn login_affordance_matrix() {
        let caps = vec!["resume".to_string(), "agent-login".to_string()];
        let none: Vec<String> = Vec::new();

        // Own device: always, offline and cap-less included — the login runs
        // in a terminal tab right here.
        let own = login_affordance(CodingAgent::Claude, true, false, &none, false).unwrap();
        assert_eq!(own.label, "Login");
        assert!(!own.switch);
        // Signed in → the switch wording, and the run signs out first.
        let own = login_affordance(CodingAgent::Codex, true, false, &none, true).unwrap();
        assert_eq!(own.label, "Switch account");
        assert!(own.switch);
        // pi is fine locally.
        assert!(login_affordance(CodingAgent::Pi, true, true, &caps, false).is_some());

        // Remote: online + cap + not pi.
        assert!(login_affordance(CodingAgent::Claude, false, true, &caps, false).is_some());
        assert!(login_affordance(CodingAgent::Claude, false, false, &caps, false).is_none());
        assert!(login_affordance(CodingAgent::Claude, false, true, &none, false).is_none());
        assert!(login_affordance(CodingAgent::Pi, false, true, &caps, false).is_none());
    }

    /// EXP-484: a newer (or corrupt) device's payload drops the bad entries,
    /// never the row.
    #[test]
    fn parse_agent_map_tolerates_garbage() {
        let value = serde_json::json!({
            "claude": {
                "signedIn": true,
                "email": "dev@acme.test",
                "plan": "max",
                "checkedAt": "2026-08-28T10:00:00.000Z",
            },
            // Unknown fields ride along unread.
            "codex": { "signedIn": false, "checkedAt": "x", "future": 1 },
            // Wrong shape entirely — dropped, not fatal.
            "pi": "signed in",
        });
        let accounts =
            parse_agent_map::<coding::agent_accounts::AgentAccount>(Some(&value));
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts["claude"].email.as_deref(), Some("dev@acme.test"));
        assert!(!accounts["codex"].signed_in);
        assert!(!accounts.contains_key("pi"));
        // A missing / non-object column is simply nothing to render.
        assert!(parse_agent_map::<coding::agent_accounts::AgentAccount>(None).is_empty());
        assert!(parse_agent_map::<coding::agent_accounts::AgentAccount>(Some(
            &serde_json::json!("nope")
        ))
        .is_empty());
    }

    #[test]
    fn editor_agents_union_falls_back_to_the_full_set() {
        // Runnable ∪ signed-out ∪ configured, in ALL order, deduped.
        let agents = editor_agents(
            &["codex".to_string()],
            &["claude".to_string()],
            &["codex".to_string()],
        );
        assert_eq!(agents, vec![CodingAgent::Claude, CodingAgent::Codex]);
        // Unknown ids are ignored, an empty union edits every agent.
        let agents = editor_agents(&["cursor".to_string()], &[], &[]);
        assert_eq!(agents, CodingAgent::ALL.to_vec());
        let agents = editor_agents(&[], &[], &[]);
        assert_eq!(agents, CodingAgent::ALL.to_vec());
    }
}
