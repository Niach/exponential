//! Outputs (#EDE-18): the mode, scale and position an output runs at,
//! from `[[outputs]]` in the config at start and reload, and live through
//! ede-protocol's `set_output` (the Displays page, #EDE-14). A mode change
//! goes through the backend's [`crate::state::Ede::set_mode`] (udev: a
//! modeset; headless and nested: the virtual output's size); scale and
//! position change here. Fractional scale (wp-fractional-scale-v1 +
//! wp-viewporter) reaches every surface as its preferred scale.

use std::sync::Mutex;

use smithay::{
    desktop::{layer_map_for_output, Window},
    output::{Mode, Output, Scale},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle},
    wayland::{compositor::with_states, fractional_scale::{with_fractional_scale, FractionalScaleHandler}},
};

use crate::{
    layout::Rect,
    state::{set_window_state, window_state, Ede},
};

/// A change asked of one output; `None` = keep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputChange {
    pub mode: Option<(i32, i32, i32)>,
    pub scale: Option<f64>,
    pub position: Option<(i32, i32)>,
}

impl Ede {
    pub fn output_named(&self, name: &str) -> Option<Output> {
        self.desk.space.outputs().find(|o| o.name() == name).cloned()
    }

    /// The mode of `output` matching `(w, h, refresh mHz)`; refresh 0 = the
    /// highest refresh of that size.
    pub fn find_mode(output: &Output, (w, h, refresh): (i32, i32, i32)) -> Option<Mode> {
        let mut modes: Vec<Mode> = output.modes().into_iter().filter(|m| m.size.w == w && m.size.h == h).collect();
        modes.sort_by_key(|m| std::cmp::Reverse(m.refresh));
        if refresh == 0 {
            return modes.first().copied();
        }
        modes.into_iter().find(|m| (m.refresh - refresh).abs() < 500)
    }

    /// Applies `change` to the output called `name`.
    pub fn set_output(&mut self, name: &str, change: &OutputChange) -> Result<(), String> {
        let output = self.output_named(name).ok_or_else(|| format!("no output {name}"))?;
        let before = self.laid_out();
        if let Some(wanted) = change.mode {
            let mode = Self::find_mode(&output, wanted).ok_or_else(|| format!("{name} has no mode {}x{}@{}", wanted.0, wanted.1, wanted.2 as f64 / 1000.0))?;
            if output.current_mode() != Some(mode) {
                match self.set_mode.as_mut() {
                    Some(set) => set(&output, mode)?,
                    None => output.change_current_state(Some(mode), None, None, None),
                }
            }
        }
        if let Some(scale) = change.scale {
            if !(0.5..=4.0).contains(&scale) {
                return Err("scale must be between 0.5 and 4".into());
            }
            output.change_current_state(None, None, Some(Scale::Fractional(scale)), None);
        }
        if let Some(at) = change.position {
            pin_position(&output, at.into());
        }
        let at = change.position.map(Into::into).or_else(|| self.desk.space.output_geometry(&output).map(|g| g.loc)).unwrap_or_default();
        self.desk.space.map_output(&output, at);
        // #EDE-139: the rest close ranks and every window stays on its
        // output (tiled and fullscreen ones fill it again); all of them
        // learn their output's scale.
        self.relayout(&before);
        for l in layer_map_for_output(&output).layers() {
            set_surface_scale(l.wl_surface(), output.current_scale().fractional_scale());
        }
        self.output_changed(name.to_string());
        Ok(())
    }

    /// The backend changed `output`'s mode under it (hotplug: a VM window
    /// resized, a monitor's new EDID): [`Self::relayout`].
    pub fn output_resized(&mut self, output: &Output) {
        let before = self.laid_out();
        self.relayout(&before);
        self.output_changed(output.name());
    }

