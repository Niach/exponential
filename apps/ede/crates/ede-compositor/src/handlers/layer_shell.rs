//! wlr-layer-shell: the panel, wallpaper, OSD and notifications of the
//! future `exponential-shell` are layer surfaces; exclusive zones shrink the work
//! area windows place and tile into.

use smithay::{
    desktop::{layer_map_for_output, LayerSurface, WindowSurfaceType},
    output::Output,
    reexports::wayland_server::protocol::{wl_output, wl_surface::WlSurface},
    utils::{IsAlive, Serial, SERIAL_COUNTER},
    wayland::{
        compositor::{get_parent, with_states},
        seat::WaylandFocus,
        shell::wlr_layer::{KeyboardInteractivity, Layer, LayerSurface as WlrLayerSurface, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState},
    },
};

use crate::{desk::DeskId, state::Ede};

/// The keyboard a layer surface holds (#EDE-71): an Exclusive one takes it
/// once, with its first buffer, and whatever had it before gets it back
/// when the layer closes.
#[derive(Default)]
struct LayerFocus {
    taken: std::sync::atomic::AtomicBool,
    previous: std::sync::Mutex<Option<WlSurface>>,
}

fn layer_focus(layer: &LayerSurface) -> &LayerFocus {
    layer.user_data().insert_if_missing_threadsafe(LayerFocus::default);
    layer.user_data().get::<LayerFocus>().unwrap()
}

/// An Exclusive top or overlay layer (a polkit, consent or share prompt)
/// that drew.
fn shows_exclusive(layer: &LayerSurface) -> bool {
    layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
        && matches!(layer.layer(), Layer::Top | Layer::Overlay)
        && smithay::backend::renderer::utils::with_renderer_surface_state(layer.wl_surface(), |s| s.buffer().is_some()).unwrap_or(false)
}

/// A sealed prompt (#EDE-107): an Exclusive layer of `exponential-sealed`
/// (polkit, consent, share) that drew. It holds the keyboard, so it is the
/// one layer a fullscreen window does not hide: drawn above it, and the
/// press on it is its own.
pub(crate) fn is_sealed_prompt(layer: &LayerSurface) -> bool {
    crate::sealed::layer_origin(layer).sealed && shows_exclusive(layer)
}

fn root_of(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

impl Ede {
    /// The layer surface `surface` (or the surface its subsurface hangs off) is.
    fn layer_for(&self, surface: &WlSurface) -> Option<LayerSurface> {
        let root = root_of(surface);
        self.desk.space.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer = map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL).cloned();
            layer
        })
    }

    /// The Exclusive prompt that holds the keyboard, if one does (#EDE-82:
    /// no window takes it then). A prompt's popup is the prompt's.
    pub(crate) fn exclusive_prompt(&self) -> Option<LayerSurface> {
        let focus = self.desk.seat.get_keyboard()?.current_focus().filter(|f| f.alive())?;
        let root = self.popups.find_popup(&focus).and_then(|p| smithay::desktop::find_popup_root_surface(&p).ok()).unwrap_or(focus);
        self.layer_for(&root).filter(shows_exclusive)
    }

    /// The window that gets the keyboard back when that prompt closes.
    pub(crate) fn prompt_previous_window(&self) -> Option<smithay::desktop::Window> {
        let previous = layer_focus(&self.exclusive_prompt()?).previous.lock().unwrap().clone()?;
        self.desk.space.elements().find(|w| w.wl_surface().is_some_and(|s| *s == previous)).cloned()
    }

    /// The keyboard goes to `surface`; when it is a layer's, the layer
    /// remembers what had the keyboard and gives it back as it closes.
    pub(crate) fn focus_layer(&mut self, surface: WlSurface, serial: Serial) {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        if let Some(layer) = self.layer_for(&surface) {
            let previous = keyboard.current_focus().filter(|p| p.alive());
            let own = previous.as_ref().is_some_and(|p| self.layer_for(p).as_ref() == Some(&layer));
            if !own {
                *layer_focus(&layer).previous.lock().unwrap() = previous;
            }
        }
        keyboard.set_focus(self, Some(surface), serial);
    }

    /// Unlocked: the keyboard goes back to the topmost window, then to an
    /// Exclusive prompt that is open (it mapped behind the lock, or had the
    /// keyboard before it): what is typed next is the prompt's. `lock_pid`
    /// is the sealed lock client, whose own surface is about to close.
    pub(crate) fn focus_after_unlock(&mut self, lock_pid: Option<u32>) {
        let top = self.desk.space.elements().last().cloned();
        self.focus(DeskId::PERSON, top);
        let prompt = self.desk.space.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let found = [Layer::Overlay, Layer::Top].into_iter().find_map(|on| map.layers_on(on).rev().find(|l| Some(crate::sealed::layer_origin(l).pid) != lock_pid && shows_exclusive(l)).cloned());
            found
        });
        if let Some(layer) = prompt {
            layer_focus(&layer).taken.store(true, std::sync::atomic::Ordering::Relaxed);
            self.focus_layer(layer.wl_surface().clone(), SERIAL_COUNTER.next_serial());
        }
    }

    /// A layer that held the keyboard closed: back to what had it before
    /// (a window, a shell surface), else to the topmost window.
    fn restore_focus_after_layer(&mut self, layer: &LayerSurface) {
        let previous = layer_focus(layer).previous.lock().unwrap().take();
        if self.is_locked() {
            return;
        }
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        let held = keyboard.current_focus().is_some_and(|f| !f.alive() || root_of(&f) == *layer.wl_surface());
        if !held {
            return;
        }
        let previous = previous.filter(|p| p.alive());
        let window = previous.as_ref().and_then(|p| self.desk.space.elements().find(|w| w.wl_surface().is_some_and(|s| *s == *p)).cloned());
        match (window, previous) {
            (Some(window), _) => self.focus(DeskId::PERSON, Some(window)),
            (None, Some(surface)) if self.layer_takes_keyboard(&surface) => keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial()),
            _ => {
                let top = self.desk.space.elements().last().cloned();
                self.focus(DeskId::PERSON, top);
            }
        }
    }
}

