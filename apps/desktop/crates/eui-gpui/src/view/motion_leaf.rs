//! EXP-1604: the surface's Motion leaves (`eui-motion/README.md` ›
//! "Painting a Motion"). Each Motion node keeps a [`MotionLeaf`]: its
//! loaded [`MotionPlayer`] (reloaded when the source, the theme or the mode
//! changes), its clock, and the image of the last painted frame. The leaf
//! paints through [`MotionCanvas`], an element that knows its device size
//! and whether it is on screen:
//!
//! - a bound / literal `progress` shows `at_progress` and never ticks;
//! - otherwise it plays when `autoplay`, `play` is not `false`, motion is
//!   not reduced, the element is inside its clip, the window is active and
//!   it is among the first [`MAX_PLAYING`] such Motions of the surface (tree
//!   order); a finite loop that played out holds its last frame;
//! - anything else shows the poster (`play: false` freezes where it is);
//! - a playing Motion repaints on a timer at most every [`FRAME_STEP`]
//!   (60 fps), never through gpui's every-frame animation request.
//!
//! EXP-1605 (README › "Interactivity (M4)"): a leaf runs the engine's
//! [`StateMachine`] (the `stateMachine` prop, else a `.lottie` src's first
//! machine); its current state's playback replaces the playback props, the
//! `inputs` prop is written on every change, pointer and keyboard input is
//! posted, a played-out state posts `complete` once, and every entered
//! state restarts the clock at 0 and fires `on.state` (`{state}`). The
//! leaf also fires `on.hover`, `on.visible` (first paint half on screen)
//! and `on.complete`. Events queue on the leaf and go through the core's
//! action dispatch ([`SurfaceView::fire`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eui_core::surface::PlacedNode;
use eui_core::theme::Mode;
use eui_motion::state_machine::{Interaction, StateMachine};
use gpui::{
    AppContext as _,
    px, AnyElement, WeakEntity, App, Bounds, Context, Corners, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Style, Window,
};
use serde_json::{json, Map, Value};

use super::SurfaceView;
use crate::motion::{fetched_motion, inline_lottie, machine_prop, theme_slots, Fit, MotionPlayer};

/// At most this many Motions animate per surface; the rest show their poster.
pub const MAX_PLAYING: usize = 8;

/// The fastest a Motion repaints: 60 fps.
pub const FRAME_STEP: Duration = Duration::from_micros(16_667);

/// Where a `src` animation is.
enum Source {
    /// Not fetched yet (the fetch runs).
    Pending,
    /// The animation and the archive's first state machine.
    Loaded(Value, Option<Value>),
    Failed(String),
}

/// One Motion node's state.
pub(crate) struct MotionLeaf {
    /// Hash of what the player was loaded from (source, slots, mode, theme).
    key: u64,
    player: Option<MotionPlayer>,
    /// A load failure: the box stays empty (README step 1).
    pub error: Option<String>,
    /// The `src` this leaf fetched, and its result.
    src: Option<(String, Rc<RefCell<Source>>)>,
    /// Play time banked before the current run.
    played: Duration,
    /// When the current run started (`None` = not ticking).
    running_since: Option<Instant>,
    /// The repaint timer's due time (none or past = none pending).
    armed_until: Option<Instant>,
    image: Option<Arc<RenderImage>>,
    /// The composition frame last painted.
    frame: Option<f64>,
    /// EXP-1605: the running state machine and the hash of its source.
    machine: Option<StateMachine>,
    machine_key: u64,
    /// The `inputs` values last written into the machine.
    inputs_seen: Map<String, Value>,
    /// The current play (state) reported `complete`.
    completed: bool,
    /// `on.visible` fired.
    seen: bool,
    /// The pointer is over it / went down on it.
    hovered: bool,
    down: bool,
    /// Events for the node's `on` (event, payload), fired by
    /// [`SurfaceView::flush_motion_events`].
    pending: Vec<(&'static str, Option<Value>)>,
}

impl MotionLeaf {
    fn new() -> Self {
        Self { key: 0, player: None, error: None, src: None, played: Duration::ZERO, running_since: None, armed_until: None, image: None, frame: None, machine: None, machine_key: 0, inputs_seen: Map::new(), completed: false, seen: false, hovered: false, down: false, pending: Vec::new() }
    }

