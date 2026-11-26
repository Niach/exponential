//! Display scaling beyond native Wayland clients (#EDE-250).
//!
//! Wayland clients learn their output's scale from `wl_output.scale` and
//! wp-fractional-scale ([`crate::outputs`]). Two things did not follow it:
//!
//! - **An output nobody configured** stayed at 100 % even on a 5K monitor,
//!   so everything was tiny. [`default_scale`] picks 200 % for a HiDPI
//!   output, Mutter's (and Muffin's) rule: at least 192 dpi both ways and
//!   at least 1200 px tall.
//! - **X11 apps** (Steam, Electron and Chromium in X11 mode) drew at 1 and
//!   were stretched: big but blurry. As KWin and Mutter's native Xwayland
//!   scaling do, Xwayland now sees every output at its full resolution
//!   (smithay's client scale: X11 coordinates are physical pixels of the
//!   most scaled output) and its windows are shown at that scale. X11 apps
//!   learn the scale the way they do on a HiDPI X11 desktop: XSETTINGS
//!   (`Xft/DPI`, `Gdk/WindowScalingFactor`, `Gdk/UnscaledDPI`, read by GTK,
//!   Chromium, Electron) and the `Xft.dpi` and `Xcursor.size` resources plus
//!   `_XWAYLAND_GLOBAL_OUTPUT_SCALE` on the root window (Qt, Xlib apps,
//!   Steam). No `GDK_SCALE` or `QT_SCALE_FACTOR` goes into the session's
//!   environment: they would scale native Wayland Qt and GTK windows a
//!   second time and stay fixed when the person changes the scale.

use smithay::{output::Output, xwayland::xwm::settings::Value};

use crate::state::Ede;

/// Mutter's `HIDPI_LIMIT`: dots per inch from which an output is HiDPI.
const HIDPI_LIMIT: f64 = 192.0;
/// Mutter's `HIDPI_MIN_HEIGHT`: a shorter mode is never HiDPI.
const HIDPI_MIN_HEIGHT: i32 = 1200;
/// A cursor's size at 100 % ([`crate::backend::cursor`] reads the same).
const CURSOR_SIZE: u32 = 24;

/// The scale an output with no configured one starts at: 2 for a HiDPI
/// panel (`physical_mm` across and down, as EDID says), else 1. Sizes an
/// EDID gives for projectors and TVs (an aspect ratio in cm, or nothing)
/// say nothing about density and keep 1.
pub fn default_scale(mode: (i32, i32), physical_mm: (i32, i32)) -> f64 {
    let (w, h) = mode;
    let (mm_w, mm_h) = physical_mm;
    if h < HIDPI_MIN_HEIGHT || mm_w <= 0 || mm_h <= 0 {
        return 1.0;
    }
    // Mutter's list of sizes that are aspect ratios, not sizes.
    if matches!((mm_w, mm_h), (160, 90) | (160, 100) | (16, 9) | (16, 10)) {
        return 1.0;
    }
    let dpi_x = w as f64 / (mm_w as f64 / 25.4);
    let dpi_y = h as f64 / (mm_h as f64 / 25.4);
    if dpi_x >= HIDPI_LIMIT && dpi_y >= HIDPI_LIMIT {
        2.0
    } else {
        1.0
    }
}

/// The scale X11 apps draw at: the largest output scale (sharp where it
/// counts, scaled down elsewhere), 1 without native scaling. Never below 1:
/// an output at 75 % shows X11 windows scaled down, never X at a lower DPI.
pub fn x11_scale<'a>(native: bool, scales: impl Iterator<Item = f64>) -> f64 {
    if !native {
        return 1.0;
    }
    scales.fold(1.0_f64, f64::max)
}

/// What X11 apps are told at `scale`: the XSETTINGS GTK's
/// xsettings daemon would set (gnome-settings-daemon's arithmetic: an
/// integer window scale, the rest of the scale in the font DPI).
pub fn xsettings(scale: f64) -> Vec<(String, Value)> {
    let window_scale = scale.floor().max(1.0);
    let dpi = |v: f64| Value::Integer((v * 1024.0).round() as i32);
    vec![
        ("Xft/DPI".to_string(), dpi(96.0 * scale)),
        ("Gdk/WindowScalingFactor".to_string(), Value::Integer(window_scale as i32)),
        ("Gdk/UnscaledDPI".to_string(), dpi(96.0 * scale / window_scale)),
    ]
}

