//! Outputs (#EDE-18): the mode, scale and position an output runs at,
//! from `[[outputs]]` in the config at start and reload, and live through
//! ede-protocol's `set_output` (the Displays page, #EDE-14). A mode change
//! goes through the backend's [`crate::state::Ede::set_mode`] (udev: a
//! modeset; headless and nested: the virtual output's size); scale and
//! position change here. Fractional scale (wp-fractional-scale-v1 +
//! wp-viewporter) reaches every surface as its preferred scale.

use smithay::{
    desktop::layer_map_for_output,
    output::{Mode, Output, Scale},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::{compositor::with_states, fractional_scale::{with_fractional_scale, FractionalScaleHandler}},
};

use crate::state::Ede;

/// A change asked of one output; `None` = keep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputChange {
    pub mode: Option<(i32, i32, i32)>,
    pub scale: Option<f64>,
    pub position: Option<(i32, i32)>,
}

impl Ede {
    pub fn output_named(&self, name: &str) -> Option<Output> {
        self.space.outputs().find(|o| o.name() == name).cloned()
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
        let at = change.position.map(Into::into).or_else(|| self.space.output_geometry(&output).map(|g| g.loc)).unwrap_or_default();
        self.space.map_output(&output, at);
        layer_map_for_output(&output).arrange();
        // Windows learn the new scale; tiled ones follow the work area.
        for w in self.space.elements().cloned().collect::<Vec<_>>() {
            self.send_preferred_scale(&w);
            if let Some(tile) = crate::state::window_state(&w).tile {
                self.tile(&w, tile);
            }
        }
        for l in layer_map_for_output(&output).layers() {
            set_surface_scale(l.wl_surface(), output.current_scale().fractional_scale());
        }
        self.emit(ede_protocol::events::OUTPUT_CHANGED, serde_json::json!({ "output": name }));
        Ok(())
    }

    /// The backend changed `output`'s mode under it (hotplug: a VM window
    /// resized, a monitor's new EDID): outputs without a configured position
    /// close ranks left to right, every output's layers re-arrange, tiled
    /// windows follow the work area, and windows left on no output come back.
    pub fn output_resized(&mut self, output: &Output) {
        let mut outputs: Vec<Output> = self.space.outputs().cloned().collect();
        outputs.sort_by_key(|o| self.space.output_geometry(o).map(|g| g.loc.x).unwrap_or(0));
        let mut x = 0;
        for o in &outputs {
            let at = match self.config.output(&o.name()).and_then(|s| s.position) {
                Some([px, py]) => (px, py),
                None => (x, 0),
            };
            self.space.map_output(o, at);
            layer_map_for_output(o).arrange();
            if let Some(g) = self.space.output_geometry(o) {
                x = x.max(g.loc.x + g.size.w);
            }
        }
        for w in self.space.elements().cloned().collect::<Vec<_>>() {
            if let Some(tile) = crate::state::window_state(&w).tile {
                self.tile(&w, tile);
            }
        }
        self.rehome_lost_windows(&[]);
        self.emit(ede_protocol::events::OUTPUT_CHANGED, serde_json::json!({ "output": output.name() }));
    }

    /// Re-applies every `[[outputs]]` entry (a config reload).
    pub fn apply_output_settings(&mut self) {
        let names: Vec<String> = self.space.outputs().map(|o| o.name()).collect();
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

pub fn set_surface_scale(surface: &WlSurface, scale: f64) {
    with_states(surface, |states| with_fractional_scale(states, |f| f.set_preferred_scale(scale)));
}

impl FractionalScaleHandler for Ede {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        // The output under the window, else the first one.
        let scale = self
            .window_for_surface(&surface)
            .and_then(|w| self.space.element_geometry(&w))
            .and_then(|g| self.output_at(g.loc.to_f64() + g.size.to_f64().to_point().downscale(2.0)))
            .or_else(|| self.space.outputs().next().cloned())
            .map(|o| o.current_scale().fractional_scale())
            .unwrap_or(1.0);
        set_surface_scale(&surface, scale);
    }
}
