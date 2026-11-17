//! #EDE-125: a space's frame clock and its capture. Nothing draws a space
//! per frame: a commit arms one tick, the tick asks the damage tracker
//! whether the scene changed (drawing nothing), counts that in the space's
//! serial and sends the frame callbacks. An idle space arms nothing. A
//! capture is drawn when it is asked for, in software, on the thread that
//! serves the person's frames: hence the size limit and the slow clock of
//! a space nobody looks at.

use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{
        element::{solid::SolidColorRenderElement, surface::WaylandSurfaceRenderElement, AsRenderElements, Id, Kind},
        pixman::PixmanRenderer,
        utils::CommitCounter,
    },
    reexports::calloop::timer::{TimeoutAction, Timer},
};

use crate::{
    backend::{draw_offscreen, headless::Frame, OutputElements},
    desk::DeskId,
    state::Ede,
};

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
        // The space may have closed since the tick was armed.
        let Some(space) = self.spaces.get_mut(id) else { return };
        space.scheduled = false;
        space.ticks += 1;
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

    /// Did space `id`'s scene change since this was last asked? Then its
    /// serial moves. Nothing is drawn.
    fn space_damage(&mut self, id: DeskId) {
        let Some(mut renderer) = self.space_renderer() else { return };
        let elements = self.space_elements(id, &mut renderer);
        if let Some(space) = self.spaces.get_mut(id) {
            if matches!(space.tracker.damage_output(1, &elements), Ok((Some(damage), _)) if !damage.is_empty()) {
                space.serial += 1;
            }
        }
        self.spaces.renderer = Some(renderer);
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
        let frame = draw_offscreen(&mut renderer, size, clear, |renderer| self.space_elements(id, renderer));
        self.spaces.renderer = Some(renderer);
        frame
    }

    /// Front to back, in the space's output's pixels (scale 1, at (0, 0)).
    fn space_elements(&self, id: DeskId, renderer: &mut PixmanRenderer) -> Vec<OutputElements<PixmanRenderer>> {
        let Some(desk) = self.try_desk(id) else { return Vec::new() };
        let mut elements = Vec::new();
        for window in desk.space.elements().rev() {
            let Some(at) = desk.space.element_location(window) else { continue };
            let at = at - window.geometry().loc;
            if self.is_protected_window(window) {
                let mut bbox = window.bbox();
                bbox.loc += at;
                elements.push(OutputElements::Solid(SolidColorRenderElement::new(Id::new(), bbox.to_physical(1), CommitCounter::default(), [0.0, 0.0, 0.0, 1.0], Kind::Unspecified)));
                continue;
            }
            elements.extend(
                AsRenderElements::<PixmanRenderer>::render_elements::<WaylandSurfaceRenderElement<PixmanRenderer>>(window, renderer, at.to_physical(1), 1.0.into(), 1.0)
                    .into_iter()
                    .map(OutputElements::Surface),
            );
        }
        elements
    }
}