impl WlrLayerShellHandler for Ede {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(&mut self, surface: WlrLayerSurface, wl_output: Option<wl_output::WlOutput>, _layer: Layer, namespace: String) {
        let Some(output) = wl_output.as_ref().and_then(Output::from_resource).or_else(|| self.desk.space.outputs().next().cloned()) else {
            // No output yet: nothing can show it.
            surface.send_close();
            return;
        };
        let layer = LayerSurface::new(surface, namespace);
        let origin = crate::sealed::origin_of(self, layer.wl_surface());
        layer.user_data().insert_if_missing(|| origin);
        let mut map = layer_map_for_output(&output);
        if let Err(e) = map.map_layer(&layer) {
            tracing::warn!("layer surface refused: {e}");
        }
        drop(map);
        if matches!(&self.lock, crate::lock::LockState::Sealed(s) if s.pid == origin.pid) {
            self.focus_sealed_lock();
        }
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let found = self.desk.space.outputs().find_map(|o| {
            let map = layer_map_for_output(o);
            let layer = map.layers().find(|l| l.layer_surface() == &surface).cloned();
            layer.map(|l| (o.clone(), l))
        });
        if let Some((output, layer)) = found {
            let mut map = layer_map_for_output(&output);
            let zone = map.non_exclusive_zone();
            map.unmap_layer(&layer);
            let zone_changed = map.non_exclusive_zone() != zone;
            drop(map);
            if zone_changed {
                self.refit_tiles(&output);
            }
            self.restore_focus_after_layer(&layer);
        }
    }
}

pub fn handle_commit(state: &mut Ede, surface: &WlSurface) {
    let Some(output) = state
        .desk.space
        .outputs()
        .find(|o| layer_map_for_output(o).layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).is_some())
        .cloned()
    else {
        return;
    };
    let initial_configure_sent =
        with_states(surface, |states| states.data_map.get::<LayerSurfaceData>().unwrap().lock().unwrap().initial_configure_sent);
    let mut map = layer_map_for_output(&output);
    // Arrange first so the initial configure carries the size the client asked for.
    let zone = map.non_exclusive_zone();
    map.arrange();
    let zone_changed = map.non_exclusive_zone() != zone;
    if !initial_configure_sent {
        if let Some(layer) = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL) {
            layer.layer_surface().send_configure();
        }
    }
    // An Exclusive top or overlay surface (a polkit, consent or share
    // prompt) takes the keyboard as it shows: what is typed right away is
    // never the window's behind it. The lock keeps its own focus: a prompt
    // that maps behind it takes the keyboard at unlock.
    let layer = map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL).cloned();
    drop(map);
    // #EDE-139: a panel's exclusive zone came or changed (the taskbar can
    // map after a window maximized at login, or come back after a shell
    // restart): the tiled windows fill the new work area.
    if zone_changed {
        state.refit_tiles(&output);
    }
    let Some(layer) = layer else { return };
    if !shows_exclusive(&layer) {
        return;
    }
    if state.is_locked() {
        if state.is_lock_surface(surface) {
            layer_focus(&layer).taken.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    } else if !layer_focus(&layer).taken.swap(true, std::sync::atomic::Ordering::Relaxed) {
        state.focus_layer(surface.clone(), SERIAL_COUNTER.next_serial());
    }
}
