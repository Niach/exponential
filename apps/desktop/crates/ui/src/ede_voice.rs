//! #EDE-267: the voice tray over the session bus. In eDE mode the shell's
//! status icon is the voice tray (the holder registers no
//! StatusNotifierItem of its own: `etalk_gpui::voice_tray::host_in_shell`),
//! and the holder publishes what its voice popup binds to the bus's
//! [`VoiceBoard`] ([`api::bus::Request::Voice`]): ONE object,
//! `etalk::voice::ede_voice_tray` (voice.json EDE TRAY), from
//!
//! - the tray's rows and icon (`voice_tray::menu` / `voice_tray::icon`);
//! - the member's global Assistant (THE voice agent of the eDE tray);
//! - the call (`voice_call::face`, only while it shows: a turn call has no
//!   surface);
//! - the last lines of the call's conversation: the call session's
//!   messages, else the call agent's (the Assistant's) chat, read from the
//!   chat store the way the feeds read the Assistant's
//!   ([`crate::ede_feeds::assistant_messages`]).
//!
//! It republishes ~[`DEBOUNCE`] after a change of the tray, the call, the
//! messages, the sessions, the agents or the account, plus every [`TICK`]
//! while a call runs (the level and the timer, which only the call surfaces
//! observe), never more often; [`VoiceBoard::set`] drops an equal object.
//! [`api::bus::Request::VoiceAct`] lands in [`act`].

use std::{
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};

use api::bus::VoiceBoard;
use etalk::voice::{ede_voice_tray, tray_item_of_value, EdeTrayAssistant, EdeTrayCall, EdeTrayInput, EdeTrayLine, TrayItem, TrayItemId};
use gpui::{App, AppContext as _, Context, Entity, Global, Subscription};
use serde_json::Value;
use sync::Store;

/// How long a change waits for the next before the object is rebuilt.
const DEBOUNCE: Duration = Duration::from_millis(50);

/// How often the object is rebuilt while a call runs (its level, its timer).
pub(crate) const TICK: Duration = Duration::from_millis(250);

/// The voice tray object as the bus hands it out.
pub fn voice_board() -> Arc<VoiceBoard> {
    static BOARD: OnceLock<Arc<VoiceBoard>> = OnceLock::new();
    BOARD.get_or_init(Default::default).clone()
}

/// What a publish reads: the object and whether a call runs (the tick).
type Gather = Rc<dyn Fn(&mut App) -> (Value, bool)>;

pub(crate) struct Publisher {
    board: Arc<VoiceBoard>,
    gather: Gather,
    /// A debounced rebuild is on its way.
    pending: bool,
    /// The [`TICK`] loop runs.
    ticking: bool,
    _subscriptions: Vec<Subscription>,
}

struct Voice(Entity<Publisher>);

impl Global for Voice {}

/// eDE mode: publish the voice tray now and after every change.
pub(crate) fn install(cx: &mut App) {
    if !crate::ede_component::is_component() || cx.has_global::<Voice>() {
        return;
    }
    let Some(store) = Store::try_global(cx) else { return };
    let collections = store.collections().clone();
    let state = store.state();
    let tray = etalk_gpui::voice_tray::entity(cx);
    let call = etalk_gpui::voice_call::entity(cx);
    let publisher = cx.new(|cx: &mut Context<Publisher>| {
        let mut subscriptions = vec![
            cx.observe(&collections.messages, |this: &mut Publisher, _, cx| this.schedule(cx)),
            cx.observe(&collections.sessions, |this: &mut Publisher, _, cx| this.schedule(cx)),
            cx.observe(&collections.agents, |this: &mut Publisher, _, cx| this.schedule(cx)),
            cx.observe(&state, |this: &mut Publisher, _, cx| this.schedule(cx)),
        ];
        if let Some(tray) = tray {
            subscriptions.push(cx.observe(&tray, |this: &mut Publisher, _, cx| this.schedule(cx)));
        }
        if let Some(call) = call {
            subscriptions.push(cx.observe(&call, |this: &mut Publisher, _, cx| this.schedule(cx)));
        }
        let mut this = Publisher::new(voice_board(), Rc::new(gather), subscriptions);
        this.schedule(cx);
        this
    });
    cx.set_global(Voice(publisher));
}

