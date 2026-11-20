//! #EDE-125: a space's frame clock and its capture. Nothing draws a space
//! per frame: a commit arms one tick, the tick asks the damage tracker
//! whether the scene changed (drawing nothing and importing nothing,
//! [`damage`], #EDE-162), counts that in the space's serial and sends the
//! frame callbacks. An idle space arms nothing. A capture is drawn when it
//! is asked for, in software, on the thread that serves the person's
//! frames: hence the size limit, the slow clock of a space nobody looks
//! at, and the captures' turns after the person's frames ([`queue`]).

mod damage;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod net;
pub mod pace;
#[cfg(feature = "gpu")]
mod passes;
mod queue;
mod stats;

pub use queue::{BUDGET, FLOOR, WINDOW};
pub use stats::PROBE;
#[cfg(feature = "gpu")]
pub use passes::IMPORTS_PER_SECOND;

use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement, AsRenderElements, Kind},
        pixman::PixmanRenderer,
        utils::CommitCounter,
        ImportAll, ImportMem, Renderer,
    },
    desktop::Window,
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{Logical, Physical, Point, Rectangle},
};

use crate::{
    backend::{draw_offscreen, headless::Frame, OutputElements},
    desk::DeskId,
    state::Ede,
};

/// What the spaces' clock and captures keep between turns of the loop.
#[derive(Default)]
pub(crate) struct Frames {
    queue: queue::Queue,
    stats: stats::Stats,
    /// What catches a GPU app that fails ([`net`]).
    net: net::Net,
    /// Per space, the surfaces that hold a commit for a tick ([`pace`]).
    pacing: std::collections::HashMap<DeskId, Vec<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>>,
    /// The spaces' GPU renderer, the spaces captured on it, the passes on
    /// their way back ([`passes`]).
    #[cfg(feature = "gpu")]
    gpu: passes::Gpu,
    #[cfg(feature = "gpu")]
    on_gpu: std::collections::HashMap<DeskId, bool>,
    #[cfg(feature = "gpu")]
    passes: std::collections::HashMap<u64, passes::Pass>,
    #[cfg(feature = "gpu")]
    next_pass: u64,
    #[cfg(feature = "gpu")]
    sweeping: bool,
    /// The spaces' linux-dmabuf global, on the spaces' renderer, and each
    /// client's recent imports (a budget, [`passes`]).
    #[cfg(feature = "gpu")]
    dmabuf: Option<smithay::wayland::dmabuf::DmabufGlobal>,
    #[cfg(feature = "gpu")]
    imports: std::collections::HashMap<smithay::reexports::wayland_server::backend::ClientId, std::collections::VecDeque<Instant>>,
}

/// Without the `gpu` feature every space draws in software.
#[cfg(not(feature = "gpu"))]
impl Ede {
    pub fn start_space_gpu(&mut self, _safe_mode: bool) {}

    pub fn space_on_gpu(&mut self, _desk: DeskId) -> bool {
        false
    }

    fn space_gpu_busy(&self, _desk: DeskId) -> bool {
        false
    }

    fn space_gpu_capture(&mut self, _desk: DeskId, jobs: Vec<queue::Waiting>) -> Result<Duration, Vec<queue::Waiting>> {
        Err(jobs)
    }

    fn space_offers_gpu_buffers(&self) -> bool {
        false
    }

    pub(crate) fn space_gpu_driver(&self) -> Option<&str> {
        None
    }
}

/// A tick this long after a commit while the space is in use: driven or
/// captured within [`IN_USE`].
pub const FAST: Duration = Duration::from_millis(16);
/// Otherwise: an animation nobody looks at costs a tenth.
pub const SLOW: Duration = Duration::from_millis(100);
pub const IN_USE: Duration = Duration::from_secs(2);

impl Ede {
    /// Space `id` was driven or looked at: its clock runs fast for a while.
    pub(crate) fn space_used(&mut self, id: DeskId) {
        if let Some(space) = self.spaces.get_mut(id) {
            space.last_use = Some(Instant::now());
        }
    }

