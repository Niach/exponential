//! Linux/BSD X11 taskbar icon: set `_NET_WM_ICON` on our windows (EXP-79).
//!
//! EXP-68 already registers `<APP_ID>.desktop` + a hicolor icon and stamps the
//! window's WM_CLASS, but that association only renders once the desktop shell
//! has re-indexed the just-written files. On the FIRST launch of a fresh
//! install/update the window maps milliseconds after `desktop_integration`
//! writes them, the shell still resolves the window to a nameless
//! "window-backed" app, and every window-backed surface (Cinnamon's
//! grouped-window-list, muffin/mutter alt-tab, xfce4-panel, …) falls back to
//! the window's own `_NET_WM_ICON` — which gpui never sets — so the user
//! stares at the generic gear for the whole session. Setting the property
//! ourselves closes that gap: EWMH shells re-read `_NET_WM_ICON` on
//! PropertyNotify and refresh in place, so even a write that lands after the
//! window was mapped (and after the shell drew the gear) heals it live.
//!
//! gpui exposes no window-icon API at the pinned rev (and we never fork gpui
//! — the property is plain EWMH, no gpui internals involved). Both deps ride
//! versions already in the tree via gpui itself: `x11rb` (gpui_linux's X11
//! backend) and `resvg` (gpui's SVG renderer). Wayland has no per-window icon
//! protocol at our pinned stack — there the `.desktop` match is the only
//! path, so we skip entirely (Wayland shells re-match on their next desktop
//! re-index). Best-effort on a background thread; never blocks startup.

use std::time::Duration;

use anyhow::Context as _;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, PropMode, Window};
use x11rb::wrapper::ConnectionExt as _;

use crate::channel::APP_ID;
use crate::desktop_integration::ICON_SVG;

/// Sizes rasterized into the property (EWMH allows several; shells pick the
/// nearest and scale). 128 covers HiDPI taskbars and alt-tab tiles, and
/// capping there keeps the payload well under the core-protocol request-size
/// limit (a 256px frame alone would exceed it and lean on BIG-REQUESTS).
const ICON_SIZES: [u32; 3] = [32, 64, 128];

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const POLL_ROUNDS: usize = 100;
/// Rounds before "every matching window is iconed" may end the poll: the
/// window whose `open_shell_window` spawned us maps a beat AFTER the
/// spawn, and exiting on the sight of older windows' icons would strand it.
const MIN_ROUNDS: usize = 20;

/// Delays for the re-stamp passes after the initial stamping. Each stamp
/// fires a PropertyNotify → the shell re-reads the window icon and re-runs
/// its per-window icon pick — including the themed `Icon=` lookup that FAILED
/// on first launch because GtkIconTheme re-stats its dirs at most every ~5s
/// (Cinnamon's grouped-window-list caches that failure as the generic gear
/// until something re-triggers it, verified live on Cinnamon 6.6). By these
/// offsets the throttle has lapsed and the theme sees the icon
/// `desktop_integration` just installed, so the button heals to the logo.
const RESTAMP_DELAYS: [Duration; 2] = [Duration::from_secs(6), Duration::from_secs(8)];

/// Fire-and-forget: watch for our X11 windows, stamp `_NET_WM_ICON` on any
/// that lack it, then re-stamp once GtkIconTheme's rescan throttle has lapsed
/// (RESTAMP_DELAYS). Call once per `open_shell_window`; threads are
/// idempotent (initial pass skips already-iconed windows) and expire after
/// ~25s.
pub fn install() {
    // gpui prefers the Wayland backend whenever a compositor is reachable —
    // then our window is no X11 client and there is nothing to stamp.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("x11-window-icon".into())
        .spawn(|| {
            if let Err(err) = run() {
                eprintln!("[exp-desktop] X11 window icon: {err:#}");
            }
        });
}

