//! Settings → General + Danger Zone (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/general-section.tsx` (name `Input`,
//! dirty-gated Save; teams are always private — there is no visibility
//! setting) and the Danger Zone card of
//! `routes/t/$teamSlug/settings/general.tsx` (type-the-name-to-confirm
//! delete, gated owner + team-only).
//!
//! Local state mirrors the web's `useState` + resync-on-team-change
//! `useEffect`: an Electric echo that changes the synced row overwrites the
//! local draft (which is exactly how the post-save echo clears `dirty`).

use std::time::Instant;

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState, Textarea, TextareaState},
    v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use crate::controls::{glass_input, web_textarea, WebControl as _};
use crate::feedback;
use crate::toast::{self, Toast};
use crate::native_dialog::{self, AlertEnter, AlertSpec};
use crate::navigation::Navigation;

use super::{
    active_team, danger_zone, is_owner, open_url, section,
    team_delete_error_message,
};
use crate::icons::registry;

/// Server fetch state for the read-only billing summary (EXP-288).
enum BillingLoad {
    Idle,
    Loading,
    Ready(api::billing::TeamPlanOut),
    /// Errors hide the section (logged) — billing must never block the
    /// name/danger-zone cards.
    Failed,
}

/// Snapshot of the synced fields the pane mirrors — resync happens whenever
/// this differs from the live row (the web `useEffect` dep list).
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    team_id: String,
    name: String,
}

// EXP-1025: the team prompt editor's copy — byte-identical to the web
// (`components/team/general-section.tsx`). EXP-1054: one short placeholder,
// no helper paragraph under the field — the counter is the footer.
const TEAM_PROMPT_TITLE: &str = "Team prompt";
const TEAM_PROMPT_PLACEHOLDER: &str = "Rules every coding session of this team follows, as markdown.";

// EXP-1105: the yolo-mode toggle — label only, no description (web parity).
const YOLO_MODE_TITLE: &str = "Yolo mode";

/// `12.3k` / `840` — the counter's short form (web `formatByteCount`).
fn format_byte_count(bytes: usize) -> String {
    if bytes < 1000 {
        bytes.to_string()
    } else {
        format!("{:.1}k", bytes as f64 / 1024.0)
    }
}

/// The rough token readout beside the byte counter (web `approxTokens`):
/// chars÷4, labelled ≈ so nobody reads it as a measurement.
fn approx_tokens(bytes: usize) -> String {
    let tokens = (bytes as f64 / 4.0).round() as usize;
    if tokens < 1000 {
        format!("≈{tokens} tokens")
    } else {
        format!("≈{:.1}k tokens", tokens as f64 / 1000.0)
    }
}

/// EXP-1275: the byte cap's INLINE line under the field (feedback-states
/// `errors` case "a prompt over its byte cap stays under its field").
pub(crate) fn prompt_too_long(used: usize) -> String {
    format!(
        "Too long: {used} of {} bytes",
        domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES
    )
}

/// The counter line: `1.2k / 12.0k bytes · ≈300 tokens`.
fn prompt_counter(bytes: usize) -> String {
    format!(
        "{} / {} bytes · {}",
        format_byte_count(bytes),
        format_byte_count(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES),
        approx_tokens(bytes)
    )
}

/// EXP-1025: the team prompt's fetch state. The text is NOT on the synced
/// team row (server-only, like an action's body), so the pane loads it once
/// per team and the field stays disabled until it lands — a blur can never
/// save an empty draft over a prompt that has not arrived (nor after the
/// load failed).
/// The team prompt when its load failed (web `general-section.tsx`).
const PROMPT_UNAVAILABLE: &str = "Prompt unavailable";

/// ONE team prompt write: the team it belongs to and exactly the text to
/// send.
#[derive(Clone)]
struct PromptSave {
    team_id: String,
    text: String,
}

/// Final review B7 — ONE team's prompt writes, serialized: one save in
/// flight, the LAST text asked for while it is out queued behind it (an
/// older queued one is replaced), and the newest text asked for at all (a
/// failed save's Retry sends that, never the failed snapshot). Pure, so the
/// ordering is testable without a window.
#[derive(Default)]
struct TeamPromptQueue {
    /// The write on the wire: its sequence number.
    in_flight: Option<u64>,
    /// The latest text asked for while one was in flight.
    queued: Option<String>,
    /// The newest text asked for at all.
    latest: Option<String>,
    /// What the server last confirmed (trailing whitespace dropped, like it
    /// does), and when.
    confirmed: Option<String>,
    confirmed_at: Option<Instant>,
    /// The last sequence number handed out.
    seq: u64,
}

impl TeamPromptQueue {
    /// Ask for `text` to be saved: `Some((seq, text))` = send it now; `None`
    /// = a write is out (it waits, the newest text winning) or there is
    /// nothing new to say.
    fn ask(&mut self, text: String) -> Option<(u64, String)> {
        self.latest = Some(text.clone());
        if self.in_flight.is_some() {
            self.queued = Some(text);
            return None;
        }
        self.begin(text)
    }

    fn begin(&mut self, text: String) -> Option<(u64, String)> {
        if self.confirmed.as_deref() == Some(text.trim_end()) {
            return None;
        }
        self.seq += 1;
        self.in_flight = Some(self.seq);
        Some((self.seq, text))
    }

