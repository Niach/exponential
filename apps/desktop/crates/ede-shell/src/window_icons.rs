//! #EDE-242: what an X11 window says about its app when no desktop entry
//! names its class, read from Xwayland like Cinnamon's window tracker reads
//! it: `_NET_WM_PID` (a Steam game's process carries `SteamAppId`, so Dota
//! 2, class `dota2`, finds Steam's `Dota 2.desktop`), else `_NET_WM_ICON`,
//! the window's own picture (Cinnamon's "window-backed app"). Read once per
//! class over a lazy connection to `$DISPLAY`; no display = no hints.

use std::{collections::HashMap, path::PathBuf};

use x11rb::{
    connection::Connection,
    protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window},
    rust_connection::RustConnection,
};

/// One class's hints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hint {
    /// `SteamAppId` of the window's process.
    pub steam_game: Option<String>,
    /// `_NET_WM_ICON` as a PNG file.
    pub icon: Option<PathBuf>,
}

#[derive(Default)]
pub struct X11Hints {
    conn: Option<Conn>,
    /// Tried to connect and failed: never again.
    unavailable: bool,
    cache: HashMap<String, Hint>,
}

struct Conn {
    c: RustConnection,
    root: Window,
    client_list: Atom,
    pid: Atom,
    icon: Atom,
}

impl X11Hints {
    pub fn cached(&self, class: &str) -> Option<&Hint> {
        self.cache.get(class)
    }

    /// The class's hints, read on the first ask; `None` without Xwayland or
    /// without a window of that class (asked again next time).
    pub fn lookup(&mut self, class: &str) -> Option<&Hint> {
        if !self.cache.contains_key(class) {
            let hint = self.read(class)?;
            self.cache.insert(class.to_string(), hint);
        }
        self.cache.get(class)
    }

    fn conn(&mut self) -> Option<&Conn> {
        if self.conn.is_none() && !self.unavailable {
            self.conn = std::env::var_os("DISPLAY").filter(|d| !d.is_empty()).and_then(|_| Conn::open().map_err(|e| tracing::debug!("window icons: no X display: {e}")).ok());
            self.unavailable = self.conn.is_none();
        }
        self.conn.as_ref()
    }

    fn read(&mut self, class: &str) -> Option<Hint> {
        let conn = self.conn()?;
        let read = conn.read(class);
        if read.is_err() {
            // Xwayland restarted: reconnect on the next ask.
            self.conn = None;
        }
        let window = read.ok()??;
        let conn = self.conn.as_ref()?;
        let steam_game = conn.cardinals(window, conn.pid, 1).and_then(|p| p.first().copied()).and_then(uicore_data::apps::steam_game_of_pid);
        let icon = conn.cardinals(window, conn.icon, 4 * 1024 * 1024).and_then(|data| best_icon(&data, 48)).and_then(|(w, h, rgba)| write_png(class, w, h, &rgba));
        Some(Hint { steam_game, icon })
    }
}

impl Conn {
    fn open() -> Result<Conn, Box<dyn std::error::Error>> {
        let (c, screen) = x11rb::connect(None)?;
        let root = c.setup().roots[screen].root;
        let atom = |name: &[u8]| -> Result<Atom, Box<dyn std::error::Error>> { Ok(c.intern_atom(false, name)?.reply()?.atom) };
        let (client_list, pid, icon) = (atom(b"_NET_CLIENT_LIST")?, atom(b"_NET_WM_PID")?, atom(b"_NET_WM_ICON")?);
        Ok(Conn { c, root, client_list, pid, icon })
    }

    /// The managed window whose `WM_CLASS` class is `class`.
    fn read(&self, class: &str) -> Result<Option<Window>, Box<dyn std::error::Error>> {
        let list = self.c.get_property(false, self.root, self.client_list, AtomEnum::WINDOW, 0, 4096)?.reply()?;
        for w in list.value32().into_iter().flatten() {
            let Ok(reply) = self.c.get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?.reply() else { continue };
            if wm_class(&reply.value).1.as_deref() == Some(class) {
                return Ok(Some(w));
            }
        }
        Ok(None)
    }

    fn cardinals(&self, w: Window, prop: Atom, len: u32) -> Option<Vec<u32>> {
        let reply = self.c.get_property(false, w, prop, AtomEnum::CARDINAL, 0, len).ok()?.reply().ok()?;
        reply.value32().map(Iterator::collect)
    }
}

/// `WM_CLASS`: instance and class, NUL-separated.
fn wm_class(raw: &[u8]) -> (Option<String>, Option<String>) {
    let mut parts = raw.split(|b| *b == 0).map(|p| String::from_utf8_lossy(p).into_owned());
    (parts.next().filter(|s| !s.is_empty()), parts.next().filter(|s| !s.is_empty()))
}

/// The `_NET_WM_ICON` picture nearest `want` px (the smallest at least that
/// big, else the biggest), as straight RGBA: the property is `width,
/// height, pixels…` repeated, each pixel premultiplied-free ARGB in a CARDINAL.
pub fn best_icon(data: &[u32], want: u32) -> Option<(u32, u32, Vec<u8>)> {
    let mut icons: Vec<(u32, u32, &[u32])> = Vec::new();
    let mut at = 0usize;
    while at + 2 <= data.len() {
        let (w, h) = (data[at], data[at + 1]);
        let n = (w as usize).checked_mul(h as usize)?;
        if w == 0 || h == 0 || at + 2 + n > data.len() {
            break;
        }
        icons.push((w, h, &data[at + 2..at + 2 + n]));
        at += 2 + n;
    }
    let pick = icons.iter().filter(|i| i.0 >= want).min_by_key(|i| i.0).or_else(|| icons.iter().max_by_key(|i| i.0))?;
    let rgba = pick.2.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
    Some((pick.0, pick.1, rgba))
}

fn write_png(class: &str, w: u32, h: u32, rgba: &[u8]) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("exponential-shell/window-icons");
    std::fs::create_dir_all(&dir).ok()?;
    let safe: String = class.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect();
    let path = dir.join(format!("{safe}.png"));
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().ok()?.write_image_data(rgba).ok()?;
    }
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_smallest_icon_at_least_as_big_as_wanted() {
        let mut data = vec![2, 2];
        data.extend([0xff00_0000u32; 4]);
        data.extend([64, 64]);
        data.extend(std::iter::repeat(0x80ff_0000u32).take(64 * 64));
        data.extend([48, 48]);
        data.extend(std::iter::repeat(0xff00_ff00u32).take(48 * 48));
        let (w, h, rgba) = best_icon(&data, 48).unwrap();
        assert_eq!((w, h), (48, 48));
        assert_eq!(&rgba[..4], &[0x00, 0xff, 0x00, 0xff], "ARGB to RGBA");
        let (w, _, rgba) = best_icon(&data, 128).unwrap();
        assert_eq!(w, 64, "none big enough: the biggest");
        assert_eq!(&rgba[..4], &[0xff, 0x00, 0x00, 0x80]);
        assert_eq!(best_icon(&[16, 16, 1, 2], 48), None, "truncated");
    }

    #[test]
    fn reads_the_class_out_of_wm_class() {
        assert_eq!(wm_class(b"steamwebhelper\0steam\0"), (Some("steamwebhelper".into()), Some("steam".into())));
        assert_eq!(wm_class(b"dota2\0dota2\0").1.as_deref(), Some("dota2"));
        assert_eq!(wm_class(b""), (None, None));
    }
}