    /// The clock back at 0 (a new state, a new play).
    fn restart(&mut self) {
        self.played = Duration::ZERO;
        self.running_since = None;
        self.armed_until = None;
        self.completed = false;
    }

    /// After the machine moved: each entered state restarts the clock and
    /// queues `on.state`; a final state queues `on.complete`; the current
    /// state's playback replaces the props'. `true` = the state changed.
    fn settle_machine(&mut self) -> bool {
        let Some(m) = self.machine.as_mut() else { return false };
        let entered = m.take_entered();
        let fin = m.is_final();
        let playback = m.playback().clone();
        if let Some(p) = self.player.as_mut() {
            p.playback = playback;
        }
        if entered.is_empty() {
            return false;
        }
        self.restart();
        for state in entered {
            self.pending.push(("state", Some(json!({"state": state}))));
        }
        if fin {
            self.completed = true;
            self.pending.push(("complete", None));
        }
        true
    }

    /// Post an interaction to the machine (none: nothing).
    fn post(&mut self, interaction: &Interaction) -> bool {
        match self.machine.as_mut() {
            Some(m) => {
                m.post(interaction);
                self.settle_machine()
            }
            None => false,
        }
    }

    /// The machine's current state (tests, automation).
    pub fn machine_state(&self) -> Option<String> {
        self.machine.as_ref().map(|m| m.state().to_string())
    }

    /// Wall time of the last frame's paint (µs), for benches and reports.
    pub fn last_paint_us(&self) -> Option<u64> {
        self.player.as_ref().map(|p| p.last_paint_us)
    }

    /// Fire the queued events after this paint.
    fn flush_later(&self, view: &WeakEntity<SurfaceView>, cx: &mut App) {
        if self.pending.is_empty() {
            return;
        }
        let view = view.clone();
        cx.defer(move |cx| {
            let _ = view.update(cx, |this, cx| this.flush_motion_events(cx));
        });
    }

    fn elapsed(&self, now: Instant) -> Duration {
        self.played + self.running_since.map_or(Duration::ZERO, |s| now.saturating_duration_since(s))
    }

}

/// What a Motion shows this render.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drive {
    /// A fixed point (0–1) of the segment.
    Progress(f64),
    /// Playing, when on screen and the window is active.
    Play,
    /// Paused where it is (`play: false`).
    Frozen,
    /// The poster (reduced motion, `autoplay: false`, over the cap).
    Poster,
}

fn hash_of(parts: &[&Value], extra: u64) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.to_string().hash(&mut h);
    }
    extra.hash(&mut h);
    h.finish()
}

/// Does this node want to play (before the per-surface cap)?
fn wants_to_play(n: &PlacedNode) -> bool {
    let p = &n.props;
    if p.get("stateMachine").is_some() {
        return true;
    }
    p.get("progress").and_then(Value::as_f64).is_none() && p.get("autoplay").and_then(Value::as_bool) != Some(false) && p.get("play").and_then(Value::as_bool) != Some(false)
}

impl SurfaceView {
    /// The Motion leaf of node `id` (tests, benches).
    pub fn motion_paint_us(&self, id: &str) -> Option<u64> {
        self.motion_leaves.borrow().get(id).and_then(|l| l.borrow().last_paint_us())
    }

    /// The composition frame Motion `id` last painted (tests, automation).
    pub fn motion_frame(&self, id: &str) -> Option<f64> {
        self.motion_leaves.borrow().get(id).and_then(|l| l.borrow().frame)
    }

    /// Pause Motions (show their posters) while the window is in the
    /// background (default on, the README's rule); off for hosts that
    /// capture or embed an inactive window, and for headless tests (gpui's
    /// test windows are never active).
    pub fn set_pause_motion_in_background(&mut self, on: bool) {
        self.motion_background_pause = on;
    }

    /// EXP-1605: the state machine state of Motion `id` (`None` = no machine).
    pub fn motion_state(&self, id: &str) -> Option<String> {
        self.motion_leaves.borrow().get(id).and_then(|l| l.borrow().machine_state())
    }