    /// Arms one tick of space `id`'s clock, unless one is due.
    pub(crate) fn space_schedule(&mut self, id: DeskId) {
        let Some(space) = self.spaces.get_mut(id) else { return };
        if space.scheduled {
            return;
        }
        let after = if space.last_use.is_some_and(|at| at.elapsed() < IN_USE) { FAST } else { SLOW };
        let armed = self.loop_handle.insert_source(Timer::from_duration(after), move |_, _, state| {
            state.space_tick(id);
            TimeoutAction::Drop
        });
        if let Some(space) = self.spaces.get_mut(id) {
            (space.scheduled, space.pace) = (armed.is_ok(), after);
        }
    }

    fn space_tick(&mut self, id: DeskId) {
        let cpu = stats::thread_cpu();
        self.space_tick_inner(id);
        self.spaces.frames.stats.tick(stats::thread_cpu().saturating_sub(cpu));
    }

    fn space_tick_inner(&mut self, id: DeskId) {
        // The space may have closed since the tick was armed.
        let Some(space) = self.spaces.get_mut(id) else { return };
        space.scheduled = false;
        space.ticks += 1;
        // A held commit goes through now, so its frame callbacks go with
        // this tick's (#EDE-162, [`pace`]).
        self.space_pace_tick(id);
        self.space_damage(id);
        let now = self.start_time.elapsed();
        let Some(space) = self.spaces.get_mut(id) else { return };
        let output = space.output.clone();
        for window in space.desk.space.elements() {
            window.send_frame(&output, now, None, |_, _| Some(output.clone()));
        }
        space.desk.space.refresh();
        self.popups.cleanup();
        let _ = self.display_handle.flush_clients();
    }

    /// The loop fired a timer `by` late (`EXP_FRAME_STATS=1`): the delay a
    /// person's vblank would wait behind everything else on this thread.
    pub fn spaces_loop_late(&mut self, by: std::time::Duration) {
        self.spaces.frames.stats.late(by);
    }

    /// Whether space stats are being kept (`EXP_FRAME_STATS=1`).
    pub fn spaces_stats_on(&self) -> bool {
        self.spaces.frames.stats.on
    }

    /// Did space `id`'s scene change since this was last asked? Then its
    /// serial moves. Nothing is drawn, no buffer is imported: a GPU
    /// client's commits count as a software client's do.
    fn space_damage(&mut self, id: DeskId) {
        let elements = self.space_damage_elements(id);
        if let Some(space) = self.spaces.get_mut(id) {
            if matches!(space.tracker.damage_output(1, &elements), Ok((Some(damage), _)) if !damage.is_empty()) {
                space.serial += 1;
            }
        }
    }

    /// Space `id`'s serial, counting what changed up to now.
    pub(crate) fn space_serial(&mut self, id: DeskId) -> Option<u64> {
        self.space_damage(id);
        self.spaces.get(id).map(|s| s.serial)
    }

    /// The spaces' one software renderer, taken out while it is used.
    fn space_renderer(&mut self) -> Option<PixmanRenderer> {
        self.spaces.renderer.take().or_else(|| PixmanRenderer::new().map_err(|e| tracing::warn!("no software renderer for spaces: {e}")).ok())
    }

    /// Space `id` as its output shows it: its windows and their popups on
    /// the configured background, a protected window black. No lock
    /// screen, no layers, no frames, no cursor: those are the person's.
    pub fn capture_space(&mut self, id: DeskId) -> Result<Frame, String> {
        let size = self.spaces.get(id).and_then(|s| s.output.current_mode()).map(|m| (m.size.w, m.size.h)).ok_or("no such space")?;
        self.space_used(id);
        self.space_damage(id);
        let clear = self.config.background_rgba().unwrap_or([0.04, 0.04, 0.04, 1.0]);
        let mut renderer = self.space_renderer().ok_or("no software renderer")?;
        // A space's output is always at scale 1 and its scene is built at 1
        // ([`Self::space_elements`]): the tracker's scale matches (#EDE-113).
        let mut undrawable = None;
        let frame = draw_offscreen(&mut renderer, size, 1.0, clear, |renderer| {
            let (elements, failed) = self.space_elements(id, renderer);
            undrawable = failed;
            elements
        });
        self.spaces.renderer = Some(renderer);
        if let Some(app) = undrawable {
            self.space_undrawable(id, &app);
        }
        frame
    }