    /// Write `seq` (of `text`) settled: the next write to send, if any.
    fn settled(&mut self, seq: u64, text: &str, ok: bool, now: Instant) -> Option<(u64, String)> {
        if self.in_flight == Some(seq) {
            self.in_flight = None;
        }
        if ok {
            self.confirmed = Some(text.trim_end().to_string());
            self.confirmed_at = Some(now);
        }
        let next = self.queued.take()?;
        self.begin(next)
    }

    /// Whether a write is out or waiting (the `Saving` caption).
    fn busy(&self) -> bool {
        self.in_flight.is_some() || self.queued.is_some()
    }

    /// A failed save's Retry once no editor shows the team: its newest text,
    /// unless that is already what the server holds.
    fn retry_text(&self) -> Option<String> {
        self.latest
            .clone()
            .filter(|latest| self.confirmed.as_deref() != Some(latest.trim_end()))
    }
}

/// Final review B7 — every team's prompt write queue, app-wide so it
/// outlives the pane: a release flush joins the same queue as the save in
/// flight instead of racing it, and ⌘Q drains it
/// ([`flush_prompt_saves_on_quit`]).
#[derive(Default)]
struct PromptSaves {
    teams: std::collections::HashMap<String, TeamPromptSlot>,
    /// ⌘Q is in progress: the foreground runs no more tasks, so asks only
    /// queue and the quit hook sends them.
    quitting: bool,
}

impl gpui::Global for PromptSaves {}

#[derive(Default)]
struct TeamPromptSlot {
    queue: TeamPromptQueue,
    /// The pane showing this team's prompt, the source of its CURRENT text.
    editor: Option<gpui::WeakEntity<GeneralPane>>,
    /// The highest write sequence number that settled, for the quit hook to
    /// wait behind the write in flight.
    wire: std::sync::Arc<(std::sync::Mutex<u64>, std::sync::Condvar)>,
}

/// How long ⌘Q waits for the team prompt's last writes (the same budget the
/// session ends get, `coding_flow::QUIT_END_TIMEOUT`).
const PROMPT_QUIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// The queue global, installed once with its quit hook.
fn prompt_saves(cx: &mut App) -> &mut PromptSaves {
    if !cx.has_global::<PromptSaves>() {
        cx.set_global(PromptSaves::default());
        cx.on_app_quit(|cx| {
            flush_prompt_saves_on_quit(cx);
            async {}
        })
        .detach();
    }
    cx.global_mut::<PromptSaves>()
}

/// Save `text` as team `team_id`'s prompt, through that team's queue.
fn queue_prompt_save(team_id: String, text: String, cx: &mut App) {
    let saves = prompt_saves(cx);
    let quitting = saves.quitting;
    let slot = saves.teams.entry(team_id.clone()).or_default();
    if quitting {
        slot.queue.latest = Some(text.clone());
        slot.queue.queued = Some(text);
        return;
    }
    if let Some((seq, text)) = slot.queue.ask(text) {
        send_prompt_write(team_id, seq, text, cx);
    }
}

/// Put write `seq` on the wire; its answer settles the queue
/// ([`prompt_write_settled`]), which sends the next one.
fn send_prompt_write(team_id: String, seq: u64, text: String, cx: &mut App) {
    let wire = prompt_saves(cx)
        .teams
        .get(&team_id)
        .map(|slot| slot.wire.clone())
        .unwrap_or_default();
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        // Signed out mid-edit: nothing can be sent; the queue moves on.
        prompt_write_settled(team_id, seq, text, Err(None), cx);
        return;
    };
    let mut input = api::teams::TeamsUpdateInput::new(team_id.clone());
    input.agent_prompt = Some(text.clone());
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                let result = api::teams::teams_update(&trpc, &input).map(|_| ());
                let (done, wake) = &*wire;
                if let Ok(mut done) = done.lock() {
                    *done = (*done).max(seq);
                }
                wake.notify_all();
                result
            })
            .await;
        let _ = cx.update(|cx| prompt_write_settled(team_id, seq, text, result.map_err(Some), cx));
    })
    .detach();
}

/// Write `seq` of team `team_id` settled: a failure toasts (its Retry sends
/// the team's CURRENT text, [`retry_prompt_save`]); then the next queued
/// write goes out. Every open pane repaints off the global.
fn prompt_write_settled(
    team_id: String,
    seq: u64,
    text: String,
    result: Result<(), Option<api::ApiError>>,
    cx: &mut App,
) {
    if let Err(Some(err)) = &result {
        log::warn!("[ui] teams.update(agentPrompt) failed: {err}");
        let retry_team = team_id.clone();
        let note = Toast::error("Couldn't save the team prompt")
            .description(err.user_message())
            .action(feedback::RETRY, move |_, cx| retry_prompt_save(&retry_team, cx));
        toast::show_in_active_window(note, cx);
    }
    let saves = prompt_saves(cx);
    let quitting = saves.quitting;
    let Some(slot) = saves.teams.get_mut(&team_id) else {
        return;
    };
    let next = slot.queue.settled(seq, &text, result.is_ok(), Instant::now());
    if let (Some((seq, text)), false) = (next, quitting) {
        send_prompt_write(team_id, seq, text, cx);
    }
}