    /// The playback range `[from, to)` (frames) Motion `id` plays now: its
    /// machine state's, else its props' (tests, automation).
    pub fn motion_range(&self, id: &str) -> Option<(f64, f64)> {
        self.motion_leaves.borrow().get(id).and_then(|l| l.borrow().player.as_ref().map(|p| p.timeline.range_of(&p.playback)))
    }

    /// Report an interaction on Motion `id` to its state machine (pointer
    /// input arrives here from the painter; hosts and tests may call it):
    /// a state change restarts its clock and fires `on.state`.
    pub fn motion_interact(&mut self, id: &str, interaction: &Interaction, cx: &mut Context<Self>) {
        let Some(leaf) = self.motion_leaves.borrow().get(id).cloned() else { return };
        if leaf.borrow_mut().post(interaction) {
            cx.notify();
        }
        self.flush_motion_events(cx);
    }

    /// Pointer over / off Motion `id`: `pointerEnter` / `pointerExit`, and
    /// `on.hover` on entering.
    pub(crate) fn motion_hover(&mut self, id: &str, hovered: bool, cx: &mut Context<Self>) {
        let Some(leaf) = self.motion_leaves.borrow().get(id).cloned() else { return };
        {
            let mut l = leaf.borrow_mut();
            if l.hovered == hovered {
                return;
            }
            l.hovered = hovered;
            if hovered {
                l.pending.push(("hover", None));
            } else {
                l.down = false;
            }
        }
        let i = if hovered { Interaction::PointerEnter { layer: None } } else { Interaction::PointerExit { layer: None } };
        self.motion_interact(id, &i, cx);
    }

    /// Pointer down / up on Motion `id` (`click` = up after down on it;
    /// a pressable Motion's click comes from [`SurfaceView::press`], which
    /// keyboard activation reaches too).
    pub(crate) fn motion_pointer(&mut self, id: &str, down: bool, click: bool, cx: &mut Context<Self>) {
        let Some(leaf) = self.motion_leaves.borrow().get(id).cloned() else { return };
        let was_down = std::mem::replace(&mut leaf.borrow_mut().down, down);
        if down {
            self.motion_interact(id, &Interaction::PointerDown { layer: None }, cx);
            return;
        }
        self.motion_interact(id, &Interaction::PointerUp { layer: None }, cx);
        if click && was_down {
            self.motion_interact(id, &Interaction::Click { layer: None }, cx);
        }
    }

    /// Is `id` a Motion leaf?
    pub(crate) fn is_motion(&self, id: &str) -> bool {
        self.motion_leaves.borrow().contains_key(id)
    }

    /// Fire the Motion leaves' queued events through the core's dispatch.
    pub(crate) fn flush_motion_events(&mut self, cx: &mut Context<Self>) {
        let queued: Vec<(String, Vec<(&'static str, Option<Value>)>)> = self.motion_leaves.borrow().iter().filter_map(|(id, l)| {
            let mut l = l.borrow_mut();
            (!l.pending.is_empty()).then(|| (id.clone(), std::mem::take(&mut l.pending)))
        }).collect();
        for (id, events) in queued {
            let Some(index) = self.cache.index_of(&id) else { continue };
            for (event, payload) in events {
                self.fire(index, event, payload, cx);
            }
        }
    }

    /// The load error of Motion `id`, if any.
    pub fn motion_error(&self, id: &str) -> Option<String> {
        self.motion_leaves.borrow().get(id).and_then(|l| l.borrow().error.clone())
    }

    /// Is Motion `id` among the first [`MAX_PLAYING`] that want to play?
    fn motion_may_play(&self, id: &str) -> bool {
        let mut rank = 0;
        for n in self.cache.nodes.iter().filter(|n| !n.removed && n.component == "Motion" && n.part.is_none()) {
            if n.id == id {
                return rank < MAX_PLAYING;
            }
            if wants_to_play(n) {
                rank += 1;
            }
        }
        false
    }