    /// Where each output was last laid out, in the space's order: what
    /// [`Self::relayout`] moves windows along from (the backend may have
    /// changed a mode under it already).
    pub(crate) fn laid_out(&self) -> Vec<(Output, Rect)> {
        self.desk
            .space
            .outputs()
            .filter_map(|o| {
                let laid = o.user_data().get::<LaidOut>().and_then(|l| *l.0.lock().unwrap());
                Some((o.clone(), laid.or_else(|| self.desk.space.output_geometry(o))?))
            })
            .collect()
    }

    /// Maps `output` at `at` in the space and remembers where it went.
    pub(crate) fn place_output(&mut self, output: &Output, at: Point<i32, Logical>) {
        self.desk.space.map_output(output, at);
        let laid = self.desk.space.output_geometry(output);
        output.user_data().insert_if_missing_threadsafe(LaidOut::default);
        *output.user_data().get::<LaidOut>().unwrap().0.lock().unwrap() = laid;
    }

    /// After a display change (`before` = [`Self::laid_out`] until then):
    /// the outputs without a configured or set position close ranks left
    /// to right, every output's layers re-arrange, and each window stays
    /// on its output (#EDE-139): it goes along as the output moves, a tiled
    /// or fullscreen one fills it again, one it no longer holds (it shrank)
    /// comes back inside it. The windows of an output that went away (or on
    /// none) reopen on the first one. Nobody is raised or activated.
    pub(crate) fn relayout(&mut self, before: &[(Output, Rect)]) {
        let mut outputs: Vec<Output> = self.desk.space.outputs().cloned().collect();
        outputs.sort_by_key(|o| self.desk.space.output_geometry(o).map(|g| g.loc.x).unwrap_or(0));
        let mut x = 0;
        for o in &outputs {
            let at = self.pinned_position(o).unwrap_or_else(|| (x, 0).into());
            self.place_output(o, at);
            layer_map_for_output(o).arrange();
            if let Some(g) = self.desk.space.output_geometry(o) {
                x = x.max(g.loc.x + g.size.w);
            }
        }
        let mut lost = Vec::new();
        for w in self.desk.space.elements().cloned().collect::<Vec<_>>() {
            let Some(geo) = self.desk.space.element_geometry(&w) else { continue };
            let Some((output, delta)) = self.output_before(before, geo) else {
                lost.push(w);
                continue;
            };
            let mut st = window_state(&w);
            st.restore = st.restore.map(|r| self.place_after(&w, r, before, &output));
            st.fullscreen = st.fullscreen.map(|r| self.place_after(&w, r, before, &output));
            set_window_state(&w, st);
            if st.fullscreen.is_some() {
                self.fit_fullscreen(&w, &output);
            } else if let Some(tile) = st.tile {
                self.tile_on(&w, tile, &output);
            } else {
                let to = self.moved_along(&w, geo, delta, &output);
                if to.loc != geo.loc {
                    self.relocate_window(&w, to.loc);
                }
            }
        }
        // Parked (another workspace) and minimized windows keep their place
        // on their output as it moves; a tiled or fullscreen one waits at
        // its output's corner and fills it as it comes back
        // ([`Ede::map_parked`]). On an output that went away they have no
        // place: they reopen on the first one.
        for w in self.desk.workspaces.parked.iter().chain(&self.desk.minimized).cloned().collect::<Vec<_>>() {
            let mut st = window_state(&w);
            let Some(at) = st.parked_at else { continue };
            let was = Rectangle::new(at, w.geometry().size);
            st.parked_at = self.output_before(before, was).and_then(|(output, delta)| {
                st.restore = st.restore.map(|r| self.place_after(&w, r, before, &output));
                st.fullscreen = st.fullscreen.map(|r| self.place_after(&w, r, before, &output));
                if st.tile.is_some() || st.fullscreen.is_some() {
                    return self.desk.space.output_geometry(&output).map(|g| g.loc);
                }
                Some(self.moved_along(&w, was, delta, &output).loc)
            });
            set_window_state(&w, st);
        }
        self.rehome_lost_windows(&lost);
        for w in self.desk.space.elements().cloned().collect::<Vec<_>>() {
            self.send_preferred_scale(&w);
        }
    }

