//! EXP-481: the Device settings dialog — the desktop twin of the web's
//! `device-settings-dialog.tsx`, opened from a machines row's settings gear
//! (EXP-909: the row's ONE control).
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
//! | Icon           | `devices.setIcon` (EXP-924: the identity row's picker,|
//! |                | shown optimistically until the shape echoes it)       |
//! | Default        | `devices.setDefault` (EXP-622, own devices only)     |
//! | Sharing        | `devices.setShared` (server-kind own devices only;    |
//! |                | FEED-33: one toggle per team, straight through)       |
//! | Agent defaults | `setLaunchDefaults` (UNCONDITIONAL — a UI edit is     |
//! |                | last-write-wins on both); the OWN device ALSO saves   |
//! |                | settings.json first through [`CodingHub::save_settings`] |
//! | Update         | `devices.requestUpdate` (EXP-909: server devices only,|
//! |                | FEED-36's "Update now…" confirms) — NOT an autosave   |
//! | Remove         | `devices.remove` behind a confirm (EXP-909); the      |
//! |                | dialog closes behind it                               |
//!
//! EXP-1020: ONE layout on the four clients, and ONE column. There is no
//! worktrees section here any more — a machine's worktrees are a LOCAL
//! surface, Settings → Worktrees, which is also the only place that cleans
//! them; the remote `worktree_remove` / `worktree_prune` queue went with it.
//! The agent-defaults card ends in a "Workflow settings" SUB-SHELL row (the
//! model pair a workflow started on this machine is seeded from,
//! `launch_defaults.workflow`), and "Remove device" is a plain row of the
//! same shell rather than a section of its own.
//!
//! Data comes from the SYNCED `devices` collection (never relay presence):
//! defaults stay editable while the machine is offline ("Applies when the
//! device comes online."). EXP-490: the dialog mirrors the LIVE baseline
//! while open — the drafted sections (name and, since EXP-696, the defaults
//! controls) re-seed only while the user has NOT diverged from the previous
//! baseline, so a background delta never stomps a draft; the default-device
//! and sharing switches hold no draft and render straight off the row. A
//! queued agent-CLI update is polled (`devices.getCommand`) until terminal —
//! a failure renders its device-reported message inline.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    div, px, size, App, AppContext as _, Div, Entity, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};
use sync::Store;

use coding::CodingAgent;

use crate::coding_flow::CodingHub;
use crate::controls::{glass_input, WebControl as _};
use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
    AGENT_CHOICES,
};
use crate::icons::registry;
use crate::launch_options::{AgentDefaultsGroup, AgentPill, DefaultsToggle};
use crate::native_dialog::{self, AlertSpec, DialogContent, DialogSpec};
use crate::queries;
use crate::surface;

/// EXP-862: what the dialog's form actually needs — Name, Default device,
/// Sharing and the agent defaults. It is a CAP, not a floor: a short screen
/// still gets the old 85% and the stack scrolls inside it.
const CONTENT_H: f32 = 560.;

/// Queued-command poll cadence while the dialog is open (offline machines
/// keep their commands queued server-side — poll slowly).
const COMMAND_POLL_ONLINE: Duration = Duration::from_secs(2);
const COMMAND_POLL_OFFLINE: Duration = Duration::from_secs(8);

/// EXP-694: the dialog AUTOSAVES — there is no Save button anywhere. Typing
/// settles for this long before the rename goes out (the web dialog's 800ms
/// twin); a blur commits it immediately. Everything else (the pickers, the
/// switches, the sharing row) writes straight through on change.
const NAME_SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

/// EXP-1020: the DISPLAY label of a choice value (`opus` -> `Opus`), for a
/// row that SUMMARISES picks made elsewhere — the "Workflow settings" pair.
/// A value with no entry shows itself, so an unknown alias is still legible.
pub(crate) fn choice_label(
    choices: &'static [(&'static str, &'static str)],
    value: &str,
) -> SharedString {
    choices
        .iter()
        .find(|(_, candidate)| *candidate == value)
        .map(|(label, _)| SharedString::from(*label))
        .unwrap_or_else(|| SharedString::from(value.to_string()))
}

/// EXP-1020: the workflow pair a machine would start a workflow with, for
/// `agent`. The machine stores ONE pair, in its DEFAULT agent's names
/// (`Settings::load` clamps it there, as does the server and web's
/// `workflowDefaultsFor`), so the other agent's selects show that agent's
/// contract defaults until it becomes the default.
pub(crate) fn workflow_pair_for(
    settings: &coding::Settings,
    agent: CodingAgent,
) -> (String, String) {
    if settings.default_agent == agent {
        (
            settings.workflow_model.clone(),
            settings.workflow_strong_model.clone(),
        )
    } else {
        workflow_defaults_for(agent)
    }
}

/// EXP-1020: the "Workflow settings" PAGE — the two rungs a workflow
/// launches on. ONE builder, rendered by both settings surfaces (the Device
/// settings dialog and Settings → Agents), so the two pages cannot drift.
///
/// The selects carry the DEFAULT account's agent's vocabulary; the caller
/// picks the pair before handing them over.
pub(crate) fn render_workflow_page(
    model: &ChoiceSelect,
    strong_model: &ChoiceSelect,
    cx: &App,
) -> gpui::AnyElement {
    use gpui_component::select::Select;
    v_flex()
        .w_full()
        .gap_2()
        .child(surface::glass_group_rows(vec![
            surface::glass_picker_row(
                "Model",
                None,
                surface::glass_picker_select(Select::new(model)).into_any_element(),
                cx,
            ),
            surface::glass_picker_row(
                "Strong model",
                None,
                surface::glass_picker_select(Select::new(strong_model)).into_any_element(),
                cx,
            ),
        ]))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "Leaf nodes and the subagents inside them run on the model. \
                     Contract, integration and risky nodes, and every review, run \
                     on the strong model.",
                ),
        )
        .into_any_element()
}

/// Contract `workflowLaunch`'s pair for `agent`.
pub(crate) fn workflow_defaults_for(agent: CodingAgent) -> (String, String) {
    match agent {
        CodingAgent::Claude => (
            domain::contract::WORKFLOW_LAUNCH_CLAUDE_MODEL.to_string(),
            domain::contract::WORKFLOW_LAUNCH_CLAUDE_STRONG_MODEL.to_string(),
        ),
        CodingAgent::Codex => (
            domain::contract::WORKFLOW_LAUNCH_CODEX_MODEL.to_string(),
            domain::contract::WORKFLOW_LAUNCH_CODEX_STRONG_MODEL.to_string(),
        ),
    }
}

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