/// Rebuild soon (after an act: the tray's optimistic flip, a mute).
fn republish(cx: &mut App) {
    if let Some(publisher) = cx.try_global::<Voice>().map(|voice| voice.0.clone()) {
        publisher.update(cx, |this, cx| this.schedule(cx));
    }
}

impl Publisher {
    fn new(board: Arc<VoiceBoard>, gather: Gather, subscriptions: Vec<Subscription>) -> Self {
        Publisher { board, gather, pending: false, ticking: false, _subscriptions: subscriptions }
    }

    /// A rebuild ~[`DEBOUNCE`] from now (one for a burst of changes).
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        self.pending = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.pending = false;
                if this.publish(cx) {
                    this.tick(cx);
                }
            });
        })
        .detach();
    }

    /// The object now on the board; true = a call runs.
    fn publish(&mut self, cx: &mut Context<Self>) -> bool {
        let gather = self.gather.clone();
        let (voice, running) = gather(cx);
        self.board.set(voice);
        running
    }

    /// While a call runs: a rebuild every [`TICK`], ending with the call.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.ticking, true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let running = this.update(cx, |this, cx| {
                    let running = this.publish(cx);
                    if !running {
                        this.ticking = false;
                    }
                    running
                });
                if !running.unwrap_or(false) {
                    return;
                }
            }
        })
        .detach();
    }
}

/// The fold's input from the holder now.
fn gather(cx: &mut App) -> (Value, bool) {
    let _span = tracing::trace_span!("ede.voice").entered();
    let call = call_input(cx);
    let running = call.is_some();
    let assistant = crate::session_data::my_assistant_anywhere(None, cx);
    let lines = lines(assistant.as_ref().map(|assistant| assistant.id.as_str()), cx);
    let input = EdeTrayInput {
        menu: etalk_gpui::voice_tray::menu(cx),
        icon: etalk_gpui::voice_tray::icon(cx),
        assistant: assistant.map(|assistant| EdeTrayAssistant { id: assistant.id, name: assistant.name.unwrap_or_default() }),
        call,
        lines,
    };
    (serde_json::to_value(ede_voice_tray(&input)).unwrap_or(Value::Null), running)
}

/// The call the surfaces show (None = none, or a turn call).
fn call_input(cx: &App) -> Option<EdeTrayCall> {
    if !etalk_gpui::voice_call::shown(cx) {
        return None;
    }
    let face = etalk_gpui::voice_call::face(cx)?;
    Some(EdeTrayCall {
        activity: face.view.activity.to_string(),
        status: face.view.status.clone(),
        timer: face.timer.to_string(),
        muted: face.view.muted,
        level: f64::from(face.level),
        agent: face.agent.to_string(),
    })
}

/// The last lines of the call's conversation: its own call session (a child
/// of the chat), else the call agent's chat (the Assistant's).
fn lines(assistant_id: Option<&str>, cx: &App) -> Vec<EdeTrayLine> {
    let read = |session_id: &str, root_id: &str| -> Vec<EdeTrayLine> {
        let rows = crate::chat_window::chat_messages(root_id, cx).rows;
        crate::ede_feeds::assistant_messages(session_id, &rows).iter().filter_map(line_of).collect()
    };
    if let Some(session_id) = etalk_gpui::voice_call::session(cx) {
        let root = Store::try_global(cx)
            .and_then(|store| store.collections().sessions.read(cx).get(&session_id).map(|session| session.root_id_or_self().to_string()));
        if let Some(root) = root {
            return read(&session_id, &root);
        }
    }
    let Some(agent_id) = etalk_gpui::voice_call::agent_id(cx).or(assistant_id.map(str::to_string)) else { return Vec::new() };
    match crate::session_data::my_conversation(&agent_id, cx) {
        Some(root) => read(&root.id, &root.id),
        None => Vec::new(),
    }
}