    /// The element painting Motion `n` in a `w × h` box.
    pub(super) fn motion_element(&self, n: &PlacedNode, cx: &mut Context<Self>) -> AnyElement {
        let leaf = self.motion_leaves.borrow_mut().entry(n.id.clone()).or_insert_with(|| Rc::new(RefCell::new(MotionLeaf::new()))).clone();
        self.prune_motion_leaves(cx);
        let mode = self.surface.mode();
        self.load_motion(&leaf, n, mode, cx);
        self.sync_machine(&leaf, n, cx);
        let props = &n.props;
        let reduced = self.surface.settings().reduced_motion || self.ghosting.get();
        let machine_autoplay = leaf.borrow().machine.as_ref().map(|m| m.playback().autoplay);
        let drive = match machine_autoplay {
            // The machine's state playback replaces the playback props.
            Some(autoplay) if reduced || !autoplay || !self.motion_may_play(&n.id) => Drive::Poster,
            Some(_) => Drive::Play,
            None => match props.get("progress").and_then(Value::as_f64) {
            Some(p) => Drive::Progress(p),
            None if reduced || props.get("autoplay").and_then(Value::as_bool) == Some(false) => Drive::Poster,
            None if props.get("play").and_then(Value::as_bool) == Some(false) => Drive::Frozen,
            None if !self.motion_may_play(&n.id) => Drive::Poster,
            None => Drive::Play,
            },
        };
        MotionCanvas { id: ElementId::Name(format!("{}.motion", n.id).into()), leaf, drive, fit: Fit::from_props(props), now: self.now_or_instant(), background_pause: self.motion_background_pause, view: cx.entity().downgrade() }.into_any_element()
    }