/// EXP-862 — write a value into one of the dialog's hidden selects the way a
/// PICK does, autosave included.
///
/// `set_selected_value` writes the selection without notifying (only the
/// select's own interactive confirm path emits and notifies), and the EXP-694
/// autosave is an observer on that entity — so a picker that renders its own
/// trigger (the shared [`crate::coding_selects::agent_picker`]) has to ring the
/// bell itself, or the pick is silently dropped when the dialog closes.
fn write_choice(select: &ChoiceSelect, value: &str, window: &mut Window, cx: &mut App) {
    select.update(cx, |select, cx| {
        select.set_selected_value(&SharedString::from(value.to_string()), window, cx);
        cx.notify();
    });
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
    // EXP-862: the dialog is CONTENT-sized. Name, Default device, Sharing
    // and the agent defaults are a short form now that the account and usage
    // rows are gone (they live on the Devices page's Accounts section), and
    // a window taking 85% of a 1440p display to render six rows read as a
    // page, not as a dialog. It stays resizable and scrolls inside.
    //
    // EXP-1020: ONE column since the worktrees left, so the landscape width
    // the second one needed (EXP-762's 820) narrows to the form's own.
    let height = (window.viewport_size().height * 0.85).min(px(CONTENT_H));
    let spec = DialogSpec::new("Device settings", size(px(560.), height))
        .resizable(size(px(440.), px(360.)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| DeviceSettingsView::new(device_row_id, window, cx));
        DialogContent::new(view).self_scrolling()
    });
}

/// One queued command the dialog tracks until terminal.
struct TrackedCommand {
    id: String,
    /// `"login {agent}"` for an EXP-484 sign-in, or `"update {agent}"` for
    /// an agent CLI update —
    /// the inline error/result slot.
    key: String,
}

