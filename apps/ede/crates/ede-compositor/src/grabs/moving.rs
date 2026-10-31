//! Moving a window: magnetic edges while dragging, edge tiling on release,
//! and a tiled window pulled off its tile goes back to its floating size.
//! The gesture passthroughs are smithay's smallvil example (MIT, Copyright
//! (c) 2017 Victor Berger and Smithay contributors).

use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData, MotionEvent, PointerGrab,
        PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point},
};

use crate::{layout, state::{window_state, Ede}};

/// How far (px) a tiled window must be dragged before it leaves its tile.
const UNTILE_PX: f64 = 8.0;

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<Ede>,
    pub window: Window,
    pub initial_window_location: Point<i32, Logical>,
}

impl MoveSurfaceGrab {
    pub fn new(start_data: PointerGrabStartData<Ede>, window: Window, initial_window_location: Point<i32, Logical>) -> Self {
        Self { start_data, window, initial_window_location }
    }
}

impl PointerGrab<Ede> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        let delta = event.location - self.start_data.location;
        if window_state(&self.window).tile.is_some() {
            if delta.x.hypot(delta.y) < UNTILE_PX {
                return;
            }
            let Some(tiled) = data.space.element_geometry(&self.window) else { return };
            let restore = data.restore_size(&self.window).unwrap_or(tiled.size);
            let loc = layout::untile_location(event.location, tiled, restore);
            data.untile(&self.window, Some(loc));
            self.initial_window_location = loc;
            self.start_data.location = event.location;
            return;
        }

        let new_location = (self.initial_window_location.to_f64() + delta).to_i32_round();
        let threshold = data.config.windows.snap_threshold;
        let snapped = match (data.space.element_geometry(&self.window), data.output_at(event.location).and_then(|o| data.work_area(&o))) {
            (Some(geo), Some(area)) if threshold > 0 => {
                let others: Vec<_> = data
                    .space
                    .elements()
                    .filter(|w| *w != &self.window)
                    .filter_map(|w| data.space.element_geometry(w))
                    .collect();
                let rect = smithay::utils::Rectangle::new(new_location, geo.size);
                layout::magnetic(rect, area, &others, threshold)
            }
            _ => new_location,
        };
        data.space.map_element(self.window.clone(), snapped, true);
    }

    fn relative_motion(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>, event: &ButtonEvent) {
        handle.button(data, event);
        if !handle.current_pressed().contains(&super::BTN_LEFT) {
            let pointer = handle.current_location();
            handle.unset_grab(self, data, event.serial, event.time, true);
            let w = &data.config.windows;
            if w.edge_tiling && window_state(&self.window).tile.is_none() {
                let (edge, corner) = (w.edge_tiling_px, w.corner_px);
                if let Some(tile) = data
                    .output_at(pointer)
                    .and_then(|o| data.work_area(&o))
                    .and_then(|area| layout::tile_zone(pointer, area, edge, corner))
                {
                    data.tile(&self.window.clone(), tile);
                }
            }
        }
    }

    fn axis(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut Ede, handle: &mut PointerInnerHandle<'_, Ede>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event)
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event)
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event)
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event)
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event)
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event)
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event)
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut Ede,
        handle: &mut PointerInnerHandle<'_, Ede>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<Ede> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut Ede) {}
}