    /// The output `r`'s centre was on before the change, when it is still
    /// there, and how far it moved.
    fn output_before(&self, before: &[(Output, Rect)], r: Rect) -> Option<(Output, Point<i32, Logical>)> {
        let center = r.loc.to_f64() + r.size.to_f64().to_point().downscale(2.0);
        match before.iter().find(|(_, g)| g.to_f64().contains(center)) {
            Some((output, was)) => Some((output.clone(), self.desk.space.output_geometry(output)?.loc - was.loc)),
            // On no output as they were laid out (a backend resized one
            // and never said so): the one it is on now.
            None => self.desk.space.output_under(center).next().map(|o| (o.clone(), (0, 0).into())),
        }
    }

    /// Where `r`, a place `window` goes back to (leaving a tile or
    /// fullscreen), is after the change: along with its own output, or a
    /// free spot on `home` (the window's output) when that one went away.
    fn place_after(&self, window: &Window, r: Rect, before: &[(Output, Rect)], home: &Output) -> Rect {
        match self.output_before(before, r) {
            Some((output, delta)) => self.moved_along(window, r, delta, &output),
            None => self.free_spot(window, r.size, home).map_or(r, |loc| Rectangle::new(loc, r.size)),
        }
    }

    /// Where `output` stays whatever the others do: `[[outputs]]`
    /// `position`, else one `set_output` gave it.
    fn pinned_position(&self, output: &Output) -> Option<Point<i32, Logical>> {
        let configured = self.config.output(&output.name()).and_then(|s| s.position).map(|[x, y]| Point::from((x, y)));
        configured.or_else(|| output.user_data().get::<SetPosition>().and_then(|p| *p.0.lock().unwrap()))
    }

    /// Re-applies every `[[outputs]]` entry (a config reload).
    pub fn apply_output_settings(&mut self) {
        let names: Vec<String> = self.desk.space.outputs().map(|o| o.name()).collect();
        for name in names {
            let Some(settings) = self.config.output(&name).cloned() else { continue };
            let change = OutputChange { mode: settings.parsed_mode(), scale: settings.scale, position: settings.position.map(|[x, y]| (x, y)) };
            if let Err(e) = self.set_output(&name, &change) {
                tracing::warn!("output {name}: {e}");
            }
        }
    }

    /// `(w, h, refresh)` the config asks for `name`, if a mode is set.
    pub fn configured_mode(&self, name: &str) -> Option<(i32, i32, i32)> {
        self.config.output(name)?.parsed_mode()
    }
}

/// Where an output was last laid out ([`Ede::laid_out`]).
#[derive(Default)]
struct LaidOut(Mutex<Option<Rect>>);

/// A position `set_output` gave an output: it stays there as the others
/// change, as a configured one does.
#[derive(Default)]
struct SetPosition(Mutex<Option<Point<i32, Logical>>>);

fn pin_position(output: &Output, at: Point<i32, Logical>) {
    output.user_data().insert_if_missing_threadsafe(SetPosition::default);
    *output.user_data().get::<SetPosition>().unwrap().0.lock().unwrap() = Some(at);
}

pub fn set_surface_scale(surface: &WlSurface, scale: f64) {
    with_states(surface, |states| with_fractional_scale(states, |f| f.set_preferred_scale(scale)));
}

impl FractionalScaleHandler for Ede {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // #EDE-126: a space's surface gets its own output's scale, never
        // the person's first output's (1 once its space is gone).
        if let Some(id) = crate::spaces::space_of_surface(&surface) {
            let scale = self.spaces.get(id).map_or(1.0, |space| space.output.current_scale().fractional_scale());
            return set_surface_scale(&surface, scale);
        }
        // The output under the window, else the first one.
        let scale = self
            .window_for_surface(&surface)
            .and_then(|w| self.desk.space.element_geometry(&w))
            .and_then(|g| self.output_at(g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0)))
            .or_else(|| self.desk.space.outputs().next().cloned())
            .map(|o| o.current_scale().fractional_scale())
            .unwrap_or(1.0);
        set_surface_scale(&surface, scale);
    }
}