/// EXP-694: what a section's OPTIMISTIC dedupe advance has to be undone to
/// when its write never lands. The autosave advances the baseline before the
/// tRPC call resolves (so a pick made mid-flight still writes last-wins); a
/// failure has to put it back, or re-committing the SAME value dedupes itself
/// away and the write is never retried. One slot is enough: `run_section`
/// runs exactly one write at a time.
enum SectionRollback {
    Label(String),
    Defaults(Box<coding::Settings>),
    /// EXP-924: the optimistic icon pick. There is nothing to put BACK — the
    /// swatch simply falls off the pick and renders the synced row again.
    Icon,
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
    if let Some(demo) = dev_agent_status() {
        return (demo.accounts, demo.usage);
    }
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

/// DEV-ONLY `EXP_DEV_AGENT_ACCOUNT` (EXP-698) — stand DEMO agent accounts
/// and demo usage windows in place of whatever this machine actually probed.
///
/// Settings → Agents renders the LOCAL install's own account row, so the
/// shots pipeline (`packages/view-catalog`) was baking the operator's real
/// address and real rate-limit numbers into a committed screenshot. With the
/// variable set the pane never consults the hub at all: no keychain read, no
/// usage fetch, nothing personal to leak. Unset (every real build) this is a
/// no-op.
///
/// EXP-951: the same payload is what this machine REPORTS while the variable
/// is set — `devices.register` ([`crate::steer_wiring`]) and every heartbeat
/// ([`crate::device_sync`]) send it instead of the probe, so the capture
/// host's synced row (the Agents screen's "This device") carries demo logins
/// too, not the operator's.
///
/// EXP-733: the numbers are the SAME ones the stand-in desktop of the shots
/// pipeline reports for the demo machine's synced row
/// (`apps/web/scripts/screenshot-demo.ts` `DEMO_AGENT_STATUS`) — all three
/// agents, so the pane's Codex tab photographs the same machine the
/// web/mobile machine-settings shots do. Each reset carries a few seconds of
/// pad past the label it is meant to print, so the shutter (which fires some
/// seconds after launch) still reads `3h 32m` rather than the minute below.
pub(crate) fn dev_agent_status() -> Option<coding::agent_usage::AgentStatusPayload> {
    use coding::agent_accounts::{AgentAccount, AgentAccounts};
    use coding::agent_usage::{AgentStatusPayload, AgentUsage, AgentUsageMap, UsageWindow};

    let email = std::env::var("EXP_DEV_AGENT_ACCOUNT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let now = chrono::Utc::now();
    let stamp = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    const COUNTDOWN_PAD_SECONDS: i64 = 45;
    let resets_in = |seconds: i64| {
        Some(
            (now + chrono::Duration::seconds(seconds + COUNTDOWN_PAD_SECONDS))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )
    };
    let window = |key: &str, label: &str, percent: u8, resets: i64| UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        percent,
        resets_at: resets_in(resets),
    };
    // The three windows the pane groups: the 5h session, the weekly
    // all-models bucket and one per-model bucket under it.
    let claude_windows = || {
        vec![
            window("session", "5h", 73, 3 * 3_600 + 32 * 60),
            window("weekly", "Week", 24, 30 * 3_600),
            window("model:fable", "Fable", 38, 30 * 3_600),
        ]
    };

    let mut accounts = AgentAccounts::new();
    let mut usage = AgentUsageMap::new();
    let mut report = |agent: CodingAgent, account_email: Option<String>, plan: &str, windows: Vec<UsageWindow>| {
        accounts.insert(
            agent.id().to_string(),
            AgentAccount {
                signed_in: true,
                email: account_email,
                plan: Some(plan.to_string()),
                checked_at: stamp.clone(),
                ..AgentAccount::default()
            },
        );
        usage.insert(
            agent.id().to_string(),
            AgentUsage {
                fetched_at: stamp.clone(),
                stale: false,
                windows,
            },
        );
    };
    report(CodingAgent::Claude, Some(email.clone()), "max", claude_windows());
    report(
        CodingAgent::Codex,
        Some(email),
        "plus",
        vec![
            window("session", "5h", 41, 2 * 3_600 + 5 * 60),
            window("weekly", "Week", 57, 4 * 86_400 + 9 * 3_600),
        ],
    );
    Some(AgentStatusPayload { accounts, usage })
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
    /// EXP-924: the glyph picked in this dialog but not yet echoed back by the
    /// `devices` shape — the swatch's optimistic value. `None` = show the
    /// row's (resolved) icon.
    icon_pick: Option<&'static str>,
    // -- per-agent defaults drafts (the AgentsPane control set, minus paths) --
    agent_select: ChoiceSelect,
    model_select: ChoiceSelect,
    effort_select: ChoiceSelect,
    codex_model_select: ChoiceSelect,
    codex_effort_select: ChoiceSelect,
    /// EXP-981/EXP-1020: the model claude's SUBAGENTS run on — the row web
    /// had and the IDE did not.
    subagent_model_select: ChoiceSelect,
    /// EXP-1020: the "Workflow settings" page's pair — what a workflow
    /// started on this machine is seeded from (`launch_defaults.workflow`).
    /// ONE pair per agent, like the model/effort selects above: the pair
    /// belongs to the DEFAULT agent's vocabulary, and the two vocabularies
    /// do not overlap, so a select cannot simply be re-pointed.
    workflow_model_select: ChoiceSelect,
    workflow_strong_model_select: ChoiceSelect,
    codex_workflow_model_select: ChoiceSelect,
    codex_workflow_strong_model_select: ChoiceSelect,
    /// EXP-1020: which sub-shell page is open, if any.
    nav: crate::sub_shell::SubShellNav,
    claude_ultracode: bool,
    claude_plan_mode: bool,
    /// EXP-872: the machine's DEFAULT ACCOUNT — the profile id of
    /// `default_agent`'s logins it launches as. It rides beside the agent
    /// select (a pick writes both) rather than in one, because an account is
    /// per-machine data and the agent list is a closed set.
    default_account: Option<String>,
    agent_tab: CodingAgent,
    editor_agents: Vec<CodingAgent>,
    /// The current baseline as a Settings value (drafts overlay it): the
    /// clamped launch_defaults column, kept LIVE by [`Self::resync`]. EXP-694
    /// autosave keys off drafted != seeded — a programmatic control rewrite
    /// (a resync, a rebuilt select) can therefore never echo back as a write.
    seeded: coding::Settings,
    /// The row label at the last (re)seed — the "has the user diverged?"
    /// reference for the name input.
    seeded_label: String,
    /// EXP-694: the label the autosave last SENT — the dedupe that keeps a
    /// debounce landing next to a blur (or a resync echo) from writing the
    /// same value twice.
    last_saved_label: String,
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
    /// A finished command's device-reported SUCCESS message, per key — the
    /// per-agent Update rows show theirs ("Claude Code updated: …") where a
    /// worktree removal shows nothing (its row simply vanishes).
    section_notes: HashMap<String, SharedString>,
    tracked: Vec<TrackedCommand>,
    polling: bool,
    /// EXP-909: what the Update section compares this device's version
    /// against — the instance's informational `CLIENT_LATEST_VERSION_CLI`,
    /// fetched ONCE per dialog (instance config, not device state).
    latest_cli: Option<String>,
    /// The one-shot guard for [`Self::ensure_latest_loaded`].
    latest_requested: bool,
    /// A `devices.requestUpdate` in flight — the synced `update_requested_at`
    /// takes the state over the moment the write lands.
    update_busy: bool,
    /// A `devices.remove` in flight.
    remove_busy: bool,
    /// EXP-909: the remove landed — the row this dialog configures is gone,
    /// so the dialog closes on the next frame.
    removed: bool,
    /// The stack's scroll position (view state, so a re-render — every
    /// autosave, every heartbeat resync — keeps it).
    settings_scroll: ScrollHandle,
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
                icon: None,
                platform: None,
                version: None,
                agents: None,
                caps: None,
                unauthed_agents: None,
                acp_agents: None,
                launch_defaults: None,
                launch_defaults_updated_at: None,
                agent_accounts: None,
                agent_usage: None,
                agent_usage_at: None,
                active_sessions: None,
                last_seen_at: None,
                shared_team_ids: Vec::new(),
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
        let subagent_model_select = choice_select(
            &crate::coding_selects::SUBAGENT_MODEL_CHOICES,
            &seeded.claude_subagent_model,
            window,
            cx,
        );
        let (claude_workflow, claude_workflow_strong) =
            workflow_pair_for(&seeded, CodingAgent::Claude);
        let (codex_workflow, codex_workflow_strong) =
            workflow_pair_for(&seeded, CodingAgent::Codex);
        let workflow_model_select = choice_select(
            model_choices_for(CodingAgent::Claude),
            &claude_workflow,
            window,
            cx,
        );
        let workflow_strong_model_select = choice_select(
            model_choices_for(CodingAgent::Claude),
            &claude_workflow_strong,
            window,
            cx,
        );
        let codex_workflow_model_select = choice_select(
            model_choices_for(CodingAgent::Codex),
            &codex_workflow,
            window,
            cx,
        );
        let codex_workflow_strong_model_select = choice_select(
            model_choices_for(CodingAgent::Codex),
            &codex_workflow_strong,
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
            &subagent_model_select,
            &workflow_model_select,
            &workflow_strong_model_select,
            &codex_workflow_model_select,
            &codex_workflow_strong_model_select,
        ] {
            // EXP-694 autosave: a picked value IS the save (the guard in
            // `save_defaults` swallows the programmatic rewrites).
            subscriptions.push(cx.observe(select, |this: &mut Self, _, cx| {
                this.save_defaults(cx);
                cx.notify();
            }));
        }
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
            icon_pick: None,
            agent_select,
            model_select,
            effort_select,
            codex_model_select,
            codex_effort_select,
            subagent_model_select,
            workflow_model_select,
            workflow_strong_model_select,
            codex_workflow_model_select,
            codex_workflow_strong_model_select,
            nav: crate::sub_shell::SubShellNav::new(),
            claude_ultracode: seeded.claude_ultracode,
            claude_plan_mode: seeded.claude_plan_mode,
            default_account: seeded.default_account.clone(),
            agent_tab: seeded.default_agent,
            editor_agents,
            seeded_label: row.label.clone().unwrap_or_default(),
            last_saved_label: row.label.clone().unwrap_or_default(),
            name_save: None,
            seeded,
            busy_section: None,
            rollback: None,
            queued: Vec::new(),
            pending_push: None,
            section_errors: HashMap::new(),
            section_notes: HashMap::new(),
            tracked: Vec::new(),
            polling: false,
            latest_cli: None,
            latest_requested: false,
            update_busy: false,
            remove_busy: false,
            removed: false,
            settings_scroll: ScrollHandle::new(),
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
    /// input and (EXP-696) the defaults controls re-seed only while the user
    /// has NOT diverged from the previous baseline, so a background delta can
    /// never stomp a draft. (FEED-33: the sharing switches render straight
    /// off the row, like the default-device toggle, so they need no seed.)
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

        // EXP-924: the optimistic pick stands only until the row says the
        // same thing — the write landed, so the swatch renders the SYNCED
        // value from here on (a failed write drops the pick instead).
        if self.icon_pick.is_some()
            && self.icon_pick == Some(crate::icons::device_icon_name(
                row.icon.as_deref(),
                row.is_server(),
            ))
        {
            self.icon_pick = None;
        }

        let label = row.label.clone().unwrap_or_default();
        if label != self.seeded_label {
            if self.name_input.read(cx).value().trim() == self.seeded_label.trim() {
                self.name_input
                    .update(cx, |input, cx| input.set_value(label.clone(), window, cx));
                self.last_saved_label = label.clone();
            }
            self.seeded_label = label;
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
        // it, exactly as the name input does.
        // `self.seeded` is deliberately NOT advanced: the standing draft
        // stays different from the baseline, so its next commit still goes
        // out. (A draft that HAPPENS to equal the new baseline falls through
        // and simply adopts it.)
        let drafted = self.drafted(cx);
        if drafted != self.seeded && drafted != baseline {
            if !self.editor_agents.contains(&self.agent_tab) {
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
            (
                &self.subagent_model_select,
                baseline.claude_subagent_model.clone(),
            ),
            // EXP-1020: each agent's pair takes ITS OWN names — the
            // baseline carries only the default agent's, so the other
            // agent's selects fall back to its contract defaults.
            (
                &self.workflow_model_select,
                workflow_pair_for(&baseline, CodingAgent::Claude).0,
            ),
            (
                &self.workflow_strong_model_select,
                workflow_pair_for(&baseline, CodingAgent::Claude).1,
            ),
            (
                &self.codex_workflow_model_select,
                workflow_pair_for(&baseline, CodingAgent::Codex).0,
            ),
            (
                &self.codex_workflow_strong_model_select,
                workflow_pair_for(&baseline, CodingAgent::Codex).1,
            ),
        ] {
            select.update(cx, |select, cx| {
                select.set_selected_value(&SharedString::from(value), window, cx)
            });
        }
        self.claude_ultracode = baseline.claude_ultracode;
        self.claude_plan_mode = baseline.claude_plan_mode;
        self.default_account = baseline.default_account.clone();
        if !self.editor_agents.contains(&self.agent_tab) {
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
        drafted.claude_subagent_model = selected(&self.subagent_model_select, cx);
        // EXP-1020: the stored pair is the DEFAULT agent's, so read the pair
        // belonging to the agent this draft names.
        let (workflow_model, workflow_strong) = self.workflow_selects(drafted.default_agent);
        drafted.workflow_model = selected(&workflow_model, cx);
        drafted.workflow_strong_model = selected(&workflow_strong, cx);
        drafted.claude_ultracode = self.claude_ultracode;
        drafted.claude_plan_mode = self.claude_plan_mode;
        drafted.default_account = self.default_account.clone();
        drafted
    }

    /// The workflow pair's selects for `agent`.
    fn workflow_selects(&self, agent: CodingAgent) -> (ChoiceSelect, ChoiceSelect) {
        match agent {
            CodingAgent::Claude => (
                self.workflow_model_select.clone(),
                self.workflow_strong_model_select.clone(),
            ),
            CodingAgent::Codex => (
                self.codex_workflow_model_select.clone(),
                self.codex_workflow_strong_model_select.clone(),
            ),
        }
    }

    /// EXP-1020: the agent the DEFAULT ACCOUNT names — what the workflow
    /// pair belongs to. Not [`Self::agent_tab`], which is only which tab of
    /// the defaults card is open.
    fn default_agent(&self, cx: &App) -> CodingAgent {
        CodingAgent::parse(&selected(&self.agent_select, cx))
            .unwrap_or(self.seeded.default_agent)
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
            Some((_, SectionRollback::Defaults(seeded))) => self.seeded = *seeded,
            Some((_, SectionRollback::Icon)) => self.icon_pick = None,
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
            "icon" => self.commit_icon(cx),
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

    /// EXP-924: the glyph the picker shows as SELECTED — the optimistic pick
    /// while a write is in flight, otherwise the row's RESOLVED icon (the ONE
    /// resolver: a machine that never picked shows its kind default, not an
    /// empty swatch).
    fn icon_name(&self, row: Option<&domain::rows::DeviceRow>) -> &'static str {
        self.icon_pick.unwrap_or_else(|| {
            crate::icons::device_icon_name(
                row.and_then(|row| row.icon.as_deref()),
                row.is_some_and(|row| row.is_server()),
            )
        })
    }

    /// EXP-924: pick this machine's glyph — a straight-through write like the
    /// default toggle, shown OPTIMISTICALLY because the swatch has nothing
    /// else to render until the Electric echo lands.
    fn save_icon(&mut self, icon: &'static str, cx: &mut gpui::Context<Self>) {
        let row = self.row(cx);
        if icon == self.icon_name(row.as_ref()) {
            return;
        }
        self.icon_pick = Some(icon);
        self.commit_icon(cx);
    }

    /// Send the standing pick. Also the replay path ([`Self::drain_queued`]):
    /// the pick lives in `icon_pick`, so a write parked behind another
    /// section still sends the LATEST glyph when the executor frees up.
    fn commit_icon(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(icon) = self.icon_pick else {
            return;
        };
        if self.busy_section.is_some() {
            self.queue_section("icon");
            cx.notify();
            return;
        }
        // A write that never goes out (or comes back failed) drops the pick
        // and the swatch falls back to the synced row.
        self.rollback = Some(("icon", SectionRollback::Icon));
        let device_id = self.device_id.clone();
        self.run_section(
            "icon",
            move |trpc| api::devices::set_icon(trpc, &device_id, Some(icon)),
            cx,
        );
    }

    /// FEED-33: share/unshare this SERVER machine with ONE team. Written
    /// straight through like the default toggle (no draft): the switches
    /// render off the row's `shared_team_ids`, so the Electric echo is what
    /// flips them and a failed write simply leaves them where the row is.
    /// The switches are disabled while a section write is in flight, so a
    /// second toggle never lands on a busy executor.
    fn save_sharing(&mut self, team_id: String, shared: bool, cx: &mut gpui::Context<Self>) {
        let device_id = self.device_id.clone();
        self.run_section(
            "sharing",
            move |trpc| api::devices::set_shared(trpc, &device_id, &team_id, shared),
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
            // EXP-746: the overlay is field-by-field, not a struct
            // assignment — the CLI paths and the Tools pane's fields must
            // survive a save from here. EXP-1020: it is `coding`'s ONE
            // definition now, shared with the Agents pane. It used to be a
            // hand-copied list, and the fields added after it was written
            // (`claude_subagent_model`, the workflow pair) were saved
            // everywhere EXCEPT on this machine's own row.
            coding::overlay_launch_defaults(&mut settings, &drafted);
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

    // -- device commands -------------------------------------------------------

    /// Queue an `agent_update` for `agent` on this machine — the agent CLI's
    /// own self-updater, run remotely (the web dialog's twin). Tracked under
    /// `update {agent}` so the outcome message lands under that row.
    fn queue_agent_update(&mut self, agent: CodingAgent, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = format!("update {}", agent.id());
        let device_id = self.device_id.clone();
        self.set_error(key.clone(), None);
        self.section_notes.remove(&key);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_update_command(&trpc, &device_id, agent.id())
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
                                            || "The device reported a failure.".to_string(),
                                        ))),
                                    );
                                } else if key.starts_with("update ") {
                                    // The version move is the outcome — the
                                    // row's version line follows on the next
                                    // heartbeat; the message says it now.
                                    if let Some(result) = row.result.filter(|r| !r.is_empty()) {
                                        this.section_notes.insert(key, SharedString::from(result));
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

    // -- EXP-909: Update + Remove ---------------------------------------------
    // The two controls the device ROW used to carry (its ⋯ menu): a row has
    // ONE control now, the gear that opens this dialog, so the predicates and
    // the confirm copy moved here unchanged.

    /// `devices.latestVersions` — the informational `CLIENT_LATEST_VERSION_*`
    /// pair behind the amber nudge, fetched once per dialog. A failure just
    /// means no nudge (the Update button itself gates on the device's own
    /// report), which beats a render-driven retry storm.
    fn ensure_latest_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.latest_requested {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return; // no client yet — a later render tries again
        };
        self.latest_requested = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::latest_versions(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(latest) => this.latest_cli = latest.cli,
                    Err(err) => log::warn!("[ui] devices.latestVersions failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `devices.requestUpdate` — the daemon picks the flag up off its next
    /// heartbeat. FEED-36: `end_sessions` is "Update now…", which ends the
    /// live sessions holding a queued update instead of waiting them out.
    fn request_update(&mut self, end_sessions: bool, cx: &mut gpui::Context<Self>) {
        if self.update_busy {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let device_id = self.device_id.clone();
        self.update_busy = true;
        self.set_error("update", None);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::request_update(&trpc, &device_id, end_sessions)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.update_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] devices.requestUpdate failed: {err}");
                    this.set_error("update", Some(SharedString::from(format!("{err}"))));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// FEED-36: "Update now…" ends the machine's live sessions, so it
    /// confirms first (the web `MyMachines` twin, copy unchanged).
    fn prompt_update_now(
        &mut self,
        label: String,
        live_sessions: i64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            format!("Update \"{label}\" now?"),
            format!(
                "Ends the {live_sessions} live session(s) on this machine (repo-backed runs can be \
                 resumed from their session page) and restarts it on the new version."
            ),
            "Update now",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.request_update(true, cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// Remove behind a confirm — destructive native actions confirm first.
    fn prompt_remove(&mut self, label: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let view = cx.entity().downgrade();
        let spec = AlertSpec::new(
            format!("Remove \"{label}\"?"),
            "The machine drops off this list. One still running the daemon \
             re-registers itself on its next heartbeat.",
            "Remove",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| this.remove_device(cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// `devices.remove`. The dialog's own window is not reachable from the
    /// confirm's `on_ok` (that runs inside the ALERT window), so the close
    /// rides a flag [`Render`] acts on — the row this dialog edits is gone.
    fn remove_device(&mut self, cx: &mut gpui::Context<Self>) {
        if self.remove_busy {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let device_id = self.device_id.clone();
        self.remove_busy = true;
        self.set_error("remove", None);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::remove(&trpc, &device_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.remove_busy = false;
                match result {
                    Ok(()) => this.removed = true,
                    Err(err) => {
                        log::warn!("[ui] devices.remove failed: {err}");
                        this.set_error("remove", Some(SharedString::from(format!("{err}"))));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render pieces ---------------------------------------------------------

    fn error_line(&self, key: &str, cx: &App) -> Option<gpui::Div> {
        self.section_errors.get(key).map(|message| {
            div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(message.clone())
        })
    }

    /// The muted twin of [`Self::error_line`] for a command's success message.
    fn note_line(&self, key: &str, cx: &App) -> Option<gpui::Div> {
        self.section_notes.get(key).map(|message| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
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
            crate::controls::web_switch(id)
                .checked(checked)
                .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    on_click(this, checked, cx);
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        )
    }

    /// EXP-872: "Default account" is the SHARED account picker
    /// ([`crate::coding_selects::account_picker`]) over the logins THIS
    /// machine reports — "default agent" became "default account", and the
    /// agent derives from the pick. It writes BOTH: the agent through the
    /// `agent_select` state (whose observer owns the EXP-694 autosave) and
    /// the profile id into [`Self::default_account`], then commits, because
    /// picking a second login of the SAME agent moves no select at all.
    ///
    /// A machine that reports no login falls back to one row per editor
    /// agent, named by the agent ([`machine_account_options`]) — the row
    /// still has to be pickable on a machine that has not beaten yet.
    fn render_account_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let agent = CodingAgent::parse(&selected(&self.agent_select, cx))
            .unwrap_or(self.seeded.default_agent);
        let mut settings = self.seeded.clone();
        settings.default_agent = agent;
        settings.default_account = self.default_account.clone();
        let (accounts, usage) = self.reported_agent_status(cx);
        let options = crate::launch_options::machine_account_options(
            &accounts,
            &usage,
            &settings,
            &self.editor_agents,
        );
        let current = crate::launch_options::account_key(agent, self.default_account.as_deref());
        let select = self.agent_select.clone();
        let view = cx.entity().downgrade();
        crate::coding_selects::account_picker(
            "device-default-account",
            &options,
            Some(current.as_str()),
            crate::coding_selects::AccountTrigger::Row,
            move |option, window, cx| {
                write_choice(&select, option.agent.id(), window, cx);
                if let Some(view) = view.upgrade() {
                    let account = option.wire_account();
                    view.update(cx, |this, cx| {
                        this.default_account = account;
                        this.save_defaults(cx);
                        cx.notify();
                    });
                }
            },
            cx,
        )
    }

    /// The logins THIS dialog's machine reports: the live local probe for our
    /// own row, the synced `agent_accounts`/`agent_usage` columns for anyone
    /// else's.
    fn reported_agent_status(
        &self,
        cx: &mut App,
    ) -> (
        coding::agent_accounts::AgentAccounts,
        coding::agent_usage::AgentUsageMap,
    ) {
        if self.own {
            if let Some(demo) = dev_agent_status() {
                return (demo.accounts, demo.usage);
            }
            let hub = CodingHub::global(cx);
            let status = hub.read(cx).agent_status.clone();
            if let Some(status) = status {
                return (status.accounts, status.usage);
            }
        }
        let Some(store) = sync::Store::try_global(cx) else {
            return Default::default();
        };
        let devices = store.collections().devices.read(cx);
        let Some(row) = devices.iter().find(|row| row.id == self.device_row_id) else {
            return Default::default();
        };
        (
            parse_agent_map::<coding::agent_accounts::AgentAccount>(row.agent_accounts.as_ref()),
            parse_agent_map::<coding::agent_usage::AgentUsage>(row.agent_usage.as_ref()),
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
        let tab_agents = self.editor_agents.clone();
        // The picked tab can drop out of the editor's set between heartbeats
        // (a probe failed, a CLI went away): fall back to the first tab for
        // the PILL AND THE BODY together, never one without the other.
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
        let (model, effort) = match agent_tab {
            CodingAgent::Claude => (self.model_select.clone(), self.effort_select.clone()),
            CodingAgent::Codex => (
                self.codex_model_select.clone(),
                self.codex_effort_select.clone(),
            ),
        };
        // `Opus · Fable` ×4 — the same display labels the Model rows show,
        // never the raw aliases. The pair belongs to the DEFAULT account's
        // agent, so both the names and the list they are labelled against
        // follow it rather than the open tab.
        let drafted = self.drafted(cx);
        let workflow_choices = model_choices_for(drafted.default_agent);
        let workflow_summary = SharedString::from(format!(
            "{} · {}",
            choice_label(workflow_choices, &drafted.workflow_model),
            choice_label(workflow_choices, &drafted.workflow_strong_model),
        ));
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
        // EXP-1020: the "Workflow settings" page — the model pair a workflow
        // started on this machine is seeded from. It hangs off the DEFAULT
        // account's agent rather than the tab, so it shows on both.
        .trailing(vec![crate::sub_shell::sub_shell_row(
            crate::sub_shell::SubShellProps::new("device-workflow-settings", "Workflow settings")
                .icon(Icon::new(registry::NAV_WORKFLOWS))
                .value(workflow_summary),
            cx.listener(|this: &mut Self, _, _window, cx| {
                this.nav.open("Workflow settings");
                cx.notify();
            }),
            cx,
        )]);
        // EXP-981/EXP-1020: claude's subagent model, the row the IDE was
        // missing while web had it.
        if agent_tab.supports_subagent_model() {
            group = group.subagent(self.subagent_model_select.clone());
        }
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
        // EXP-686: no section title — the "Default account" row already
        // names what the block is.
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![surface::glass_picker_row(
                "Default account",
                None,
                self.render_account_picker(cx),
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

    /// FEED-33: the Sharing group — one switch per team the signed-in user
    /// belongs to, checked when the row's `shared_team_ids` names it. A
    /// machine can be shared with several teams at once, so this is a set of
    /// toggles rather than a picker. The rows read straight off the live
    /// row; a write in flight disables the whole set (one section write at a
    /// time).
    fn render_sharing_section(
        &self,
        row: Option<&domain::rows::DeviceRow>,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let teams = Store::global(cx).collections().teams_sorted(cx);
        if teams.is_empty() {
            return div()
                .text_xs()
                .text_color(muted)
                .child("Join a team to share this device.");
        }
        let busy = self.busy_section.is_some();
        let rows: Vec<Div> = teams
            .into_iter()
            .map(|team| {
                let checked = row.is_some_and(|row| row.is_shared_with(&team.id));
                let team_id = team.id.clone();
                surface::glass_toggle_row(
                    team.name.clone(),
                    None,
                    crate::controls::web_switch(SharedString::from(format!(
                        "device-share-{}",
                        team.id
                    )))
                    .checked(checked)
                        .disabled(busy)
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            this.save_sharing(team_id.clone(), *checked, cx);
                            cx.notify();
                        }))
                        .into_any_element(),
                    cx,
                )
            })
            .collect();
        v_flex()
            .w_full()
            .gap_1()
            .child(surface::glass_group_rows(rows))
            .child(div().text_xs().text_color(muted).child(
                "Teammates of a shared team can start coding sessions on this device.",
            ))
    }
}

/// FEED-36: the caption under a QUEUED update — the daemon's own rules for
/// getting there (every session ends, or one sits idle for 2 hours). Web
/// `QUEUED_UPDATE_TOOLTIP`, ×4.
const QUEUED_UPDATE_TOOLTIP: &str = "Live sessions hold this update — the device restarts itself \
     once every session ends or sits idle for 2 hours.";

impl DeviceSettingsView {
    /// EXP-909: the Update section — SERVER devices only (a desktop app
    /// updates itself), with the row's predicates unchanged: EXP-420 offers
    /// the request only when a newer CLI release really exists (or one is
    /// already in flight), FEED-36 parks it behind live sessions as "Queued"
    /// and offers "Update now…" on a daemon that advertises the cap. Up to
    /// date or offline, the row is the version alone.
    fn render_update_section(
        &mut self,
        row: Option<&domain::rows::DeviceRow>,
        online: bool,
        server: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        // The agent CLI rows: one per agent the machine reports an install
        // for, its version off the heartbeat's account row and an "Update"
        // that queues `agent_update` (the CLI's own self-updater, run there).
        // No cap: the button gates on the account row REPORTING a version —
        // the same build that started reporting it is the one that runs the
        // command; an older daemon/app answers "doesn't support that command
        // yet", so a version-less row gets a hint instead of a button.
        let (accounts, _) = self.reported_agent_status(cx);
        let muted = cx.theme().muted_foreground;
        let mut agent_rows: Vec<Div> = Vec::new();
        for agent in CodingAgent::ALL {
            let Some(account) = accounts.get(agent.id()) else {
                continue;
            };
            let key = format!("update {}", agent.id());
            let updating = self.tracked.iter().any(|command| command.key == key);
            let version = account.version.clone();
            let mut agent_row = surface::glass_row_shell().min_w_0().gap_2().child(
                v_flex().flex_1().min_w_0().gap_0p5().child(
                    div().w_full().min_w_0().truncate().text_sm().child(SharedString::from(
                        match version.as_deref() {
                            Some(version) => format!("{} v{version}", agent.label()),
                            None => agent.label().to_string(),
                        },
                    )),
                ),
            );
            if version.is_some() {
                agent_row = agent_row.child(
                    gpui_component::button::Button::new(("device-agent-update", agent as usize))
                        .ghost()
                        .web_sm()
                        .icon(Icon::new(registry::UI_UPDATE))
                        .label(if updating { "Updating…" } else { "Update" })
                        .loading(updating)
                        .disabled(updating)
                        .tooltip(if online {
                            format!("Run `{} update` on this machine.", agent.id())
                        } else {
                            format!(
                                "Run `{} update` on this machine (queued until it comes online).",
                                agent.id()
                            )
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.queue_agent_update(agent, cx);
                        })),
                );
            }
            // No reported version = a daemon that cannot run `agent_update`
            // yet. The hint sits UNDER the label (like a note), never beside
            // it: a sentence-long trailing slot truncated the agent's name on
            // the web twin at its narrower column.
            let under = self
                .error_line(&key, cx)
                .or_else(|| self.note_line(&key, cx))
                .or_else(|| {
                    version.is_none().then(|| {
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Update the app on this machine first")
                    })
                });
            match under {
                Some(line) => agent_rows.push(
                    v_flex()
                        .w_full()
                        .child(agent_row)
                        .child(div().px_4().pb_3().child(line)),
                ),
                None => agent_rows.push(agent_row),
            }
        }
        if !server {
            // Desktops update themselves (EXP-420/FEED-36): the section is
            // the agent rows alone, and only while there is one to show.
            if agent_rows.is_empty() {
                return div();
            }
            return v_flex()
                .w_full()
                .gap_2()
                .child(surface::glass_section_header("Update", None, cx))
                .child(surface::glass_group_rows(agent_rows));
        }
        self.ensure_latest_loaded(cx);
        let muted = cx.theme().muted_foreground;
        let amber = theme::tokens::YELLOW.to_hsla();
        let label = row
            .and_then(|row| row.label.clone())
            .unwrap_or_else(|| self.device_id.clone());
        let version = row.and_then(|row| row.version.clone());
        let latest = self.latest_cli.clone();
        let outdated = crate::machines::update_available(version.as_deref(), latest.as_deref());
        let requested = row.is_some_and(|row| row.update_requested_at.is_some());
        let updating = requested || self.update_busy;
        // EXP-411: the request is parked behind live sessions on the device —
        // "Queued" instead of an indefinite "Updating…".
        let live_sessions = row.and_then(|row| row.active_sessions).unwrap_or(0).max(0);
        let queued = requested && live_sessions > 0;
        let can_update = (online && outdated) || updating;
        let can_update_now =
            queued && row.is_some_and(|row| row.cap_ids().iter().any(|cap| cap == "update-now"));

        let mut controls = h_flex().flex_shrink_0().items_center().gap_1();
        if can_update {
            let tooltip: SharedString = if queued {
                QUEUED_UPDATE_TOOLTIP.into()
            } else {
                "Ask the daemon to self-update (it restarts when idle)".into()
            };
            controls = controls.child(
                gpui_component::button::Button::new("device-update")
                    .ghost()
                    .web_sm()
                    .icon(Icon::new(registry::UI_UPDATE))
                    .label(if queued {
                        "Queued"
                    } else if updating {
                        "Updating…"
                    } else {
                        "Update"
                    })
                    .loading(updating && !queued)
                    .text_color(if outdated { amber } else { muted })
                    .disabled(updating)
                    .tooltip(tooltip)
                    .on_click(cx.listener(|this, _, _, cx| this.request_update(false, cx))),
            );
        }
        if can_update_now {
            let confirm_label = label.clone();
            controls = controls.child(
                surface::glass_pill_button("device-update-now", surface::PillSize::Sm, cx)
                    .danger()
                    .label("Update now…")
                    .tooltip("End this device's live sessions and restart it on the new version now.")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.prompt_update_now(confirm_label.clone(), live_sessions, window, cx);
                    })),
            );
        }

        let mut version_line = v_flex().flex_1().min_w_0().gap_0p5().child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_sm()
                .child(match version.clone() {
                    Some(version) => SharedString::from(format!("v{version}")),
                    None => SharedString::from("Version unknown"),
                }),
        );
        if outdated {
            version_line = version_line.child(
                div()
                    .w_full()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(amber)
                    .child(SharedString::from(format!(
                        "Update available: v{}",
                        latest.unwrap_or_default()
                    ))),
            );
        }

        let mut rows = vec![surface::glass_row_shell()
            .min_w_0()
            .gap_2()
            .child(version_line)
            .child(controls)];
        rows.extend(agent_rows);
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_section_header("Update", None, cx))
            .child(surface::glass_group_rows(rows));
        if queued {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(amber)
                    .child(QUEUED_UPDATE_TOOLTIP),
            );
        }
        body.children(self.error_line("update", cx))
    }

    /// EXP-909: the Remove section — the row's old "Remove…" entry, confirm
    /// copy unchanged. The dialog closes behind a successful remove: the row
    /// it configures is gone.
    fn render_remove_section(
        &self,
        row: Option<&domain::rows::DeviceRow>,
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let label = row
            .and_then(|row| row.label.clone())
            .unwrap_or_else(|| self.device_id.clone());
        // EXP-1020: a ROW of the same shell, not a section of its own — a
        // destructive row needs no headline to be found.
        v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![surface::glass_row_shell()
                .min_w_0()
                .gap_2()
                .child(
                    gpui_component::button::Button::new("device-remove")
                        .danger()
                        .web_sm()
                        .icon(Icon::new(registry::UI_DELETE))
                        .label("Remove device")
                        .disabled(self.remove_busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.prompt_remove(label.clone(), window, cx);
                        })),
                )]))
            .children(self.error_line("remove", cx))
    }
}

impl Render for DeviceSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let row = self.row(cx);
        let online = self.online(cx);
        // EXP-909: Remove lives in this dialog now — once it lands, the row
        // this dialog configures is gone and the dialog goes with it. The
        // close DEFERS, so calling it from a render is safe (and the confirm's
        // `on_ok` runs inside the alert window, which cannot close this one).
        if self.removed {
            native_dialog::close_dialog_window(window, cx);
        }
        let muted = cx.theme().muted_foreground;
        let server = row
            .as_ref()
            .map(|row| row.is_server())
            .unwrap_or(false);

        // EXP-694: the device's own fields as THREE inset-grouped cards on the
        // shared 8px group rhythm — the name typed into its row (autosaved, no
        // Save button), the default-device switch (EXP-622: a straight-through
        // write, which is now what every control here does), and the sharing
        // switches (FEED-33: one per team). One card per section, exactly as
        // the Android/iOS/web dialogs render them.
        let is_default = row.as_ref().and_then(|row| row.is_default).unwrap_or(false);
        // EXP-924: the identity row every form on this client shares (board,
        // action) — the icon picker, then the bare name field. Only the row's
        // OWNER may write either, so a row that is not mine (or not synced at
        // all) shows the glyph without a picker hung off it.
        let icon_name = self.icon_name(row.as_ref());
        let mine = row
            .as_ref()
            .and_then(|row| row.user_id.clone())
            .is_some_and(|user_id| {
                queries::active_account(cx).is_some_and(|me| me.user_id == user_id)
            });
        let icon_control = if mine {
            let view = cx.entity().clone();
            crate::board_form::icon_picker(
                "device-settings",
                registry::DEVICE_ICONS,
                Some(icon_name),
                None,
                false,
                move |name, _, cx| {
                    let Some(name) = name else { return };
                    view.update(cx, |this, cx| {
                        this.save_icon(name, cx);
                        cx.notify();
                    });
                },
                cx,
            )
            .into_any_element()
        } else {
            // Not mine to change: the glyph alone, on the same 32px control
            // rung the picker's trigger occupies, so the row keeps its rhythm.
            h_flex()
                .flex_shrink_0()
                .size(px(crate::controls::CTL_MD_H))
                .justify_center()
                .child(
                    Icon::new(crate::icons::device_icon(Some(icon_name), server))
                        .text_color(muted),
                )
                .into_any_element()
        };
        let identity_row = surface::glass_row_shell()
            .gap_2()
            .child(icon_control)
            .child(
                div().flex_1().min_w_0().child(
                    surface::glass_row_input(glass_input(&self.name_input, window, cx))
                        .text_left(),
                ),
            );
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![identity_row]))
            .child(surface::glass_group_rows(vec![Self::toggle_row(
                "device-default",
                "Default device",
                is_default,
                move |this, checked, cx| this.save_default(*checked, cx),
                cx,
            )]));
        if server {
            body = body.child(self.render_sharing_section(row.as_ref(), cx));
        }
        // The autosave says nothing while it succeeds; a write in flight or a
        // failed one reports under the group it belongs to.
        for section in ["name", "icon", "default", "sharing"] {
            if self.busy_section == Some(section) {
                body = body.child(div().text_xs().text_color(muted).child("Saving…"));
            }
            if let Some(error) = self.error_line(section, cx) {
                body = body.child(error);
            }
        }

        // Every group in the dialog sits on the SAME 8px rhythm (the ×4
        // parity look).
        let mut body = body.child(self.render_defaults_section(online, cx));
        // EXP-909: Update and Remove are the LAST two sections ×4 — the
        // device row carries one control now (the gear that opened this), so
        // these are the only place left that updates or removes a machine.
        // Desktops update themselves: the daemon row is the server's; the
        // agent CLI rows (claude/codex self-update) show for both kinds.
        body = body.child(self.render_update_section(row.as_ref(), online, server, cx));
        let body = body.child(self.render_remove_section(row.as_ref(), cx));

        // EXP-1020: ONE column. The second one (EXP-762/798) existed to park
        // the worktrees beside the settings, and a machine's worktrees are a
        // LOCAL surface now — Settings → Worktrees.
        //
        // EXP-1029/1020: the stack is a SUB-SHELL host, so "Workflow
        // settings" slides its page in place of the whole stack (back glyph
        // on top) rather than opening a dialog on top of a dialog.
        let mut host = crate::sub_shell::SubShellHost::new(body.pr_2().pb_2());
        if self.nav.is_open() {
            let (model, strong_model) = self.workflow_selects(self.default_agent(cx));
            let page = render_workflow_page(&model, &strong_model, cx);
            host = host.open(
                crate::sub_shell::SubShellPage::new("Workflow settings", page),
                cx.listener(|this: &mut Self, _, _window, cx| {
                    this.nav.back();
                    cx.notify();
                }),
            );
        }
        let body = div().w_full().child(host.render(window, cx));

        // EXP-762: ONE scroll pane (EXP-1020 dropped the second column). The
        // dialog is `self_scrolling` (see [`open`]) so this root gets a
        // DEFINITE height and the pane resolves to the visible height, and
        // the pane's scroll area is a BLOCK container: nothing inside can be
        // flex-squeezed the way the shell's `overflow_y_scrollbar` wrapper
        // squeezed the overflow-clipped agent card (a taffy scroll container
        // has an automatic minimum size of ZERO, so it was the one thing in
        // the column that could give — and the rows at its foot were what got
        // cut).
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "device-settings-settings",
                &self.settings_scroll,
                body,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the dialog: it counts the EXP-694 autosave, which is an
    /// observer on the select the picker writes through.
    struct Saver {
        saves: usize,
    }

    impl gpui::Render for Saver {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div()
        }
    }

    /// EXP-872: "Default account" is the shared account picker now, and its
    /// pick still reaches the agent select PROGRAMMATICALLY — and
    /// `set_selected_value` alone never notifies, which silently dropped the
    /// new default. The write has to carry the autosave with it. (The profile
    /// half of the pick commits directly, since picking a second login of the
    /// SAME agent moves no select at all.)
    #[gpui::test]
    async fn picking_a_default_account_reaches_the_autosave(cx: &mut gpui::TestAppContext) {
        let (saver, cx) = cx.add_window_view(|_, _| Saver { saves: 0 });
        let select = saver.update_in(cx, |_, window, cx| {
            let select = choice_select(&AGENT_CHOICES, CodingAgent::Claude.id(), window, cx);
            cx.observe(&select, |this: &mut Saver, _, _| this.saves += 1).detach();
            select
        });
        cx.run_until_parked();
        saver.update(cx, |this, _| assert_eq!(this.saves, 0));

        saver.update_in(cx, |_, window, cx| {
            write_choice(&select, CodingAgent::Codex.id(), window, cx);
        });
        cx.run_until_parked();
        saver.update(cx, |_, cx| {
            assert_eq!(selected(&select, cx), CodingAgent::Codex.id());
        });
        saver.update(cx, |this, _| {
            assert_eq!(this.saves, 1, "the pick has to reach the autosave observer")
        });
    }

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

    /// EXP-765: the code field is offered for a REMOTE machine only — the own
    /// machine's login tab is right there to type into.
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
            "gemini": "signed in",
        });
        let accounts =
            parse_agent_map::<coding::agent_accounts::AgentAccount>(Some(&value));
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts["claude"].email.as_deref(), Some("dev@acme.test"));
        assert!(!accounts["codex"].signed_in);
        assert!(!accounts.contains_key("gemini"));
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