/// The X resources at `scale`, merged into `existing` (the root window's
/// `RESOURCE_MANAGER`, what an `xrdb -merge` of the person's left): our two
/// lines replace theirs, the rest stays.
pub fn resources(existing: &str, scale: f64, cursor_size: u32) -> String {
    let mut out: Vec<String> = existing
        .lines()
        .filter(|l| {
            let key = l.split(':').next().unwrap_or("").trim();
            !key.is_empty() && key != "Xft.dpi" && key != "Xcursor.size"
        })
        .map(String::from)
        .collect();
    out.push(format!("Xft.dpi:\t{}", (96.0 * scale).round() as i64));
    out.push(format!("Xcursor.size:\t{}", (cursor_size as f64 * scale).round() as i64));
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// `_XWAYLAND_GLOBAL_OUTPUT_SCALE`: an integer, as Mutter sets it.
pub fn global_output_scale(scale: f64) -> u32 {
    scale.round().max(1.0) as u32
}

/// The person's cursor size at 100 % (`$XCURSOR_SIZE`, the theme bridge's).
fn cursor_size() -> u32 {
    std::env::var("XCURSOR_SIZE").ok().and_then(|s| s.parse().ok()).filter(|s| *s > 0).unwrap_or(CURSOR_SIZE)
}

impl Ede {
    /// The scale an output starts at: its `[[outputs]]` one, else
    /// [`default_scale`] (with `[display] auto_scale`), else 1.
    pub fn initial_scale(&self, output: &Output) -> Option<f64> {
        if let Some(scale) = self.config.output(&output.name()).and_then(|s| s.scale) {
            return Some(scale);
        }
        if !self.config.display.auto_scale {
            return None;
        }
        let mode = output.preferred_mode().or_else(|| output.current_mode())?;
        let size = output.physical_properties().size;
        let scale = default_scale((mode.size.w, mode.size.h), (size.w, size.h));
        (scale != 1.0).then_some(scale)
    }

    /// The person's X11 scale now ([`x11_scale`] over their outputs).
    pub fn person_x11_scale(&self) -> f64 {
        x11_scale(self.config.display.xwayland_native_scaling, self.desk.space.outputs().map(|o| o.current_scale().fractional_scale()))
    }

    /// After an output came, went or changed scale (and when Xwayland
    /// became ready): the person's Xwayland sees the outputs at the new X11
    /// scale, its windows keep their size on screen and draw at the new
    /// resolution, and X11 apps are told the scale.
    pub fn update_x11_scale(&mut self) {
        #[cfg(feature = "xwayland")]
        {
            let scale = self.person_x11_scale();
            let Some(client) = self.person_x11.client.clone() else { return };
            let Some(data) = client.get_data::<smithay::xwayland::XWaylandClientData>() else { return };
            let changed = (data.compositor_state.client_scale() - scale).abs() > f64::EPSILON;
            if changed {
                data.compositor_state.set_client_scale(scale);
                // Every wl_output (and xdg_output) Xwayland bound is sent
                // again in its new coordinates.
                for output in self.desk.space.outputs() {
                    output.change_current_state(None, None, None, None);
                }
                // Each X11 window keeps its logical size: X learns its new
                // size in pixels.
                let windows: Vec<smithay::xwayland::X11Surface> = self
                    .desk
                    .space
                    .elements()
                    .chain(self.desk.workspaces.parked.iter())
                    .chain(self.desk.minimized.iter())
                    .filter_map(|w| w.x11_surface().cloned())
                    .filter(|x| !x.is_override_redirect())
                    .collect();
                for x in windows {
                    let _ = x.configure(x.geometry());
                }
            }
            if self.person_x11.told_scale == Some(scale) {
                return;
            }
            self.person_x11.told_scale = Some(scale);
            tracing::info!("X11 apps draw at {scale}×");
            if let Some(xwm) = self.xwm.as_mut() {
                if let Err(e) = xwm.set_xsettings(xsettings(scale).into_iter()) {
                    tracing::warn!("XSETTINGS: {e:?}");
                }
            }
            if let (Some(display), Some(cookie)) = (self.xdisplay, self.person_x11.cookie) {
                set_root_resources(display, cookie, scale);
            }
        }
    }
}

/// Writes `Xft.dpi`/`Xcursor.size` into the root window's
/// `RESOURCE_MANAGER` and `_XWAYLAND_GLOBAL_OUTPUT_SCALE`, on a connection
/// of its own on a thread of its own: Xwayland is our client, so a blocking
/// round trip on the compositor's loop could wait for itself.
#[cfg(feature = "xwayland")]
fn set_root_resources(number: u32, cookie: [u8; 16], scale: f64) {
    let spawned = std::thread::Builder::new().name("x11-scale".into()).spawn(move || {
        if let Err(e) = write_root_resources(number, cookie, scale) {
            tracing::warn!("X resources on :{number}: {e}");
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("X resources: {e}");
    }
}

#[cfg(feature = "xwayland")]
fn write_root_resources(number: u32, cookie: [u8; 16], scale: f64) -> Result<(), Box<dyn std::error::Error>> {
    use smithay::reexports::x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt, PropMode},
        rust_connection::{DefaultStream, RustConnection},
        wrapper::ConnectionExt as _,
    };
    let path = format!("/tmp/.X11-unix/X{number}");
    let stream = DefaultStream::from_unix_stream(std::os::unix::net::UnixStream::connect(&path)?)?.0;
    let conn = RustConnection::connect_to_stream_with_auth_info(stream, 0, b"MIT-MAGIC-COOKIE-1".to_vec(), cookie.to_vec())?;
    let root = conn.setup().roots[0].root;
    let existing = conn.get_property(false, root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, 0, 1 << 20)?.reply()?;
    let text = resources(&String::from_utf8_lossy(&existing.value), scale, cursor_size());
    conn.change_property8(PropMode::REPLACE, root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, text.as_bytes())?;
    let atom = conn.intern_atom(false, b"_XWAYLAND_GLOBAL_OUTPUT_SCALE")?.reply()?.atom;
    conn.change_property32(PropMode::REPLACE, root, atom, AtomEnum::CARDINAL, &[global_output_scale(scale)])?;
    conn.sync()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_5k_27_inch_panel_starts_at_200_percent() {
        // Danny's monitors: 5120×2880 on 597×336 mm (218 dpi).
        assert_eq!(default_scale((5120, 2880), (597, 336)), 2.0);
        // A 4K 15.6" laptop panel (282 dpi).
        assert_eq!(default_scale((3840, 2160), (344, 194)), 2.0);
    }

    #[test]
    fn ordinary_and_unknown_outputs_stay_at_100_percent() {
        assert_eq!(default_scale((1920, 1080), (527, 296)), 1.0, "24\" 1080p");
        assert_eq!(default_scale((3840, 2160), (697, 392)), 1.0, "32\" 4K, 139 dpi");
        assert_eq!(default_scale((2560, 1440), (0, 0)), 1.0, "no EDID size (a VM)");
        assert_eq!(default_scale((5120, 2880), (160, 90)), 1.0, "an aspect ratio, not a size");
        assert_eq!(default_scale((2560, 1080), (100, 40)), 1.0, "under 1200 px tall");
    }

    #[test]
    fn x11_follows_the_most_scaled_output_never_below_one() {
        assert_eq!(x11_scale(true, [1.0, 2.0, 1.5].into_iter()), 2.0);
        assert_eq!(x11_scale(true, [1.25].into_iter()), 1.25);
        assert_eq!(x11_scale(true, [0.75].into_iter()), 1.0);
        assert_eq!(x11_scale(true, std::iter::empty()), 1.0);
        assert_eq!(x11_scale(false, [2.0].into_iter()), 1.0, "native scaling off: stretched");
    }

    #[test]
    fn xsettings_split_the_scale_like_gnome_settings_daemon() {
        let get = |s: &[(String, Value)], k: &str| s.iter().find(|(n, _)| n == k).map(|(_, v)| format!("{v:?}"));
        let int = |v: i32| Some(format!("{:?}", Value::Integer(v)));
        let two = xsettings(2.0);
        assert_eq!(get(&two, "Gdk/WindowScalingFactor"), int(2));
        assert_eq!(get(&two, "Xft/DPI"), int(192 * 1024));
        assert_eq!(get(&two, "Gdk/UnscaledDPI"), int(96 * 1024));
        let one_half = xsettings(1.5);
        assert_eq!(get(&one_half, "Gdk/WindowScalingFactor"), int(1));
        assert_eq!(get(&one_half, "Xft/DPI"), int(144 * 1024));
        assert_eq!(get(&one_half, "Gdk/UnscaledDPI"), int(144 * 1024));
    }

    #[test]
    fn resources_replace_ours_and_keep_the_persons() {
        let text = resources("Xft.dpi:\t96\nXTerm*faceName:\tMono\nXcursor.size:\t24\n", 1.5, 24);
        assert_eq!(text, "XTerm*faceName:\tMono\nXft.dpi:\t144\nXcursor.size:\t36\n");
        assert_eq!(resources("", 2.0, 32), "Xft.dpi:\t192\nXcursor.size:\t64\n");
        assert_eq!(global_output_scale(1.5), 2);
        assert_eq!(global_output_scale(1.0), 1);
    }
}