/// A failed save's Retry (review B7): the team's CURRENT text, never the
/// snapshot that failed. A pane showing that team's loaded prompt sends
/// what its field holds (nothing, when that is already saved); otherwise
/// the newest text asked for.
fn retry_prompt_save(team_id: &str, cx: &mut App) {
    let editor = prompt_saves(cx)
        .teams
        .get(team_id)
        .and_then(|slot| slot.editor.as_ref())
        .and_then(gpui::WeakEntity::upgrade)
        .filter(|pane| pane.read(cx).shows_loaded_prompt(team_id));
    if let Some(pane) = editor {
        pane.update(cx, |pane, cx| pane.save_prompt(cx));
        return;
    }
    let text = prompt_saves(cx)
        .teams
        .get(team_id)
        .and_then(|slot| slot.queue.retry_text());
    if let Some(text) = text {
        queue_prompt_save(team_id.to_string(), text, cx);
    }
}

/// ⌘Q (review #8 / B7): gpui runs this BEFORE it stops the foreground, and
/// grants returned futures only `SHUTDOWN_TIMEOUT` (200 ms), so the writes
/// go out from here, synchronously, on their own threads: every open pane's
/// unsaved text joins its team's queue, then each team's last text is sent
/// once the write already in flight for it settled (never before it, so an
/// older text cannot land last), all within [`PROMPT_QUIT_TIMEOUT`].
fn flush_prompt_saves_on_quit(cx: &mut App) {
    let editors: Vec<Entity<GeneralPane>> = prompt_saves(cx)
        .teams
        .values()
        .filter_map(|slot| slot.editor.as_ref().and_then(gpui::WeakEntity::upgrade))
        .collect();
    prompt_saves(cx).quitting = true;
    for pane in editors {
        if let Some(save) = pane.read(cx).pending_prompt_save(cx) {
            queue_prompt_save(save.team_id, save.text, cx);
        }
    }
    let Some(trpc) = crate::queries::trpc_client(cx).map(std::sync::Arc::new) else {
        return;
    };
    let writes: Vec<_> = prompt_saves(cx)
        .teams
        .iter_mut()
        .filter_map(|(team_id, slot)| {
            let text = slot.queue.queued.take()?;
            let behind = slot.queue.in_flight.unwrap_or(0);
            Some((team_id.clone(), text, behind, slot.wire.clone()))
        })
        .collect();
    if writes.is_empty() {
        return;
    }
    let deadline = Instant::now() + PROMPT_QUIT_TIMEOUT;
    let (sent, done) = std::sync::mpsc::channel::<()>();
    let count = writes.len();
    for (team_id, text, behind, wire) in writes {
        let trpc = std::sync::Arc::clone(&trpc);
        let sent = sent.clone();
        std::thread::spawn(move || {
            let (settled, wake) = &*wire;
            let mut guard = settled.lock().unwrap_or_else(|poison| poison.into_inner());
            while *guard < behind {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return;
                }
                guard = wake
                    .wait_timeout(guard, left)
                    .unwrap_or_else(|poison| poison.into_inner())
                    .0;
            }
            drop(guard);
            let mut input = api::teams::TeamsUpdateInput::new(team_id);
            input.agent_prompt = Some(text);
            if let Err(err) = api::teams::teams_update(&trpc, &input) {
                log::warn!("[ui] teams.update(agentPrompt) on quit failed: {err}");
            }
            let _ = sent.send(());
        });
    }
    for _ in 0..count {
        let left = deadline.saturating_duration_since(Instant::now());
        if done.recv_timeout(left).is_err() {
            break;
        }
    }
}

#[derive(Default)]
struct PromptState {
    /// The team the loaded text belongs to; a team switch reloads.
    team_id: Option<String>,
    loading: bool,
    load_failed: bool,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); the load on open fails quietly into the empty state.
    retry_pressed: bool,
    /// What the server holds (trailing whitespace dropped, like it does).
    saved: String,
    /// ISO stamp of the last write, for the "Edited …" caption.
    updated_at: Option<String>,
    /// EXP-1275: the autosave debounce — each keystroke replaces it
    /// (dropping the previous timer cancels it).
    debounce: Option<Task<()>>,
    /// EXP-1275: the last save landed at — the caption reads `Saved` for
    /// `feedback::AUTOSAVE_SAVED_HOLD` after it.
    saved_at: Option<Instant>,
    /// The queue's last confirmation this pane took over (`PromptSaves`):
    /// a newer one is a save that landed.
    seen_confirm: Option<Instant>,
    /// Bumped per fetch so a slow reply for the previous team is dropped.
    generation: u64,
}