/// One of [`crate::ede_feeds::assistant_messages`]' messages as a line.
fn line_of(message: &Value) -> Option<EdeTrayLine> {
    Some(EdeTrayLine {
        id: message["id"].as_str()?.to_string(),
        side: message["side"].as_str()?.to_string(),
        text: message["text"].as_str()?.to_string(),
    })
}

/// The tray row a `menu` act names, if the tray shows it enabled now (Open
/// and Quit are never the shell's).
pub(crate) fn menu_row(menu: Option<&[TrayItem]>, item: Option<&str>) -> Result<TrayItemId, String> {
    let item = item.filter(|item| !item.is_empty()).ok_or("no menu item")?;
    let id = tray_item_of_value(item).filter(|id| !matches!(id, TrayItemId::Open | TrayItemId::Quit)).ok_or_else(|| format!("no menu item {item}"))?;
    let menu = menu.ok_or("this device is not the voice device")?;
    let enabled = menu.iter().find_map(|row| match row {
        TrayItem::Action { id: row, enabled, .. } | TrayItem::Check { id: row, enabled, .. } if *row == id => Some(*enabled),
        _ => None,
    });
    match enabled {
        Some(true) => Ok(id),
        Some(false) => Err(format!("{item} is disabled")),
        None => Err(format!("the menu has no {item}")),
    }
}