    /// Front to back, in the space's output's pixels (scale 1, at (0, 0)).
    /// Also names the first window whose buffers `renderer` could not
    /// draw (#EDE-162): it is a grey placeholder, never blank.
    fn space_elements<R>(&self, id: DeskId, renderer: &mut R) -> (Vec<OutputElements<R>>, Option<String>)
    where
        R: Renderer + ImportAll + ImportMem,
        R::TextureId: Clone + 'static,
    {
        let mut elements = Vec::new();
        let mut undrawable = None;
        self.space_walk(id, |window, at, protected| {
            if protected {
                elements.push(OutputElements::Solid(protected_black(window, at)));
                return;
            }
            let drawn: Vec<WaylandSurfaceRenderElement<R>> = AsRenderElements::<R>::render_elements(window, renderer, at.to_physical(1), 1.0.into(), 1.0);
            let mut wanted = Vec::new();
            damage::window_damage(window, at.to_physical(1), &mut wanted);
            if drawn.len() < wanted.len() {
                undrawable.get_or_insert_with(|| crate::ipc::window_title(window).1.or(crate::ipc::window_title(window).0).unwrap_or_else(|| "an app".into()));
                elements.push(OutputElements::Solid(placeholder(window, at)));
                return;
            }
            elements.extend(drawn.into_iter().map(OutputElements::Surface));
        });
        (elements, undrawable)
    }

    /// The same scene as [`Self::space_elements`], as the damage tracker
    /// sees it, without a renderer.
    fn space_damage_elements(&self, id: DeskId) -> Vec<damage::SpaceDamage> {
        let mut elements = Vec::new();
        self.space_walk(id, |window, at, protected| match protected.then(|| protected_black(window, at)) {
            Some(black) => elements.push(damage::SpaceDamage::Solid(black)),
            None => damage::window_damage(window, at.to_physical(1), &mut elements),
        });
        elements
    }

    /// Space `id`'s windows front to back: `f(window, where its surface
    /// goes, whether it is protected)`.
    fn space_walk(&self, id: DeskId, mut f: impl FnMut(&Window, Point<i32, Logical>, bool)) {
        let Some(desk) = self.try_desk(id) else { return };
        for window in desk.space.elements().rev() {
            let Some(at) = desk.space.element_location(window) else { continue };
            f(window, at - window.geometry().loc, self.is_protected_window(window));
        }
    }
}

/// A window whose buffers cannot be drawn: solid grey over its box, never
/// blank ([`net::PLACEHOLDER`]).
fn placeholder(window: &Window, at: Point<i32, Logical>) -> SolidColorRenderElement {
    let mut bbox = window.bbox();
    bbox.loc += at;
    SolidColorRenderElement::new(damage::protected_id(window), bbox.to_physical(1), CommitCounter::default(), net::PLACEHOLDER, Kind::Unspecified)
}

/// A protected window: solid black over its whole box, with the same id on
/// every call ([`damage::protected_id`]).
fn protected_black(window: &Window, at: Point<i32, Logical>) -> SolidColorRenderElement {
    let mut bbox = window.bbox();
    bbox.loc += at;
    let geometry: Rectangle<i32, Physical> = bbox.to_physical(1);
    SolidColorRenderElement::new(damage::protected_id(window), geometry, CommitCounter::default(), [0.0, 0.0, 0.0, 1.0], Kind::Unspecified)
}