fn run() -> anyhow::Result<()> {
    let icon = rasterize_net_wm_icon(ICON_SVG, &ICON_SIZES)?;
    let (conn, screen) = x11rb::connect(None).context("X11 connect")?;
    let root = conn.setup().roots[screen].root;
    let net_client_list = intern(&conn, "_NET_CLIENT_LIST")?;
    let net_wm_icon = intern(&conn, "_NET_WM_ICON")?;

    // Phase 1: wait for our window(s) to map and stamp any that lack an icon.
    let mut found = false;
    for round in 0..POLL_ROUNDS {
        let mut matching = 0usize;
        for window in client_list(&conn, root, net_client_list) {
            if !wm_class_matches(&conn, window) {
                continue;
            }
            matching += 1;
            if has_icon(&conn, window, net_wm_icon) {
                continue;
            }
            stamp(&conn, window, net_wm_icon, &icon)?;
        }
        if matching > 0 && round + 1 >= MIN_ROUNDS {
            found = true;
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    if !found {
        return Ok(());
    }

    // Phase 2: re-stamp (same payload) to nudge the shell into re-picking the
    // icon once GtkIconTheme's rescan throttle has lapsed — see RESTAMP_DELAYS.
    for delay in RESTAMP_DELAYS {
        std::thread::sleep(delay);
        for window in client_list(&conn, root, net_client_list) {
            if wm_class_matches(&conn, window) {
                stamp(&conn, window, net_wm_icon, &icon)?;
            }
        }
    }
    Ok(())
}

fn stamp(
    conn: &impl Connection,
    window: Window,
    net_wm_icon: Atom,
    icon: &[u32],
) -> anyhow::Result<()> {
    conn.change_property32(PropMode::REPLACE, window, net_wm_icon, AtomEnum::CARDINAL, icon)
        .context("change _NET_WM_ICON")?
        .check()
        .context("set _NET_WM_ICON")?;
    Ok(())
}

fn intern(conn: &impl Connection, name: &str) -> anyhow::Result<Atom> {
    Ok(conn
        .intern_atom(false, name.as_bytes())
        .with_context(|| format!("intern {name}"))?
        .reply()
        .with_context(|| format!("intern {name} reply"))?
        .atom)
}

/// The WM's list of managed windows; empty on any failure (window races and
/// exotic WMs just mean we retry next round or give up quietly).
fn client_list(conn: &impl Connection, root: Window, atom: Atom) -> Vec<Window> {
    conn.get_property(false, root, atom, AtomEnum::WINDOW, 0, 4096)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32().map(|values| values.collect()))
        .unwrap_or_default()
}

/// WM_CLASS is `instance\0class\0`; ours carries `APP_ID` in both slots (set
/// via gpui's `WindowOptions.app_id`, EXP-68). Matching either slot also
/// tolerates a window that has vanished mid-poll (error ⇒ no match).
fn wm_class_matches(conn: &impl Connection, window: Window) -> bool {
    conn.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| {
            reply
                .value
                .split(|byte| *byte == 0)
                .any(|part| part == APP_ID.as_bytes())
        })
}

fn has_icon(conn: &impl Connection, window: Window, net_wm_icon: Atom) -> bool {
    conn.get_property(false, window, net_wm_icon, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|reply| reply.value_len > 0)
}

/// Rasterize the logo into the `_NET_WM_ICON` payload: for each size a
/// `[width, height, pixel…]` run of 32-bit **straight-alpha** ARGB (EWMH wants
/// unpremultiplied; tiny-skia renders premultiplied, hence the demultiply).
fn rasterize_net_wm_icon(svg: &[u8], sizes: &[u32]) -> anyhow::Result<Vec<u32>> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .context("parse icon svg")?;
    let mut payload =
        Vec::with_capacity(sizes.iter().map(|s| (s * s + 2) as usize).sum());
    for &size in sizes {
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(size, size).context("icon pixmap alloc")?;
        let transform = resvg::tiny_skia::Transform::from_scale(
            size as f32 / tree.size().width(),
            size as f32 / tree.size().height(),
        );
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        payload.push(size);
        payload.push(size);
        payload.extend(pixmap.pixels().iter().map(|pixel| {
            let c = pixel.demultiply();
            (u32::from(c.alpha()) << 24)
                | (u32::from(c.red()) << 16)
                | (u32::from(c.green()) << 8)
                | u32::from(c.blue())
        }));
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::{rasterize_net_wm_icon, ICON_SIZES, ICON_SVG};

    #[test]
    fn payload_carries_size_headers_and_frames() {
        let payload = rasterize_net_wm_icon(ICON_SVG, &[8, 16]).unwrap();
        assert_eq!(payload.len(), (2 + 64) + (2 + 256));
        assert_eq!(&payload[0..2], &[8, 8]);
        assert_eq!(&payload[66..68], &[16, 16]);
    }

    #[test]
    fn logo_renders_visible_and_transparent_pixels() {
        let payload = rasterize_net_wm_icon(ICON_SVG, &[64]).unwrap();
        let pixels = &payload[2..];
        // The white disc must produce fully-opaque white (0xffffffff — also
        // proves the alpha is straight, not premultiplied), and the corners
        // outside the disc must stay fully transparent.
        assert!(pixels.contains(&0xffff_ffff), "no opaque white pixel");
        assert_eq!(pixels[0] >> 24, 0, "corner pixel should be transparent");
    }

    #[test]
    fn shipped_sizes_stay_under_the_core_request_size_cap() {
        // A core-protocol X11 request tops out at 262140 bytes; blowing past
        // it would make ChangeProperty depend on the BIG-REQUESTS extension.
        let payload = rasterize_net_wm_icon(ICON_SVG, &ICON_SIZES).unwrap();
        assert!(payload.len() * 4 < 240_000, "payload too large: {}", payload.len() * 4);
    }
}