/// The bus's `voice-act`: `call` the Assistant, `mute` / `end` the call,
/// `menu {item}` = the tray's own handler for that row (its toggles
/// optimistic, as in the tray). `Err` = why not.
pub(crate) fn act(action: &str, item: Option<&str>, cx: &mut App) -> Result<(), String> {
    use etalk_gpui::voice_call;
    match action {
        "call" => {
            if voice_call::active(cx) {
                return Err("a call runs".into());
            }
            let assistant = crate::session_data::my_assistant_anywhere(None, cx).ok_or("no Assistant")?;
            if let Some(reason) = voice_call::unavailable(&assistant.id, cx) {
                return Err(reason);
            }
            voice_call::start(assistant.id, cx);
        }
        "mute" | "end" if !voice_call::shown(cx) => return Err("no call".into()),
        "mute" => voice_call::toggle_mute(cx),
        "end" => voice_call::end(cx),
        "menu" => {
            let id = menu_row(etalk_gpui::voice_tray::menu(cx).as_deref(), item)?;
            etalk_gpui::voice_tray::click(id, cx);
        }
        other => return Err(format!("no action {other}")),
    }
    republish(cx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn row(id: TrayItemId, enabled: bool) -> TrayItem {
        TrayItem::Check { id, label: "x".into(), checked: false, enabled }
    }

    #[test]
    fn a_menu_act_names_a_shown_enabled_row() {
        let menu = vec![
            TrayItem::Action { id: TrayItemId::Open, label: "Open".into(), enabled: true },
            TrayItem::Separator,
            row(TrayItemId::WakeWord, true),
            row(TrayItemId::Meeting, false),
            TrayItem::Action { id: TrayItemId::Quit, label: "Quit".into(), enabled: true },
        ];
        assert_eq!(menu_row(Some(&menu), Some("wakeWord")), Ok(TrayItemId::WakeWord));
        assert!(menu_row(Some(&menu), Some("meeting")).unwrap_err().contains("disabled"));
        assert!(menu_row(Some(&menu), Some("mute")).unwrap_err().contains("no mute"));
        assert!(menu_row(Some(&menu), Some("open")).is_err(), "Open is the shell's");
        assert!(menu_row(Some(&menu), Some("quit")).is_err(), "Quit makes no sense in eDE");
        assert!(menu_row(Some(&menu), Some("bogus")).is_err());
        assert!(menu_row(Some(&menu), None).is_err());
        assert!(menu_row(None, Some("wakeWord")).is_err(), "not the voice device");
    }

    #[test]
    fn lines_read_the_feeds_messages() {
        let message = serde_json::json!({"id": "m1", "side": "in", "text": "hi", "at": "2026-11-27T10:00:00Z"});
        assert_eq!(line_of(&message), Some(EdeTrayLine { id: "m1".into(), side: "in".into(), text: "hi".into() }));
        assert_eq!(line_of(&serde_json::json!({"id": "m2"})), None);
    }

    /// The publisher with a fake gather: how often it ran, and a switch for
    /// "a call runs".
    fn publisher(cx: &mut gpui::TestAppContext) -> (Entity<Publisher>, Arc<VoiceBoard>, Rc<Cell<usize>>, Rc<Cell<bool>>) {
        let board = Arc::new(VoiceBoard::default());
        let (gathers, running) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(false)));
        let gather: Gather = {
            let (gathers, running) = (gathers.clone(), running.clone());
            Rc::new(move |_: &mut App| {
                gathers.set(gathers.get() + 1);
                (serde_json::json!({"call": running.get(), "n": gathers.get()}), running.get())
            })
        };
        let entity = cx.update(|cx| cx.new(|_| Publisher::new(board.clone(), gather, Vec::new())));
        (entity, board, gathers, running)
    }

    #[gpui::test]
    async fn a_change_publishes_once_and_without_a_call_nothing_ticks(cx: &mut gpui::TestAppContext) {
        let (publisher, board, gathers, _) = publisher(cx);
        publisher.update(cx, |this, cx| {
            this.schedule(cx);
            this.schedule(cx);
            this.schedule(cx);
        });
        cx.executor().advance_clock(DEBOUNCE);
        cx.run_until_parked();
        assert_eq!(gathers.get(), 1, "a burst of changes is one rebuild");
        assert_eq!(board.wait(None, Duration::ZERO).0, 1);
        cx.executor().advance_clock(Duration::from_secs(5));
        cx.run_until_parked();
        assert_eq!(gathers.get(), 1, "no call: no tick");
        publisher.update(cx, |this, _| assert!(!this.ticking));
    }

    #[gpui::test]
    async fn the_tick_runs_every_250_ms_while_a_call_runs_and_stops_with_it(cx: &mut gpui::TestAppContext) {
        let (publisher, _, gathers, running) = publisher(cx);
        running.set(true);
        publisher.update(cx, |this, cx| this.schedule(cx));
        cx.executor().advance_clock(DEBOUNCE);
        cx.run_until_parked();
        assert_eq!(gathers.get(), 1);
        publisher.update(cx, |this, _| assert!(this.ticking));
        for n in 2..=5 {
            cx.executor().advance_clock(TICK);
            cx.run_until_parked();
            assert_eq!(gathers.get(), n, "one rebuild per tick");
        }
        // Never more often: a change while it ticks starts no second loop.
        publisher.update(cx, |this, cx| this.schedule(cx));
        cx.executor().advance_clock(DEBOUNCE);
        cx.run_until_parked();
        assert_eq!(gathers.get(), 6, "the change's own rebuild");
        cx.executor().advance_clock(TICK - DEBOUNCE);
        cx.run_until_parked();
        assert_eq!(gathers.get(), 7, "the tick kept its period");
        // The call ends: the next tick sees it and the loop stops.
        running.set(false);
        cx.executor().advance_clock(TICK);
        cx.run_until_parked();
        assert_eq!(gathers.get(), 8);
        publisher.update(cx, |this, _| assert!(!this.ticking));
        cx.executor().advance_clock(Duration::from_secs(5));
        cx.run_until_parked();
        assert_eq!(gathers.get(), 8, "no call: the tick stopped");
    }
}
