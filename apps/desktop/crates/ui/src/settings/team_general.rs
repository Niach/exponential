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

use std::rc::Rc;
use std::time::Instant;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window,
};
use gpui_component::{input::InputState, v_flex, ActiveTheme as _};
use serde_json::{json, Value};
use sync::Store;

use crate::controls::{glass_input, WebControl as _};
use crate::feedback;
use crate::toast::{self, Toast};
use crate::native_dialog::{self, AlertEnter, AlertSpec};
use crate::navigation::Navigation;

use super::doc::{and, faded, hex, not, path, IdeToasts, Ink, SM, VALUE_ALPHA, XS};
use super::{active_team, is_owner, open_url, team_delete_error_message};

/// #EXP-1645: why a picked file never goes out (the server's own words,
/// `team-icon-upload.ts`). The native picker has no type filter, so `upload.accept`
/// runs on the result.
fn team_image_refusal(content_type: &str, len: usize) -> Option<String> {
    let upload = &domain::team_image::TEAM_IMAGE.upload;
    if !upload.accept.iter().any(|accepted| accepted == content_type) {
        return Some("Team images must be PNG, JPEG, WebP, GIF or SVG".to_string());
    }
    (len > upload.max_bytes)
        .then(|| format!("Team images must be {} MB or smaller", upload.max_bytes / (1024 * 1024)))
}

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
    /// The text the field was last given (the load): the document's bound
    /// value. A save that lands never moves it, so a slow confirmation can
    /// never write an older text over the field.
    shown: String,
    /// The field's live text (its edits).
    text: String,
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

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct GeneralShot {
    /// `None` = no team selected.
    pub team: Option<domain::rows::Team>,
    pub owner: bool,
    /// The team image's PNG bytes (its `icon_url` names it).
    pub picture: Option<Vec<u8>>,
    /// The Name field's typed text (differs from the team's = `Unsaved`).
    pub name_draft: Option<String>,
    pub saving: bool,
    pub prompt: PromptShot,
    pub billing: Option<api::billing::TeamPlanOut>,
}

#[cfg(test)]
pub(crate) enum PromptShot {
    Loading,
    Failed,
    /// The text, its last-write stamp, and whether a write is out.
    Ready(String, Option<String>, bool),
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The `Device time zone` option's value (a Select option needs one; the
/// write sends `null`).
const DEVICE_ZONE: &str = "device";

/// #EXP-1629: Settings → General is ONE document on eUI's native host
/// binding: the pane keeps its state and writes, [`GeneralPane::project`] is
/// its data model at `/pane`, [`document`] the tree (the Name `row` Input,
/// the team image row, the Time zone and Merge `row` Selects, the team
/// prompt's Textarea, Plan & Billing, the Danger zone); every press, pick and
/// edit comes back to the pane.
pub struct GeneralPane {
    nav: Entity<Navigation>,
    delete_input: Entity<InputState>,
    /// The Name field's live text.
    name: String,
    /// EXP-1025: the team prompt — a RAW monospace field, never the markdown
    /// WYSIWYG: a prompt is read by a model, so the bytes shown must be the
    /// bytes it gets.
    prompt: PromptState,
    snapshot: Option<Snapshot>,
    saving: bool,
    /// #EXP-1710: a merge-mode `teams.update` is in flight (row disabled).
    merge_busy: bool,
    /// VAPP-201: a time-zone `teams.update` is in flight (picker disabled).
    zone_busy: bool,
    /// #EXP-1645: a team image upload or remove is out (row disabled).
    image_busy: bool,
    /// EXP-288: the read-only plan/usage summary between the name card and
    /// the Danger Zone. Refetched on team/account change; hidden while
    /// loading/failed and entirely on self-hosted (`plan == "unlimited"`).
    billing: BillingLoad,
    billing_team: Option<String>,
    billing_account: Option<String>,
    billing_generation: u64,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state (headless has no team, membership or server).
    #[cfg(test)]
    shot: Option<GeneralShot>,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let delete_input = cx.new(|cx| InputState::new(window, cx));

        let collections = Store::global(cx).collections().clone();
        let mut subscriptions = vec![
            cx.observe(&nav, |this, _, cx| this.resync(cx)),
            cx.observe(&collections.teams, |this, _, cx| this.resync(cx)),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];
        // Review #8: leaving Settings (or closing the window) drops the pane
        // — an edit still in its debounce is sent on the way out.
        cx.on_release(|this, cx| this.flush_prompt_on_release(cx)).detach();
        // Final review B7: the writes run in the app-wide queue; the pane
        // follows it for `Saving` / `Saved`.
        prompt_saves(cx);
        subscriptions.push(cx.observe_global::<PromptSaves>(|this, cx| this.sync_prompt_saves(cx)));

        // Every action leaves the binding for the pane (the pickers, the
        // delete confirm and the file picker open on its window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["zone", "merge", "upload", "removeImage", "retryPrompt", "manageBilling", "deleteTeam"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).cloned().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        // The two text fields' edits (live and committed) reach the pane
        // through its plugin: the binding takes only commits.
        let plugin: Rc<dyn HostPlugin> = Rc::new(PanePlugin { pane: cx.weak_entity(), inner: super::doc::ide_plugin(), media: crate::plan_card::CardSurfaceHost::new(cx) });
        let view = super::doc::mount_with("settings-general", document(&Ink::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);

        let mut this = Self {
            nav,
            delete_input,
            name: String::new(),
            prompt: PromptState::default(),
            snapshot: None,
            saving: false,
            merge_busy: false,
            zone_busy: false,
            image_busy: false,
            billing: BillingLoad::Idle,
            billing_team: None,
            billing_account: None,
            billing_generation: 0,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        };
        this.resync(cx);
        this
    }