pub struct GeneralPane {
    nav: Entity<Navigation>,
    name_input: Entity<InputState>,
    delete_input: Entity<InputState>,
    /// EXP-1025: the team prompt — a RAW monospace field, never the markdown
    /// WYSIWYG: a prompt is read by a model, so the bytes shown must be the
    /// bytes it gets.
    prompt_input: Entity<TextareaState>,
    prompt: PromptState,
    snapshot: Option<Snapshot>,
    saving: bool,
    /// EXP-1105: a yolo-mode `teams.update` is in flight (switch disabled).
    yolo_busy: bool,
    /// EXP-288: the read-only plan/usage summary between the name card and
    /// the Danger Zone. Refetched on team/account change; hidden while
    /// loading/failed and entirely on self-hosted (`plan == "unlimited"`).
    billing: BillingLoad,
    billing_team: Option<String>,
    billing_account: Option<String>,
    billing_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Team name"));
        let delete_input = cx.new(|cx| InputState::new(window, cx));
        // D8: no "Loading…" placeholder — the field is disabled under the
        // header's inline `Loading` status until the text lands.
        let prompt_input = cx.new(|cx| web_textarea(10, 40, window, cx));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Resync needs the window (set_value) — window-aware observers.
            cx.observe_in(&nav, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe_in(&collections.teams, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // EXP-818: the name row saves ITSELF — on blur and on Enter, the
            // Linear way (web `GlassInputRow` twin). No Save button.
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } | InputEvent::Blur => this.save(cx),
                _ => {}
            }),
            // EXP-1275: the prompt AUTOSAVES — `AUTOSAVE_DEBOUNCE` after the
            // last keystroke and on blur (Enter is a newline in a textarea).
            cx.subscribe(&prompt_input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.schedule_prompt_save(cx);
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.prompt.debounce = None;
                    this.save_prompt(cx);
                }
                _ => {}
            }),
        ];
        // Review #8: leaving Settings (or closing the window) drops the pane
        // — an edit still in its debounce is sent on the way out.
        cx.on_release(|this, cx| this.flush_prompt_on_release(cx)).detach();
        // Final review B7: the writes run in the app-wide queue; the pane
        // follows it for `Saving` / `Saved`.
        prompt_saves(cx);
        let mut subscriptions = subscriptions;
        subscriptions.push(cx.observe_global::<PromptSaves>(|this, cx| this.sync_prompt_saves(cx)));

        let mut this = Self {
            nav,
            name_input,
            delete_input,
            prompt_input,
            prompt: PromptState::default(),
            snapshot: None,
            saving: false,
            yolo_busy: false,
            billing: BillingLoad::Idle,
            billing_team: None,
            billing_account: None,
            billing_generation: 0,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Kick the `billing.teamPlan` fetch when the pane is shown or the
    /// team/account changed (render-time — a hidden pane never fetches;
    /// generation-guarded like the boards repo cache).
    fn ensure_billing(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.billing_account {
            self.billing_account = account_id;
            self.billing = BillingLoad::Idle;
        }
        if self.billing_team.as_deref() == Some(team_id)
            && !matches!(self.billing, BillingLoad::Idle)
        {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.billing = BillingLoad::Loading;
        self.billing_team = Some(team_id.to_string());
        self.billing_generation += 1;
        let generation = self.billing_generation;
        let team_id = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::billing::billing_team_plan(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.billing_generation != generation {
                    return; // superseded
                }
                this.billing = match result {
                    Ok(plan) => BillingLoad::Ready(plan),
                    Err(err) => {
                        log::warn!("[ui] billing.teamPlan failed: {err}");
                        BillingLoad::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Mirror the web `useEffect`: whenever the synced row (or the active
    /// team) changes, replace the local draft.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team) = active_team(cx, &self.nav) else {
            return;
        };
        let snapshot = Snapshot {
            team_id: team.id.clone(),
            name: team.name.clone(),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name_input.update(cx, |state, cx| {
            state.set_value(snapshot.name.clone(), window, cx);
        });
        let team_changed = self.snapshot.as_ref().map(|held| &held.team_id) != Some(&snapshot.team_id);
        self.snapshot = Some(snapshot);
        if team_changed {
            self.fetch_prompt(window, cx);
        }
        cx.notify();
    }

    /// EXP-1025: the ONE read path for the team prompt (`teams.getAgentPrompt`;
    /// the shape excludes it). Clears the field first so the previous team's
    /// text never shows under the new team's name.
    fn fetch_prompt(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.snapshot.as_ref().map(|snapshot| snapshot.team_id.clone()) else {
            return;
        };
        // EXP-1275: an unsaved edit belongs to the team being left: queue it
        // (with that team's id) before the field clears.
        self.prompt.debounce = None;
        self.save_prompt(cx);
        self.prompt.generation += 1;
        let generation = self.prompt.generation;
        self.prompt.team_id = Some(team_id.clone());
        // This pane is now where that team's CURRENT text lives (a failed
        // save's Retry reads it); confirmations from before do not apply.
        let editor = cx.entity().downgrade();
        let slot = prompt_saves(cx).teams.entry(team_id.clone()).or_default();
        slot.editor = Some(editor);
        self.prompt.seen_confirm = slot.queue.confirmed_at;
        self.prompt.loading = true;
        self.prompt.load_failed = false;
        self.prompt.saved.clear();
        self.prompt.updated_at = None;
        self.prompt.saved_at = None;
        self.prompt_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder("", window, cx);
        });
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            self.prompt.loading = false;
            self.prompt.load_failed = true;
            return;
        };
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::teams::teams_get_agent_prompt(&trpc, &team_id) })
                .await;
            let _ = this.update_in(window, |view, window, cx| {
                if view.prompt.generation != generation {
                    return;
                }
                view.prompt.loading = false;
                let attended = std::mem::take(&mut view.prompt.retry_pressed);
                match result {
                    Ok(prompt) => {
                        view.prompt.saved = prompt.agent_prompt.clone();
                        view.prompt.updated_at = prompt.agent_prompt_updated_at;
                        view.prompt_input.update(cx, |state, cx| {
                            state.set_value(prompt.agent_prompt, window, cx);
                            state.set_placeholder(TEAM_PROMPT_PLACEHOLDER, window, cx);
                        });
                    }
                    Err(err) => {
                        // FEED-79: the load on open fails quietly: the
                        // field's empty state + Retry (nothing to save over);
                        // only a Retry press that fails again toasts.
                        view.prompt.load_failed = true;
                        crate::action_error::failed_load(
                            "the team prompt",
                            attended,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The failed prompt's Retry: the same load, attended.
    fn retry_prompt(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.prompt.retry_pressed = true;
        self.fetch_prompt(window, cx);
        cx.notify();
    }

    fn prompt_bytes(&self, cx: &App) -> usize {
        self.prompt_input.read(cx).value().len()
    }

    fn prompt_dirty(&self, cx: &App) -> bool {
        !self.prompt.loading
            && !self.prompt.load_failed
            && self.prompt_input.read(cx).value().trim_end() != self.prompt.saved
    }

    /// EXP-1275: (re)start the autosave debounce: the save fires
    /// `AUTOSAVE_DEBOUNCE` after the LAST keystroke.
    fn schedule_prompt_save(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.prompt_dirty(cx) {
            self.prompt.debounce = None;
            return;
        }
        self.prompt.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(feedback::AUTOSAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.prompt.debounce = None;
                this.save_prompt(cx);
            });
        }));
    }

    /// The write the field holds right now, or `None` when there is nothing
    /// to send: no team, still loading, equal to the last saved text, or
    /// over the cap (the inline line says so and the server would refuse
    /// it anyway).
    fn pending_prompt_save(&self, cx: &App) -> Option<PromptSave> {
        let team_id = self.prompt.team_id.clone()?;
        if !self.prompt_dirty(cx) {
            return None;
        }
        if self.prompt_bytes(cx) > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES {
            return None;
        }
        Some(PromptSave {
            team_id,
            text: self.prompt_input.read(cx).value().to_string(),
        })
    }

    /// EXP-1025/1275: the ONE save path (debounce, blur, team switch) — the
    /// web `handleSavePrompt` twin.
    fn save_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(save) = self.pending_prompt_save(cx) {
            self.send_prompt(save, cx);
        }
    }

    /// Send `save` through its team's queue (final review B7: one write in
    /// flight per team, the newest text waiting behind it).
    fn send_prompt(&mut self, save: PromptSave, cx: &mut gpui::Context<Self>) {
        self.prompt.saved_at = None;
        queue_prompt_save(save.team_id, save.text, cx);
        cx.notify();
    }

    /// Whether this pane shows team `team_id`'s prompt, loaded (its field
    /// is that team's current text).
    fn shows_loaded_prompt(&self, team_id: &str) -> bool {
        self.prompt.team_id.as_deref() == Some(team_id)
            && !self.prompt.loading
            && !self.prompt.load_failed
    }

    /// A write for this pane's team is out or waiting (the `Saving` caption).
    fn prompt_saving(&self, cx: &App) -> bool {
        let Some(team_id) = self.prompt.team_id.as_deref() else {
            return false;
        };
        cx.try_global::<PromptSaves>()
            .and_then(|saves| saves.teams.get(team_id))
            .is_some_and(|slot| slot.queue.busy())
    }

    /// The queue moved (`PromptSaves`): a save of this pane's team that
    /// landed since the pane last looked becomes the saved text and the
    /// `Saved` caption.
    fn sync_prompt_saves(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.prompt.team_id.as_deref() else {
            return;
        };
        let confirmed = cx
            .try_global::<PromptSaves>()
            .and_then(|saves| saves.teams.get(team_id))
            .and_then(|slot| Some((slot.queue.confirmed.clone()?, slot.queue.confirmed_at?)));
        if let Some((text, at)) = confirmed {
            if self.prompt.seen_confirm != Some(at) {
                self.prompt.seen_confirm = Some(at);
                self.prompt.saved = text;
                self.prompt.updated_at = Some(chrono::Utc::now().to_rfc3339());
                self.prompt.saved_at = Some(at);
                // Repaint once the `Saved` hold is over.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(feedback::AUTOSAVE_SAVED_HOLD).await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
        }
        cx.notify();
    }

    /// Review #8: the pane is going away (Settings left, window closed): a
    /// dirty, under-cap edit joins its team's queue now (behind a write in
    /// flight, never racing it; the queue outlives the pane).
    fn flush_prompt_on_release(&mut self, cx: &mut App) {
        if let Some(save) = self.pending_prompt_save(cx) {
            queue_prompt_save(save.team_id, save.text, cx);
        }
    }

    /// EXP-1025: the team prompt section — header with the save caption, the
    /// raw monospace field in its own glass block, the help line and the
    /// byte/token counter under it. Owner-only, and HIDDEN from everyone
    /// else like every owner control (the web route hides the whole
    /// section too): a non-owner gets nothing here, not a disabled field.
    fn render_prompt_section(&self, owner: bool, cx: &mut gpui::Context<Self>) -> gpui::Div {
        if !owner {
            return div();
        }
        let bytes = self.prompt_bytes(cx);
        let over = bytes > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES;
        // EXP-1275: `Saving` (spinner) → `Saved` for the hold → `Edited
        // {ago}`; there is no Unsaved state.
        let just_saved = self
            .prompt
            .saved_at
            .is_some_and(|at| at.elapsed() < feedback::AUTOSAVE_SAVED_HOLD);
        // D8: the load = the header's inline `Loading` status (the field
        // stays disabled), never a "Loading…" placeholder.
        let trailing = if self.prompt.loading {
            Some(feedback::inline_status(feedback::WORD_LOADING, true, cx))
        } else if self.prompt_saving(cx) {
            Some(feedback::inline_status(feedback::AUTOSAVE_SAVING, true, cx))
        } else if just_saved {
            Some(feedback::inline_status(feedback::AUTOSAVE_SAVED, false, cx))
        } else {
            self.prompt.updated_at.as_deref().map(|stamp| {
                feedback::inline_status(
                    feedback::autosave_edited(&crate::inbox::relative_time(stamp)),
                    false,
                    cx,
                )
            })
        }
        .map(IntoElement::into_any_element);
        let field = Textarea::new(&self.prompt_input)
            .appearance(false)
            .w_full()
            .px_4()
            .py_3()
            .font_family(theme::terminal::FONT_FAMILY)
            .text_xs()
            .disabled(self.prompt.loading || self.prompt.load_failed);
        let footer = h_flex()
            .w_full()
            .items_start()
            .justify_end()
            .gap_4()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(super::row_stroke(cx))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .flex_shrink_0()
                    .when(over, |counter| counter.text_color(cx.theme().danger))
                    .child(prompt_counter(bytes)),
            );
        // A failed load (FEED-79): the field's empty state + Retry.
        if self.prompt.load_failed && !self.prompt.loading {
            return section(cx)
                .child(crate::surface::glass_section_header(TEAM_PROMPT_TITLE, None, cx))
                .child(crate::surface::glass_group().child(
                    crate::action_error::load_failed_state(
                        "team-prompt-retry",
                        gpui_component::Icon::new(registry::SETTINGS_GENERAL),
                        PROMPT_UNAVAILABLE,
                        cx.listener(|this, _, window, cx| this.retry_prompt(window, cx)),
                        cx,
                    ),
                ));
        }
        section(cx)
            .child(crate::surface::glass_section_header(TEAM_PROMPT_TITLE, trailing, cx))
            .child(crate::surface::glass_group().child(field).child(footer))
            // FEED-79: a byte cap is INVALID INPUT — inline under the field.
            .when(over, |body| {
                body.child(crate::action_error::field_error(prompt_too_long(bytes), cx))
            })
    }

    fn dirty(&self, cx: &App) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name_input.read(cx).value().as_ref() != snapshot.name
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        if !self.dirty(cx) || self.saving {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        let typed = self.name_input.read(cx).value().trim().to_string();
        // Web: `name.trim() || team.name` — an emptied field falls back.
        let name = if typed.is_empty() {
            snapshot.name.clone()
        } else {
            typed
        };
        let mut input = api::teams::TeamsUpdateInput::new(snapshot.team_id.clone());
        input.name = Some(name);

        self.saving = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                // FEED-79: the error toast; Retry saves the field again.
                if let Err(err) = result {
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "save",
                        "the team name",
                        crate::action_error::action_error_description(&err),
                        move |_, cx| {
                            let _ = pane.update(cx, |this, cx| this.save(cx));
                        },
                        cx,
                    );
                }
                // Success needs no action: the Electric echo resyncs the
                // snapshot, which clears `dirty`.
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-1105: flip the team's synced `yolo_mode` through `teams.update`
    /// (owner-only server-side). No optimistic flip — the Electric echo moves
    /// the switch.
    fn set_yolo_mode(&mut self, team_id: String, enabled: bool, cx: &mut gpui::Context<Self>) {
        if self.yolo_busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.yolo_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut input = api::teams::TeamsUpdateInput::new(team_id);
                    input.yolo_mode = Some(enabled);
                    api::teams::teams_update(&trpc, &input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.yolo_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] teams.update(yoloMode) failed: {err}");
                    crate::action_error::failed_action_err("update", "yolo mode", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The EXP-288 read-only billing summary: plan chip + usage bars + a
    /// "Manage on the web" hand-off. Returns `None` while loading/failed,
    /// on self-hosted (`unlimited`), or without a synced team slug.
    fn render_billing_section(
        &self,
        team: &domain::rows::Team,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let BillingLoad::Ready(plan) = &self.billing else {
            return None;
        };
        if plan.plan == "unlimited" {
            return None; // self-hosted: no billing surface at all (web parity)
        }
        // Web `PLAN_LABELS` (billing-section.tsx). EXP-771: this still named
        // the retired `pro`/`business` tiers, so the ONE paid tier fell to the
        // fallback arm and the chip read a raw lowercase "team".
        let plan_label: SharedString = match plan.plan.as_str() {
            "free" => "Free".into(),
            "team" => "Team".into(),
            "unlimited" => "Unlimited".into(),
            other => other.to_string().into(),
        };
        let plan_chip = div()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(cx.theme().primary.opacity(0.4))
            .bg(cx.theme().primary.opacity(0.1))
            .text_xs()
            .child(plan_label);

        let fraction = |current: f64, limit: Option<f64>| {
            limit.filter(|limit| *limit > 0.).map(|limit| current / limit)
        };
        let seats = super::usage_bar(
            "Seats",
            format!("{}", plan.usage.members.round() as i64),
            plan.limits
                .seats
                .map(|seats| format!("{}", seats.round() as i64)),
            fraction(plan.usage.members, plan.limits.seats),
            cx,
        );
        let storage = super::usage_bar(
            "Attachment storage",
            super::format_storage(plan.usage.storage_mb),
            plan.limits.storage_mb.map(super::format_storage),
            fraction(plan.usage.storage_mb, plan.limits.storage_mb),
            cx,
        );
        let widgets = super::usage_bar(
            "Feedback widgets",
            format!("{}", plan.usage.widget_configs.round() as i64),
            plan.limits
                .widget_configs
                .map(|widgets| format!("{}", widgets.round() as i64)),
            fraction(plan.usage.widget_configs, plan.limits.widget_configs),
            cx,
        );

        let mut body = section(cx)
            .child(
                crate::surface::glass_section_header(
                    "Plan & Billing",
                    Some(plan_chip.into_any_element()),
                    cx,
                ),
            )
            .child(
                // EXP-862: the usage meters sit in the section's own glass
                // block, not in a bordered card inside a card.
                crate::surface::glass_group()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .child(seats)
                    .child(storage)
                    .child(widgets),
            );

        // Checkout/portal/seat changes are web flows (Creem runs in the
        // browser) — hand off to the team's billing settings page.
        if let Some(slug) = team.slug.clone() {
            if let Some(account) = crate::queries::active_account(cx) {
                let url = format!(
                    "{}/t/{slug}/settings/billing",
                    account.instance_url.trim_end_matches('/')
                );
                body = body.child(
                    h_flex().child(
                        Button::new("billing-manage")
                            .outline()
                            .web_sm()
                            .icon(registry::UI_EXTERNAL_LINK)
                            .label("Manage billing on the web")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                open_url(cx, url.clone());
                            })),
                    ),
                );
            }
        }
        Some(body)
    }

    fn open_delete_dialog(
        &mut self,
        team_id: String,
        team_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.delete_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        cx.notify();
        let pane = cx.entity().downgrade();
        let content_input = self.delete_input.clone();
        let ok_input = self.delete_input.clone();
        let confirm_name = team_name.clone();
        let prompt = format!("Type {team_name} to confirm");
        // The typed-confirm block rides as extra content between the
        // description and the ok/cancel footer.
        let spec = AlertSpec::from_prompt("Delete team", &domain::prompts::delete_team(&team_name))
        // The fixture's slot: the type-the-name field holds focus, and Enter
        // only deletes once the name matches (`on_ok` refuses a mismatch).
        .enter(AlertEnter::Ok)
        .height(gpui::px(340.))
        .content(move |window, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(prompt.clone())),
                )
                .child(glass_input(&content_input, window, cx).web_input_sm())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let typed = ok_input.read(cx).value().trim().to_string();
            if typed != confirm_name {
                // Mismatch keeps the dialog open (web disables the button
                // until it matches).
                return false;
            }
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                log::warn!("[ui] teams.delete skipped: no signed-in account");
                return true;
            };
            let team_id = team_id.clone();
            let pane = pane.clone();
            // Not fire-and-forget like the other Danger Zone mutations: the
            // server REFUSES a team whose subscription is still live (REV2-55,
            // PRECONDITION_FAILED), so the failure has to land on the screen.
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::teams::teams_delete(&trpc, &team_id) })
                    .await;
                let _ = pane.update(cx, |_, cx| {
                    // FEED-79: a refusal (the REV2-55 billing gate above all)
                    // is the error toast, never swallowed.
                    if let Err(err) = &result {
                        log::warn!("[ui] teams.delete failed: {err}");
                        crate::action_error::failed_action(
                            "delete",
                            "the team",
                            Some(team_delete_error_message(err).to_string()),
                            cx,
                        );
                    }
                    // Success needs no action: the Electric echo drops the
                    // team and navigation re-scopes.
                });
            })
            .detach();
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }
}