    /// EXP-1605: (re)start the leaf's state machine when its source
    /// changed (the `stateMachine` prop, else the fetched `.lottie`'s),
    /// write changed `inputs`, and hand the state's playback to the player.
    fn sync_machine(&self, leaf: &Rc<RefCell<MotionLeaf>>, n: &PlacedNode, cx: &mut Context<Self>) {
        let mut guard = leaf.borrow_mut();
        let l = &mut *guard;
        let source = machine_prop(&n.props).or_else(|| match l.src.as_ref().map(|(_, c)| c.borrow()) {
            Some(c) => match &*c {
                Source::Loaded(_, m) => m.clone(),
                _ => None,
            },
            None => None,
        });
        let key = source.as_ref().map_or(0, |m| hash_of(&[m], 1));
        if key != l.machine_key {
            l.machine_key = key;
            l.machine = source.as_ref().and_then(StateMachine::parse);
            l.inputs_seen.clear();
            l.pending.retain(|(e, _)| *e != "state");
            if l.machine.is_none() {
                if let Some(p) = l.player.as_mut() {
                    p.set_playback(&n.props);
                }
            }
        }
        if let (Some(m), Some(Value::Object(inputs))) = (l.machine.as_mut(), n.props.get("inputs")) {
            let changed: Vec<(String, Value)> = inputs.iter().filter(|(k, v)| l.inputs_seen.get(*k) != Some(*v)).map(|(k, v)| (k.clone(), v.clone())).collect();
            for (k, v) in &changed {
                m.set_input(k, v);
            }
            for (k, v) in changed {
                l.inputs_seen.insert(k, v);
            }
        }
        l.settle_machine();
        if !l.pending.is_empty() {
            let view = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = view.update(cx, |this, cx| this.flush_motion_events(cx));
            });
        }
    }

    /// Forget leaves whose node left the tree (their images leave the atlas).
    fn prune_motion_leaves(&self, cx: &mut Context<Self>) {
        let mut leaves = self.motion_leaves.borrow_mut();
        let live = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "Motion").count();
        if leaves.len() <= live {
            return;
        }
        let ids: std::collections::HashSet<&str> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "Motion").map(|n| n.id.as_str()).collect();
        let gone: Vec<String> = leaves.keys().filter(|k| !ids.contains(k.as_str())).cloned().collect();
        for id in gone {
            if let Some(l) = leaves.remove(&id) {
                if let Some(image) = l.borrow_mut().image.take() {
                    cx.drop_image(image, None);
                }
            }
        }
    }

    /// (Re)load the leaf's player when its source, slots, theme or mode
    /// changed; start a `src` fetch once.
    fn load_motion(&self, leaf: &Rc<RefCell<MotionLeaf>>, n: &PlacedNode, mode: Mode, cx: &mut Context<Self>) {
        let props = &n.props;
        let theme = self.surface.theme().cloned();
        let theme_id = theme.as_ref().map_or(0, |t| Arc::as_ptr(t) as usize as u64);
        let src = props.get("src").and_then(Value::as_str).unwrap_or("").to_string();
        let null = Value::Null;
        let mut l = leaf.borrow_mut();
        // A `src` fetch: once per url.
        if props.get("layers").is_none() && props.get("lottie").is_none() && !src.is_empty() && l.src.as_ref().is_none_or(|(u, _)| *u != src) {
            let cell = Rc::new(RefCell::new(Source::Pending));
            l.src = Some((src.clone(), cell.clone()));
            match crate::media::allowed_request(self.host.as_ref(), &src) {
                None => *cell.borrow_mut() = Source::Failed(format!("`{src}` is not an allowed media url")),
                Some(req) => {
                    let key = crate::media::MediaKey::new(&req, &self.host.media_options());
                    let task = cx.background_spawn(async move { crate::media::fetch(&key) });
                    cx.spawn(async move |this, cx| {
                        let result = task.await;
                        *cell.borrow_mut() = match result.and_then(|b: Vec<u8>| fetched_motion(&b)) {
                            Ok((v, m)) => Source::Loaded(v, m),
                            Err(e) => Source::Failed(e),
                        };
                        let _ = this.update(cx, |_, cx| cx.notify());
                    })
                    .detach();
                }
            }
        }
        let src_state = l.src.as_ref().map(|(_, c)| c.clone());
        let src_version = match src_state.as_ref().map(|c| matches!(&*c.borrow(), Source::Pending)) {
            Some(true) => 1,
            Some(false) => 2,
            None => 0,
        };
        let key = hash_of(
            &[props.get("layers").unwrap_or(&null), props.get("lottie").unwrap_or(&null), props.get("size").unwrap_or(&null), props.get("duration").unwrap_or(&null), props.get("fps").unwrap_or(&null), props.get("assets").unwrap_or(&null), props.get("markers").unwrap_or(&null), props.get("start").unwrap_or(&null), props.get("time").unwrap_or(&null), props.get("slots").unwrap_or(&null), &Value::String(src)],
            theme_id ^ ((mode == Mode::Dark) as u64) << 1 ^ src_version << 2,
        );
        if l.key == key && (l.player.is_some() || l.error.is_some() || src_version == 1) {
            if l.machine.is_none() {
                if let Some(p) = l.player.as_mut() {
                    p.set_playback(props);
                }
            }
            return;
        }
        l.key = key;
        let lottie = match inline_lottie(props) {
            Ok(Some(v)) => Ok(v),
            Err(e) => Err(e),
            Ok(None) => match src_state.as_ref().map(|c| match &*c.borrow() {
                Source::Loaded(v, _) => Ok(Some(v.clone())),
                Source::Failed(e) => Err(e.clone()),
                Source::Pending => Ok(None),
            }) {
                Some(Ok(Some(v))) => Ok(v),
                Some(Ok(None)) => {
                    l.player = None;
                    l.error = None;
                    return;
                }
                Some(Err(e)) => Err(e),
                None => Err("no source (layers, lottie or src)".to_string()),
            },
        };
        let loaded = lottie.and_then(|lottie| {
            let slots = theme_slots(&lottie, theme.as_deref(), mode, props.get("slots"));
            MotionPlayer::load(&lottie, &slots, props)
        });
        match loaded {
            Ok(mut p) => {
                if let Some(m) = l.machine.as_ref() {
                    p.playback = m.playback().clone();
                }
                l.player = Some(p);
                l.error = None;
                l.completed = false;
            }
            Err(e) => {
                l.player = None;
                l.error = Some(e.clone());
                let host = self.host.clone();
                let error = crate::host::PaintError { surface_id: self.surface.id.clone(), component_id: n.id.clone(), message: format!("Motion failed to load: {e}") };
                cx.defer(move |cx| host.on_paint_error(&error, cx));
            }
        }
    }
}

/// The element painting one Motion leaf into its bounds.
pub(crate) struct MotionCanvas {
    id: ElementId,
    leaf: Rc<RefCell<MotionLeaf>>,
    drive: Drive,
    fit: Fit,
    now: Instant,
    background_pause: bool,
    view: WeakEntity<SurfaceView>,
}