    /// The pane's team (the bench's, under test).
    fn team(&self, cx: &App) -> Option<domain::rows::Team> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team(cx, &self.nav)
    }

    fn owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        is_owner(cx, team_id)
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: GeneralShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        use base64::Engine as _;
        let mut shot = shot;
        // The picture rides a data URL (headless has no transport).
        if let (Some(team), Some(bytes)) = (shot.team.as_mut(), &shot.picture) {
            if team.icon_url.is_some() {
                team.icon_url = Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
            }
        }
        if let Some(team) = &shot.team {
            self.snapshot = Some(Snapshot { team_id: team.id.clone(), name: team.name.clone() });
            self.name = shot.name_draft.clone().unwrap_or_else(|| team.name.clone());
            self.prompt.team_id = Some(team.id.clone());
            self.billing_team = Some(team.id.clone());
        }
        self.saving = shot.saving;
        self.billing = match &shot.billing {
            Some(plan) => BillingLoad::Ready(plan.clone()),
            None => BillingLoad::Failed,
        };
        match &shot.prompt {
            PromptShot::Loading => self.prompt.loading = true,
            PromptShot::Failed => self.prompt.load_failed = true,
            PromptShot::Ready(text, stamp, busy) => {
                self.prompt.saved = text.trim_end().to_string();
                self.prompt.shown = text.clone();
                self.prompt.text = text.clone();
                self.prompt.updated_at = stamp.clone();
                if let (true, Some(team)) = (*busy, &shot.team) {
                    prompt_saves(cx).teams.entry(team.id.clone()).or_default().queue.in_flight = Some(1);
                }
            }
        }
        self.shot = Some(shot);
        self.push(cx);
        cx.notify();
    }

    /// Kick the `billing.teamPlan` fetch when the pane is shown or the
    /// team/account changed (render-time — a hidden pane never fetches;
    /// generation-guarded like the boards repo cache).
    fn ensure_billing(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
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
    fn resync(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team) = self.team(cx) else {
            return;
        };
        let snapshot = Snapshot {
            team_id: team.id.clone(),
            name: team.name.clone(),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name = snapshot.name.clone();
        let team_changed = self.snapshot.as_ref().map(|held| &held.team_id) != Some(&snapshot.team_id);
        self.snapshot = Some(snapshot);
        if team_changed {
            self.fetch_prompt(cx);
        }
        cx.notify();
    }

    /// EXP-1025: the ONE read path for the team prompt (`teams.getAgentPrompt`;
    /// the shape excludes it). Clears the field first so the previous team's
    /// text never shows under the new team's name.
    fn fetch_prompt(&mut self, cx: &mut gpui::Context<Self>) {
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
        self.prompt.shown.clear();
        self.prompt.text.clear();
        self.prompt.updated_at = None;
        self.prompt.saved_at = None;
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            self.prompt.loading = false;
            self.prompt.load_failed = true;
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_get_agent_prompt(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.prompt.generation != generation {
                    return;
                }
                view.prompt.loading = false;
                let attended = std::mem::take(&mut view.prompt.retry_pressed);
                match result {
                    Ok(prompt) => {
                        view.prompt.saved = prompt.agent_prompt.clone();
                        view.prompt.updated_at = prompt.agent_prompt_updated_at;
                        view.prompt.shown = prompt.agent_prompt.clone();
                        view.prompt.text = prompt.agent_prompt;
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
    fn retry_prompt(&mut self, cx: &mut gpui::Context<Self>) {
        self.prompt.retry_pressed = true;
        self.fetch_prompt(cx);
        cx.notify();
    }

    fn prompt_bytes(&self) -> usize {
        self.prompt.text.len()
    }

    fn prompt_dirty(&self) -> bool {
        !self.prompt.loading
            && !self.prompt.load_failed
            && self.prompt.text.trim_end() != self.prompt.saved
    }

    /// EXP-1275: (re)start the autosave debounce: the save fires
    /// `AUTOSAVE_DEBOUNCE` after the LAST keystroke.
    fn schedule_prompt_save(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.prompt_dirty() {
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
    fn pending_prompt_save(&self, _cx: &App) -> Option<PromptSave> {
        let team_id = self.prompt.team_id.clone()?;
        if !self.prompt_dirty() {
            return None;
        }
        if self.prompt_bytes() > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES {
            return None;
        }
        Some(PromptSave {
            team_id,
            text: self.prompt.text.clone(),
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

    /// A text field's edit (the plugin's): `name` saves on commit (blur /
    /// Enter, EXP-818: no Save button); the prompt AUTOSAVES
    /// `AUTOSAVE_DEBOUNCE` after the last keystroke and on blur (EXP-1275).
    fn edited(&mut self, name: &str, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        match name {
            "name" => {
                self.name = value;
                if commit {
                    self.save(cx);
                }
                cx.notify();
            }
            "prompt" => {
                self.prompt.text = value;
                if commit {
                    self.prompt.debounce = None;
                    self.save_prompt(cx);
                } else {
                    self.schedule_prompt_save(cx);
                }
                cx.notify();
            }
            _ => {}
        }
    }

    /// A document action.
    fn action(&mut self, name: &str, value: Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team) = self.team(cx) else { return };
        let owner = self.owner(&team.id, cx);
        match name {
            "zone" if owner => {
                let current = team.time_zone.clone().unwrap_or_default();
                let picked = value.as_str().unwrap_or_default();
                let zone = if picked == DEVICE_ZONE { String::new() } else { picked.to_string() };
                if !picked.is_empty() && zone != current {
                    self.set_time_zone(team.id.clone(), zone, cx);
                }
            }
            "merge" if owner => {
                if let Some(mode) = value.as_str().filter(|mode| !mode.is_empty() && *mode != team.merge_mode()) {
                    self.set_merge_mode(team.id.clone(), mode.to_string(), cx);
                }
            }
            "upload" if owner => self.pick_team_image(team.id.clone(), window, cx),
            "removeImage" if owner => self.remove_team_image(team.id.clone(), cx),
            "retryPrompt" => self.retry_prompt(cx),
            "manageBilling" => {
                if let Some(url) = billing_url(&team, cx) {
                    open_url(cx, url);
                }
            }
            "deleteTeam" if owner => self.open_delete_dialog(team.id.clone(), team.name.clone(), window, cx),
            _ => {}
        }
    }

    fn dirty(&self) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name != snapshot.name
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        if !self.dirty() || self.saving {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        let typed = self.name.trim().to_string();
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

    /// VAPP-201: write the team zone (`""` = none, the device's) through
    /// `teams.update`; the Electric echo moves the row, a failure toasts.
    fn set_time_zone(&mut self, team_id: String, zone: String, cx: &mut gpui::Context<Self>) {
        if self.zone_busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the time zone", None, cx);
            return;
        };
        self.zone_busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut input = api::teams::TeamsUpdateInput::new(team_id);
                    input.time_zone = if zone.is_empty() { api::Patch::Null } else { api::Patch::Set(zone) };
                    api::teams::teams_update(&trpc, &input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.zone_busy = false;
                if let Err(err) = result {
                    crate::action_error::failed_action_err("save", "the time zone", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The Upload pill: the native picker, one file.
    fn pick_team_image(&mut self, team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.image_busy {
            return;
        }
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(domain::team_image::TEAM_IMAGE.copy.upload.clone().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            // Receiver error = dialog dismissed/unsupported; None = cancelled.
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| this.upload_team_image(team_id, path, cx));
        })
        .detach();
    }

    /// One multipart `file` to `upload.path`; the Electric echo delivers the
    /// new url, a failure toasts with a Retry that reads the file again.
    fn upload_team_image(&mut self, team_id: String, path: std::path::PathBuf, cx: &mut gpui::Context<Self>) {
        if self.image_busy {
            return;
        }
        let Some(transport) = crate::queries::http_attachment_transport(cx) else {
            crate::action_error::failed_action("upload", "the team image", None, cx);
            return;
        };
        self.image_busy = true;
        cx.notify();
        let (send_team, send_path) = (team_id.clone(), path.clone());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (filename, content_type, bytes) = crate::markdown::image_paste::read_any_file(&send_path)?;
                    if let Some(refusal) = team_image_refusal(&content_type, bytes.len()) {
                        anyhow::bail!(refusal);
                    }
                    transport.upload_team_image(&send_team, &filename, &content_type, &bytes)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.image_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] team image upload failed: {err}");
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "upload",
                        "the team image",
                        crate::action_error::description_of(&err.to_string()),
                        move |_, cx| {
                            let _ = pane.update(cx, |this, cx| {
                                this.upload_team_image(team_id.clone(), path.clone(), cx);
                            });
                        },
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Remove: no confirm, `teams.update {iconUrl: null}`; the echo moves the row.
    fn remove_team_image(&mut self, team_id: String, cx: &mut gpui::Context<Self>) {
        if self.image_busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("remove", "the team image", None, cx);
            return;
        };
        self.image_busy = true;
        cx.notify();
        let send_team = team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut input = api::teams::TeamsUpdateInput::new(send_team);
                    input.icon_url = api::Patch::Null;
                    api::teams::teams_update(&trpc, &input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.image_busy = false;
                if let Err(err) = result {
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "remove",
                        "the team image",
                        crate::action_error::action_error_description(&err),
                        move |_, cx| {
                            let _ = pane.update(cx, |this, cx| this.remove_team_image(team_id.clone(), cx));
                        },
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #EXP-1710: write the team's `merge_mode` through `teams.update`
    /// (owner-only server-side). No optimistic flip: the Electric echo moves
    /// the row; a failure toasts with Retry.
    fn set_merge_mode(&mut self, team_id: String, mode: String, cx: &mut gpui::Context<Self>) {
        if self.merge_busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the merge mode", None, cx);
            return;
        };
        self.merge_busy = true;
        cx.notify();
        let view = cx.entity().downgrade();
        cx.spawn(async move |this, cx| {
            let (id, value) = (team_id.clone(), mode.clone());
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut input = api::teams::TeamsUpdateInput::new(id);
                    input.merge_mode = Some(value);
                    api::teams::teams_update(&trpc, &input)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.merge_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] teams.update(mergeMode) failed: {err}");
                    crate::action_error::failed_action_with_retry(
                        "save",
                        "the merge mode",
                        crate::action_error::action_error_description(&err),
                        move |_, cx| {
                            let (team_id, mode) = (team_id.clone(), mode.clone());
                            let _ = view.update(cx, |this, cx| this.set_merge_mode(team_id, mode, cx));
                        },
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
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

    /// EXP-1025: the team prompt section's state — the header's caption
    /// (`Loading` while it loads, D8; `Saving` (spinner) → `Saved` for the
    /// hold → `Edited {ago}`, EXP-1275: no Unsaved state), the field, its
    /// byte/token counter and the cap's inline line (FEED-79: invalid input).
    fn project_prompt(&self, cx: &App) -> Value {
        let bytes = self.prompt_bytes();
        let over = bytes > domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES;
        let just_saved = self.prompt.saved_at.is_some_and(|at| at.elapsed() < feedback::AUTOSAVE_SAVED_HOLD);
        let (status, spinning) = if self.prompt.loading {
            (Some(feedback::WORD_LOADING.to_string()), true)
        } else if self.prompt_saving(cx) {
            (Some(feedback::AUTOSAVE_SAVING.to_string()), true)
        } else if just_saved {
            (Some(feedback::AUTOSAVE_SAVED.to_string()), false)
        } else {
            (self.prompt.updated_at.as_deref().map(|stamp| feedback::autosave_edited(&crate::inbox::relative_time(stamp)).to_string()), false)
        };
        let failed = self.prompt.load_failed && !self.prompt.loading;
        let loaded = !self.prompt.loading && !self.prompt.load_failed;
        json!({
            "failed": failed,
            "ready": !failed,
            "value": if loaded { self.prompt.shown.as_str() } else { "" },
            "placeholder": if loaded { TEAM_PROMPT_PLACEHOLDER } else { "" },
            "disabled": !loaded,
            "hasStatus": status.is_some(),
            "spinning": spinning,
            "status": status.unwrap_or_default(),
            "counter": prompt_counter(bytes),
            "over": over,
            "under": !over,
            "error": if over { prompt_too_long(bytes) } else { String::new() },
        })
    }

    /// The EXP-288 read-only billing summary: plan chip + usage bars + a
    /// "Manage on the web" hand-off. Hidden while loading/failed and on
    /// self-hosted (`unlimited`).
    fn project_billing(&self, team: &domain::rows::Team, cx: &App) -> Value {
        let BillingLoad::Ready(plan) = &self.billing else {
            return json!({"show": false});
        };
        if plan.plan == "unlimited" {
            return json!({"show": false}); // self-hosted: no billing surface at all (web parity)
        }
        // Web `PLAN_LABELS` (billing-section.tsx). EXP-771: the ONE paid
        // tier is `team`.
        let label = match plan.plan.as_str() {
            "free" => "Free".to_string(),
            "team" => "Team".to_string(),
            other => other.to_string(),
        };
        let usage = |current: String, limit: Option<String>, fraction: Option<f64>| {
            let amount = match limit {
                Some(limit) => format!("{current} / {limit}"),
                None => format!("{current} / unlimited"),
            };
            json!({"amount": amount, "track": fraction.is_some(), "width": format!("{}%", fraction.unwrap_or(0.).clamp(0., 1.) * 100.)})
        };
        let fraction = |current: f64, limit: Option<f64>| limit.filter(|limit| *limit > 0.).map(|limit| current / limit);
        let count = |value: f64| format!("{}", value.round() as i64);
        json!({
            "show": true,
            "plan": label,
            "seats": usage(count(plan.usage.members), plan.limits.seats.map(count), fraction(plan.usage.members, plan.limits.seats)),
            "storage": usage(super::format_storage(plan.usage.storage_mb), plan.limits.storage_mb.map(super::format_storage), fraction(plan.usage.storage_mb, plan.limits.storage_mb)),
            "widgets": usage(count(plan.usage.widget_configs), plan.limits.widget_configs.map(count), fraction(plan.usage.widget_configs, plan.limits.widget_configs)),
            // Checkout/portal/seat changes are web flows (Creem runs in the
            // browser) — hand off to the team's billing settings page.
            "manage": billing_url(team, cx).is_some(),
        })
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let Some(team) = self.team(cx) else {
            return json!({"hasTeam": false});
        };
        let owner = self.owner(&team.id, cx);
        let caption = if self.saving { Some("Saving…") } else { self.dirty().then_some("Unsaved") };
        let zone_copy = &domain::team_zone::TEAM_ZONE.copy;
        let zone = team.time_zone.clone().unwrap_or_default();
        let mut zones = vec![json!({"label": zone_copy.device, "value": DEVICE_ZONE})];
        zones.extend(crate::schedule_fields::time_zone_items(&zone).into_iter().map(|item| json!({"label": item.label.to_string(), "value": item.value})));
        let modes: Vec<Value> = domain::contract::TEAM_MERGE_MODE_VALUES
            .iter()
            .map(|mode| json!({"label": domain::merge_train::merge_mode_label(mode), "value": mode}))
            .collect();
        let image = match domain::team_image::team_mark(team.icon_url.as_deref()) {
            domain::team_image::TeamMark::Image { src } => Some(src),
            domain::team_image::TeamMark::Glyph => None,
        };
        json!({
            "hasTeam": true,
            "owner": owner,
            "member": !owner,
            "name": self.shown_name(),
                        "hasCaption": caption.is_some(),
            "caption": caption.unwrap_or_default(),
            "image": {"has": image.is_some(), "none": image.is_none(), "src": image.unwrap_or_default(), "disabled": !owner || self.image_busy},
            "zone": {"label": if zone.is_empty() { zone_copy.device.clone() } else { zone.clone() }, "value": if zone.is_empty() { DEVICE_ZONE.to_string() } else { zone }, "options": zones, "disabled": !owner || self.zone_busy},
            "merge": {"label": domain::merge_train::merge_mode_label(team.merge_mode()), "value": team.merge_mode(), "options": modes, "disabled": !owner || self.merge_busy},
            "prompt": self.project_prompt(cx),
            "billing": self.project_billing(&team, cx),
        })
    }

    /// The Name field's bound value: the synced name (the bench's typed
    /// draft, under test).
    fn shown_name(&self) -> String {
        #[cfg(test)]
        if let Some(draft) = self.shot.as_ref().and_then(|shot| shot.name_draft.clone()) {
            return draft;
        }
        self.snapshot.as_ref().map(|snapshot| snapshot.name.clone()).unwrap_or_default()
    }

    /// Write the projection now (the bench's state lands before a frame).
    fn push(&mut self, cx: &mut gpui::Context<Self>) {
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
    }
}

/// The team's billing settings page on the web.
fn billing_url(team: &domain::rows::Team, cx: &App) -> Option<String> {
    let slug = team.slug.clone()?;
    let account = crate::queries::active_account(cx)?;
    Some(format!("{}/t/{slug}/settings/billing", account.instance_url.trim_end_matches('/')))
}

impl Render for GeneralPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "GeneralPane").entered();
        // EXP-288: the plan read is kicked at render like the boards repo cache.
        if let Some(team) = self.team(cx) {
            self.ensure_billing(&team.id, cx);
        }
        self.push(cx);
        // An ordinary section pane — `SettingsView` owns the scroll
        // container and the shared `detail_column` grid.
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The pane's plugin: the IDE's, the team image through the card host's
/// media rule (the instance's `/api/attachments/*` with the account's
/// bearer, or https), and the two text fields' edits for the pane.
struct PanePlugin {
    pane: gpui::WeakEntity<GeneralPane>,
    inner: crate::exponential_ui_host::IdePlugin,
    media: crate::plan_card::CardSurfaceHost,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn media_allowed(&self, src: &str) -> bool {
        crate::plan_card::card_media_allowed(src) || cfg!(test) && src.starts_with("data:image/png;base64,")
    }

    fn media_options(&self) -> eui_core::host::MediaOptions {
        // The bench's picture is a data URL (headless has no transport).
        #[cfg(test)]
        return Default::default();
        #[cfg(not(test))]
        self.media.media_options()
    }

    fn media_request(&self, src: &str) -> Option<eui_core::host::MediaRequest> {
        #[cfg(test)]
        if src.starts_with("data:image/png;base64,") {
            return eui_core::host::media_request(src, &Default::default());
        }
        self.media.media_request(src)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut App) {
        let (name, commit) = (event.name.clone(), event.kind == eui_gpui::host::InputKind::Commit);
        let value = event.value.as_str().unwrap_or_default().to_string();
        let pane = self.pane.clone();
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(&name, value, commit, cx));
        });
    }
}

/// The colours the document needs beyond [`Ink`].
struct Chrome {
    radius: f32,
    danger: String,
    danger_button: String,
    danger_button_fg: String,
    danger_button_hover: String,
    /// The usage meters' track (`muted` at 35%).
    track: String,
    /// The plan chip: a 40% primary stroke over a 10% primary wash.
    chip_stroke: String,
    chip_fill: String,
    /// The outline `Button` (the input wash and stroke).
    button_fg: String,
    button_fill: String,
    button_stroke: String,
    /// The Remove ghost's glyph.
    muted: String,
    list_hover: String,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            danger_button: hex(theme.button_danger),
            danger_button_fg: hex(theme.button_danger_foreground),
            danger_button_hover: hex(theme.button_danger_hover),
            track: hex(theme.muted.opacity(0.35)),
            chip_stroke: premultiplied_hex(theme.primary.opacity(0.4)),
            // The 10% wash on the byte gpui lands on (25, not 25.5 rounded up).
            chip_fill: hex(theme.primary.opacity(25. / 255.)),
            button_fg: hex(theme.button_foreground),
            button_fill: premultiplied_hex(theme.input_background()),
            button_stroke: hex(theme.input),
            muted: hex(theme.muted_foreground),
            list_hover: hex(theme.list_hover),
        }
    }
}

/// `#rrggbbaa` of a faint translucent colour that paints like the exact one
/// (the alpha rounds to its byte, the channels absorb the rounding).
fn premultiplied_hex(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let alpha = (c.a.clamp(0., 1.) * 255.).round().max(1.);
    let channel = |v: f32| ((v * c.a * 255. / alpha) * 255.).round().clamp(0., 255.) as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", channel(c.r), channel(c.g), channel(c.b), alpha as u8)
}

/// A column (`section`: `gap_2`).
fn column(id: &str, gap: f32, visible: Value, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": gap, "width": "100%", "minWidth": 0}, "children": children})
}

/// EXP-818: the Name row: label left, value right, saved on blur / Enter
/// (the `row` Input), a `Saving…` / `Unsaved` caption trailing while the
/// write is out or the text differs.
fn name_row(ink: &Ink) -> Value {
    let input = json!({"id": "name", "component": "Input", "props": {"variant": "row", "label": "Name", "name": "name", "placeholder": "Team name", "value": path("/pane/name")},
        "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "width": 0, "minWidth": 0}});
    // The field's own 14 px end padding stays; the caption sits `gap_3`
    // after the value, the row's `px_4` after it.
    let mut caption = ink.text("name_caption", path("/pane/caption"), XS, &ink.muted);
    caption["visible"] = and(path("/pane/owner"), path("/pane/hasCaption"));
    caption["style"]["flexShrink"] = json!(0);
    caption["style"]["marginLeft"] = json!(ink.r(0.75) - ink.r(1.));
    caption["style"]["marginRight"] = json!(ink.r(1.));
    let field = json!({"id": "name_row", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0}, "children": [input, caption]});
    // A member's field is the same row Input, disabled (today's is a
    // disabled Input too); a disabled field dims whole, so its label is
    // drawn beside it at full ink (today's label never dims).
    let mut label = ink.text("name_member_label", json!("Name"), SM, &ink.foreground);
    label["style"]["flexShrink"] = json!(0);
    label["style"]["paddingLeft"] = json!(ink.r(1.));
    let disabled = json!({"id": "name_member", "component": "Input", "props": {"variant": "row", "label": "", "name": "name_member", "value": path("/pane/name"), "disabled": true},
        "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "width": 0, "minWidth": 0}});
    let member = json!({"id": "name_member_row", "component": "Box", "visible": path("/pane/member"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0}, "children": [label, disabled]});
    json!({"id": "name_rows", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [field, member]})
}

/// A disabled picker row: the label at full ink, the value and its chevron
/// at half the value's 70% (the picker's ghost `Button` run: `text_sm` on
/// its 1.25 rem line).
fn read_only_row(ink: &Ink, id: &str, label: &str, value: Value, visible: Value) -> Value {
    let mut text = faded(ink.text(&format!("{id}_value"), value, SM, &ink.value), VALUE_ALPHA * 0.5);
    text["props"]["lines"] = json!(1);
    text["style"]["minWidth"] = json!(0);
    text["style"]["flexShrink"] = json!(1);
    text["style"]["lineHeight"] = json!(ink.r(1.25));
    let glyph = ink.r(1.);
    let chevron = json!({"id": format!("{id}_chevron"), "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": VALUE_ALPHA * 0.5, "flexShrink": 0}});
    let control = json!({"id": format!("{id}_control"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "flex-end", "gap": ink.r(0.5), "flexGrow": 1, "minWidth": 0}, "children": [text, chevron]});
    let label = json!({"id": format!("{id}_text"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground)]});
    ink.shell(id, false, visible, vec![label, control])
}

/// #EXP-1645 (team-image.json): the label, the 28 px mark, the Upload pill,
/// then, only with an image, the icon-only Remove ghost.
fn image_row(ink: &Ink, chrome: &Chrome) -> Value {
    let spec = &*domain::team_image::TEAM_IMAGE;
    let side = spec.row.mark_size;
    let mut label = ink.text("image_label", json!(spec.copy.row), SM, &ink.foreground);
    label["style"]["flexGrow"] = json!(1);
    label["style"]["flexShrink"] = json!(1);
    label["style"]["minWidth"] = json!(0);
    let mark = json!({"id": "image_mark", "component": "Box", "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": side, "height": side, "overflow": "hidden"},
        "children": [
            {"id": "image_picture", "component": "Image", "visible": path("/pane/image/has"), "props": {"src": path("/pane/image/src"), "alt": "", "fit": "cover", "fallback": spec.mark.glyph},
                "style": {"width": side, "height": side, "borderRadius": side * spec.mark.radius_fraction}},
            {"id": "image_glyph", "component": "Icon", "visible": path("/pane/image/none"), "props": {"name": spec.mark.glyph, "size": "xl"}, "style": {"width": side, "height": side, "color": ink.foreground}},
        ]});
    let mut upload = ink.pill_button("image_upload", None, json!(spec.copy.upload), path("/pane/image/disabled"), json!({"name": "upload"}));
    // gpui centres the label's line half a point lower than the flex centre.
    for pill in upload.iter_mut() {
        pill["children"][0]["style"]["paddingTop"] = json!(1);
    }
    let remove = |dim: bool| {
        let id = if dim { "image_remove_dim" } else { "image_remove" };
        let visible = if dim { and(path("/pane/image/has"), path("/pane/image/disabled")) } else { and(path("/pane/image/has"), not(path("/pane/image/disabled"))) };
        let mut button = json!({"id": id, "component": "Box", "visible": visible, "accessibility": {"label": spec.copy.remove},
            "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": side, "height": side, "borderRadius": chrome.radius},
            "children": [{"id": format!("{id}_icon"), "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": 12, "height": 12, "color": chrome.muted}}]});
        if !dim {
            button["props"] = json!({"pressable": true});
            button["on"] = json!({"press": {"event": {"name": "removeImage"}}});
            button["style"]["cursor"] = json!("pointer");
            button["style"][":hover"] = json!({"backgroundColor": chrome.list_hover});
            return json!({"id": "image_remove_tip", "component": "Tooltip", "visible": and(path("/pane/image/has"), not(path("/pane/image/disabled"))), "props": {"content": spec.copy.remove}, "children": [button]});
        }
        button
    };
    let mut children = vec![label, mark];
    children.extend(upload);
    children.push(remove(false));
    children.push(remove(true));
    ink.shell("image_row", false, json!(true), children)
}

/// VAPP-201 / #EXP-1710: a `row` Select (the Time zone with `Device time
/// zone` first, searchable; the Merge mode).
fn select_row(ink: &Ink, id: &str, label: &str, searchable: bool, action: &str) -> Value {
    let disabled = path(&format!("/pane/{id}/disabled"));
    let select = json!({"id": id, "component": "Select", "visible": not(disabled.clone()), "props": {"label": label, "name": id, "triggerVariant": "row", "searchable": searchable,
            "value": path(&format!("/pane/{id}/value")), "options": path(&format!("/pane/{id}/options"))},
        "on": {"change": {"event": {"name": action}}}});
    // Not an owner, or the write is out: the picker reads its value, dimmed.
    let read_only = read_only_row(ink, &format!("{id}_read"), label, path(&format!("/pane/{id}/label")), disabled);
    json!({"id": format!("{id}_rows"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [select, read_only]})
}

/// The General band over its group (Name, Team image, Time zone), then the
/// owner's Merge row in its own group (web general-section's second
/// GlassGroup).
fn general(ink: &Ink, chrome: &Chrome) -> Value {
    let rows = ink.group("general_group", vec![
        name_row(ink),
        ink.divider("image_divider", json!(true)),
        image_row(ink, chrome),
        ink.divider("zone_divider", json!(true)),
        select_row(ink, "zone", &domain::team_zone::TEAM_ZONE.copy.row, true, "zone"),
    ]);
    let mut merge = ink.group("merge_group", vec![select_row(ink, "merge", &domain::merge_train::MERGE_TRAIN.copy.settings.label, false, "merge")]);
    merge["visible"] = path("/pane/owner");
    column("general", ink.r(0.5), json!(true), vec![ink.band("general_band", "General", None), rows, merge])
}

/// EXP-1025: the team prompt section (owners only): the band with the
/// save caption, the raw monospace field and its counter footer in one
/// glass block, the cap's line under it; a failed load = the empty state +
/// Retry.
fn prompt(ink: &Ink, chrome: &Chrome) -> Value {
    // `feedback::inline_status`: the spinner + the muted `text_xs` word.
    let mut word = ink.text("prompt_status_word", path("/pane/prompt/status"), XS, &ink.muted);
    word["style"]["flexShrink"] = json!(0);
    let spinner = eui_gpui::controls::INLINE_SPINNER_PX;
    let status = json!({"id": "prompt_status", "component": "Box", "visible": path("/pane/prompt/hasStatus"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "gap": crate::feedback::INLINE_GAP_PX},
        "children": [
            {"id": "prompt_status_spinner", "component": "Spinner", "visible": path("/pane/prompt/spinning"), "style": {"width": spinner, "height": spinner, "color": ink.muted, "flexShrink": 0}},
            word,
        ]});
    // The raw monospace field (`variant: code`: chromeless in the group,
    // the code type), 10 rows growing to 40.
    let field = json!({"id": "prompt", "component": "Textarea", "props": {"label": TEAM_PROMPT_TITLE, "name": "prompt", "variant": "code", "rows": 10, "autosize": true, "maxRows": 40,
            "value": path("/pane/prompt/value"), "placeholder": path("/pane/prompt/placeholder"), "disabled": path("/pane/prompt/disabled")},
        "style": {"width": "100%"}});
    let counter = |id: &str, color: &str, visible: Value| {
        let mut text = ink.text(id, path("/pane/prompt/counter"), XS, color);
        text["visible"] = visible;
        text["style"]["flexShrink"] = json!(0);
        text
    };
    let footer = json!({"id": "prompt_footer", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "justifyContent": "flex-end", "width": "100%", "gap": ink.r(1.), "paddingHorizontal": ink.r(1.), "paddingVertical": ink.r(0.5), "borderTopWidth": 1, "borderColor": ink.stroke_row},
        "children": [counter("prompt_counter", &ink.muted, path("/pane/prompt/under")), counter("prompt_counter_over", &chrome.danger, path("/pane/prompt/over"))]});
    // FEED-79: `action_error::field_error`, 13 px on a 20 px line.
    let error = json!({"id": "prompt_error", "component": "Text", "visible": path("/pane/prompt/over"), "props": {"text": path("/pane/prompt/error")},
        "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger}});
    let ready = column("prompt_ready", ink.r(0.5), path("/pane/prompt/ready"), vec![
        ink.band(&"prompt_band".to_string(), TEAM_PROMPT_TITLE, Some(status)),
        ink.group("prompt_group", vec![field, footer]),
        error,
    ]);
    let failed = column("prompt_failed", ink.r(0.5), path("/pane/prompt/failed"), vec![
        ink.band("prompt_failed_band", TEAM_PROMPT_TITLE, None),
        ink.group("prompt_failed_group", vec![super::doc::failed_state(ink, "prompt_retry", "settings-general", PROMPT_UNAVAILABLE, json!(true))]),
    ]);
    // The failed state's Retry is the pane's own action.
    let mut failed = failed;
    rename_event(&mut failed, "retry", "retryPrompt");
    column("prompt_section", 0., path("/pane/owner"), vec![ready, failed])
}

/// Rename every `{"event": {"name": from}}` under `node` to `to`.
fn rename_event(node: &mut Value, from: &str, to: &str) {
    match node {
        Value::Object(map) => {
            if map.get("name").and_then(Value::as_str) == Some(from) && map.len() <= 2 {
                map.insert("name".into(), json!(to));
            }
            for value in map.values_mut() {
                rename_event(value, from, to);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| rename_event(item, from, to)),
        _ => {}
    }
}

/// One usage row (`settings::usage_bar`, web `UsageBar`): label left,
/// "current / limit" right, the 6 px capsule track under it (`gap_1`; no
/// track for unlimited).
fn usage(ink: &Ink, chrome: &Chrome, id: &str, label: &str) -> Value {
    let at = |key: &str| path(&format!("/pane/billing/{id}/{key}"));
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25)},
        "children": [
            {"id": format!("{id}_head"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "space-between"},
                "children": [ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground), ink.text(&format!("{id}_amount"), at("amount"), XS, &ink.muted)]},
            {"id": format!("{id}_track"), "component": "Box", "visible": at("track"), "style": {"height": 6, "width": "100%", "borderRadius": 9999, "backgroundColor": chrome.track},
                "children": [{"id": format!("{id}_fill"), "component": "Box", "style": {"height": "100%", "width": at("width"), "borderRadius": 9999, "backgroundColor": ink.primary}}]},
        ]})
}

/// EXP-288: Plan & Billing: the band with the plan chip, the three usage
/// meters in the section's own glass block (EXP-862), then the outline
/// `Manage billing on the web` hand-off.
fn billing(ink: &Ink, chrome: &Chrome) -> Value {
    let chip = json!({"id": "plan_chip", "component": "Box", "style": {"flexShrink": 0, "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.125), "borderRadius": 9999, "borderWidth": 1, "borderColor": chrome.chip_stroke, "backgroundColor": chrome.chip_fill},
        "children": [ink.text("plan_chip_text", path("/pane/billing/plan"), XS, &ink.foreground)]});
    let mut meters = ink.group("billing_group", vec![
        usage(ink, chrome, "seats", "Seats"),
        usage(ink, chrome, "storage", "Attachment storage"),
        usage(ink, chrome, "widgets", "Feedback widgets"),
    ]);
    meters["style"]["gap"] = json!(ink.r(0.75));
    meters["style"]["paddingHorizontal"] = json!(ink.r(1.));
    meters["style"]["paddingVertical"] = json!(ink.r(0.75));
    // `Button::outline().web_sm()` with its glyph: 32 px, `px 12`, the
    // input wash and stroke, the `size_4` glyph `gap_1` before the label.
    let size = ink.r(SM);
    let manage = json!({"id": "billing_manage", "component": "Box", "visible": path("/pane/billing/manage"), "style": {"display": "flex", "flexDirection": "row"},
        "children": [{"id": "billing_manage_button", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Manage billing on the web"}, "on": {"press": {"event": {"name": "manageBilling"}}},
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.button_stroke, "backgroundColor": chrome.button_fill, "cursor": "pointer"},
            "children": [
                {"id": "billing_manage_icon", "component": "Icon", "props": {"name": "ui-external-link"}, "style": {"width": ink.r(1.), "height": ink.r(1.), "color": chrome.button_fg, "flexShrink": 0}},
                {"id": "billing_manage_text", "component": "Text", "props": {"text": "Manage billing on the web"}, "style": {"fontSize": size, "lineHeight": size, "paddingTop": 1, "color": chrome.button_fg, "flexShrink": 0}},
            ]}]});
    column("billing", ink.r(0.5), path("/pane/billing/show"), vec![ink.band("billing_band", "Plan & Billing", Some(chip)), meters, manage])
}

/// Web settings/general.tsx's Danger zone (`settings::danger_zone`): the
/// danger-tinted band label, then one row: the muted sentence and the
/// danger `Button` at `web_xs` (24 px, `px 8`, the `size_3` glyph).
fn danger_zone(ink: &Ink, chrome: &Chrome) -> Value {
    let mut label = ink.text("danger_band_label", json!("Danger zone"), SM, &chrome.danger);
    label["style"]["fontWeight"] = json!(500);
    let mut sentence = ink.text("danger_text", json!("Permanently delete this team and all its data."), SM, &ink.muted);
    sentence["style"]["flexGrow"] = json!(1);
    sentence["style"]["flexShrink"] = json!(1);
    sentence["style"]["minWidth"] = json!(0);
    let size = ink.r(XS);
    let button = json!({"id": "danger_delete", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Delete team"}, "on": {"press": {"event": {"name": "deleteTeam"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 24, "paddingHorizontal": 8, "borderRadius": chrome.radius, "backgroundColor": chrome.danger_button, "cursor": "pointer", ":hover": {"backgroundColor": chrome.danger_button_hover}},
        "children": [
            {"id": "danger_delete_icon", "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": ink.r(0.75), "height": ink.r(0.75), "color": chrome.danger_button_fg, "flexShrink": 0}},
            {"id": "danger_delete_text", "component": "Text", "props": {"text": "Delete team"}, "style": {"fontSize": size, "lineHeight": size, "paddingTop": 1, "color": chrome.danger_button_fg, "flexShrink": 0}},
        ]});
    let mut row = ink.shell("danger_row", false, json!(true), vec![sentence, button]);
    row["style"]["justifyContent"] = json!("space-between");
    json!({"id": "danger", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.band_with("danger_band", Some(label), "", None, None), ink.group("danger_group", vec![row])]})
}

/// The pane's tree: `gap_4` between General, the team prompt, Plan &
/// Billing and the Danger zone; a member's note at the end. No team = the
/// empty state.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let mut note = ink.text("member_note", json!("Only team owners can change these settings."), XS, &ink.muted);
    note["visible"] = path("/pane/member");
    // A member's pane keeps the prompt's (empty) place in the stack.
    let spacer = json!({"id": "prompt_place", "component": "Box", "visible": path("/pane/member"), "style": {"height": 0}});
    let team = column("team", ink.r(1.), path("/pane/hasTeam"), vec![general(ink, chrome), prompt(ink, chrome), spacer, billing(ink, chrome), danger_zone(ink, chrome), note]);
    let empty = super::doc::empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, not(path("/pane/hasTeam")));
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [empty, team]})
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

    /// #EXP-1645: `upload.accept` + `upload.maxBytes`, on the picked file.
    #[test]
    fn a_team_image_pick_follows_the_fixture() {
        let max = domain::team_image::TEAM_IMAGE.upload.max_bytes;
        for accepted in &domain::team_image::TEAM_IMAGE.upload.accept {
            assert_eq!(team_image_refusal(accepted, max), None, "{accepted}");
        }
        assert!(team_image_refusal("image/png", max + 1).is_some());
        assert!(team_image_refusal("application/pdf", 1).is_some());
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