impl Render for GeneralPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team) = active_team(cx, &self.nav) else {
            return v_flex().child(crate::controls::empty_state_list(
                gpui_component::Icon::new(crate::icons::registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        // Team visibility is deliberately not configurable (v6) — the pane
        // is just the name card, billing summary and Danger Zone.
        let owner = is_owner(cx, &team.id);
        let dirty = self.dirty(cx);
        let saving = self.saving;

        // EXP-818: label left, value right, saves on blur/Enter — the row
        // vocabulary the device editor's Name row wears; a "Saving…" caption
        // trails while the write is out.
        let name_row = crate::surface::glass_input_row(
            "Name",
            crate::surface::glass_row_input(
                glass_input(&self.name_input, window, cx).disabled(!owner),
            )
            .into_any_element(),
            cx,
        )
        .when(saving || dirty, |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if saving { "Saving…" } else { "Unsaved" }),
            )
        });
        let mut general = section(cx)
            .child(crate::surface::glass_section_header("General", None, cx))
            .child(crate::surface::glass_group_rows(vec![name_row]));
        // EXP-1105: yolo mode — owner-only and HIDDEN from everyone else,
        // like every owner control in this pane. Its OWN group under Name,
        // label only (web general-section's second GlassGroup).
        if owner {
            let team_id = team.id.clone();
            general = general.child(crate::surface::glass_group_rows(vec![
                crate::surface::glass_toggle_row(
                    YOLO_MODE_TITLE,
                    None,
                    crate::controls::web_switch("team-yolo-mode")
                        .checked(team.yolo_mode())
                        .disabled(self.yolo_busy)
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            this.set_yolo_mode(team_id.clone(), *checked, cx);
                        }))
                        .into_any_element(),
                    cx,
                ),
            ]));
        }


        let mut pane = v_flex().gap_4().child(general);

        // EXP-1025: the team prompt, right under the name (web parity).
        pane = pane.child(self.render_prompt_section(owner, cx));

        // EXP-288: read-only Plan & Billing between the name card and the
        // Danger Zone (fetch kicked at render like the boards repo cache).
        self.ensure_billing(&team.id, cx);
        if let Some(billing) = self.render_billing_section(&team, cx) {
            pane = pane.child(billing);
        }

        // Danger zone (web settings/general.tsx): owner-only. EXP-720: the
        // shared `danger_zone` recipe, same as Tools' "Reset IDE data".
        if owner {
            let team_id = team.id.clone();
            let team_name = team.name.clone();
            pane = pane.child(
                danger_zone(
                    "Permanently delete this team and all its data.",
                    Button::new("team-delete")
                        .danger()
                        .web_sm()
                        .label("Delete team")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_delete_dialog(
                                team_id.clone(),
                                team_name.clone(),
                                window,
                                cx,
                            );
                        })),
                    cx,
                ),
            );
        }

        pane.when(!owner, |pane| {
            pane.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Only team owners can change these settings."),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Final review B7: one write in flight per team, the newest text
    /// waiting behind it (an older queued one replaced), and a save equal
    /// to what the server holds never sent.
    #[test]
    fn prompt_writes_are_serialized_and_the_last_text_wins() {
        let now = Instant::now();
        let mut queue = TeamPromptQueue::default();
        let (first, text) = queue.ask("a".into()).expect("sent at once");
        assert_eq!(text, "a");
        assert!(queue.ask("ab".into()).is_none(), "waits behind the write out");
        assert!(queue.ask("abc".into()).is_none());
        assert!(queue.busy());
        let (second, text) = queue.settled(first, "a", true, now).expect("the newest text next");
        assert_eq!(text, "abc");
        assert_eq!(queue.settled(second, "abc", true, now), None);
        assert!(!queue.busy());
        assert_eq!(queue.confirmed.as_deref(), Some("abc"));
        // Nothing new to say: equal (trailing whitespace aside) to the saved.
        assert_eq!(queue.ask("abc \n".into()), None);
    }

    /// Final review B7: a failed save's Retry sends the team's CURRENT
    /// text, not the snapshot that failed — and nothing once a later save
    /// already holds it.
    #[test]
    fn a_failed_prompt_save_retries_the_current_text() {
        let now = Instant::now();
        let mut queue = TeamPromptQueue::default();
        let (first, _) = queue.ask("abc".into()).expect("sent");
        assert!(queue.ask("abcd".into()).is_none());
        // "abc" fails, "abcd" goes out next and lands.
        let (second, text) = queue.settled(first, "abc", false, now).expect("next");
        assert_eq!(text, "abcd");
        assert_eq!(queue.settled(second, "abcd", true, now), None);
        assert_eq!(queue.retry_text(), None, "the current text is already saved");
        // A later edit that fails: the Retry sends THAT text.
        let (third, _) = queue.ask("abcde".into()).expect("sent");
        assert_eq!(queue.settled(third, "abcde", false, now), None);
        assert_eq!(queue.retry_text().as_deref(), Some("abcde"));
    }

    /// FEED-79: the byte cap's inline line is the fixture's
    /// "a prompt over its byte cap stays under its field" message.
    #[test]
    fn the_byte_cap_line_is_the_fixture_copy() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/feedback-states.json"
        ))
        .expect("feedback-states.json parses");
        let case = fixture["errors"]["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|case| case["name"] == "a prompt over its byte cap stays under its field")
            .expect("the byte cap case");
        let max = domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES;
        let want = case["message"]
            .as_str()
            .expect("message")
            .replace("{used}", &(max + 1).to_string())
            .replace("{max}", &max.to_string());
        assert_eq!(prompt_too_long(max + 1), want);
    }
}