impl IntoElement for MotionCanvas {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for MotionCanvas {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut Window, _: &mut App) {}

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        let (visible, half_visible) = {
            let clip = window.content_mask().bounds.intersect(&bounds);
            let area = |b: &Bounds<Pixels>| f32::from(b.size.width).max(0.0) * f32::from(b.size.height).max(0.0);
            (clip.size.width > px(0.0) && clip.size.height > px(0.0), area(&bounds) > 0.0 && area(&clip) >= area(&bounds) * 0.5)
        };
        let active = !self.background_pause || window.is_window_active();
        let now = self.now;
        let mut played_out = false;
        let mut leaf = self.leaf.borrow_mut();
        let leaf = &mut *leaf;
        if half_visible && !leaf.seen {
            leaf.seen = true;
            leaf.pending.push(("visible", None));
        }
        if leaf.player.is_none() {
            leaf.flush_later(&self.view, cx);
            return;
        }
        let Some(player) = leaf.player.as_mut() else { return };
        let (frame, ticking) = match self.drive {
            Drive::Progress(p) => (player.timeline.at_progress(&player.playback, p), false),
            Drive::Poster => (player.timeline.poster(&player.playback), false),
            Drive::Frozen | Drive::Play if !visible || !active => {
                // Off screen / window in the background: the poster, clock paused.
                if leaf.running_since.is_some() {
                    leaf.played += now.saturating_duration_since(leaf.running_since.take().unwrap_or(now));
                }
                (player.timeline.poster(&player.playback), false)
            }
            Drive::Frozen => {
                if leaf.running_since.is_some() {
                    leaf.played += now.saturating_duration_since(leaf.running_since.take().unwrap_or(now));
                }
                (player.timeline.tick(&player.playback, leaf.played.as_secs_f64() * 1000.0).frame, false)
            }
            Drive::Play => {
                let since = *leaf.running_since.get_or_insert(now);
                let elapsed = leaf.played + now.saturating_duration_since(since);
                let t = player.timeline.tick(&player.playback, elapsed.as_secs_f64() * 1000.0);
                if t.done && !leaf.completed {
                    played_out = true;
                }
                (t.frame, !t.done)
            }
        };
        if !ticking && self.drive != Drive::Play {
            leaf.armed_until = None;
        }
        let scale = window.scale_factor();
        let (w, h) = ((f32::from(bounds.size.width) * scale).round() as u32, (f32::from(bounds.size.height) * scale).round() as u32);
        if played_out {
            // The state's (or the props') finite play finished: `complete`
            // once; the machine may move on (its clock restarts at 0).
            leaf.completed = true;
            let moved = match leaf.machine.as_mut() {
                Some(m) => {
                    let changed = m.complete();
                    // Entering a final state reports `complete` itself.
                    if !(changed && m.is_final()) {
                        leaf.pending.push(("complete", None));
                    }
                    leaf.settle_machine()
                }
                None => {
                    leaf.pending.push(("complete", None));
                    false
                }
            };
            if moved {
                let view = window.current_view();
                cx.defer(move |cx| cx.notify(view));
            }
        }
        leaf.flush_later(&self.view, cx);
        let Some(player) = leaf.player.as_mut() else { return };
        if !visible {
            return;
        }
        let mut replaced = None;
        let image = player.render_image(frame, w, h, self.fit, &mut replaced);
        if let Some(old) = replaced {
            cx.drop_image(old, Some(window));
        }
        leaf.image = image.clone();
        leaf.frame = Some(frame);
        if let Some(image) = image {
            let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
        }
        if ticking && leaf.armed_until.is_none_or(|until| now >= until) {
            let elapsed = leaf.elapsed(now);
            let wait = crate::controls::until_next_step(elapsed, FRAME_STEP);
            leaf.armed_until = Some(now + wait);
            let view = window.current_view();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(wait).await;
                cx.update(|cx| cx.notify(view));
            })
            .detach();
        }
    }
}

/// The view's Motion leaves by node id.
pub(crate) type MotionLeaves = RefCell<HashMap<String, Rc<RefCell<MotionLeaf>>>>;
