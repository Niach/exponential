//! Cinnamon's screenshot keys: Print the whole screen, Shift+Print an
//! area the person drags (the compositor's pick), Alt+Print the focused
//! window; with Ctrl to the clipboard instead of a file. A file goes where
//! gnome-screenshot (what Cinnamon runs) puts it: the Pictures folder,
//! `Screenshot from 2026-11-26 16-53-05.png` in the person's language
//! (gnome-screenshot's own translation), `… - 2.png` beside an existing one.
//!
//! The person's Cinnamon settings decide first, as on Cinnamon: a custom
//! keybinding on the same key runs its command instead (Flameshot's
//! `flameshot gui` on Print, say), and a media key the person cleared does
//! nothing.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use ede_protocol::{shot, CaptureTarget, Client, ClipboardItem, PickMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Screen,
    Area,
    Window,
}

/// The compositor's hotkey name → `(what, to the clipboard)`.
pub fn from_hotkey(name: &str) -> Option<(What, bool)> {
    Some(match name {
        "screenshot" => (What::Screen, false),
        "screenshot.clip" => (What::Screen, true),
        "screenshot.area" => (What::Area, false),
        "screenshot.area-clip" => (What::Area, true),
        "screenshot.window" => (What::Window, false),
        "screenshot.window-clip" => (What::Window, true),
        _ => return None,
    })
}

/// Cinnamon's media-keys setting and default accelerator for a key.
fn media_key(what: What, clip: bool) -> (&'static str, &'static str) {
    match (what, clip) {
        (What::Screen, false) => ("screenshot", "Print"),
        (What::Screen, true) => ("screenshot-clip", "<Control>Print"),
        (What::Area, false) => ("area-screenshot", "<Shift>Print"),
        (What::Area, true) => ("area-screenshot-clip", "<Control><Shift>Print"),
        (What::Window, false) => ("window-screenshot", "<Alt>Print"),
        (What::Window, true) => ("window-screenshot-clip", "<Control><Alt>Print"),
    }
}

/// What the key does.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// A Cinnamon custom keybinding's command.
    Command(String),
    /// Ours.
    Take,
    /// The person cleared the binding.
    Nothing,
}

/// `<Control><Shift>Print` as a comparable set: modifier names as GTK
/// parses them (`<Ctrl>`, `<Primary>` = Control), any order, any case.
fn accel_key(accel: &str) -> Option<(Vec<&'static str>, String)> {
    let mut mods = Vec::new();
    let mut rest = accel.trim();
    while let Some(after) = rest.strip_prefix('<') {
        let (name, tail) = after.split_once('>')?;
        mods.push(match name.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "primary" => "control",
            "shift" => "shift",
            "alt" | "mod1" => "alt",
            "super" | "mod4" => "super",
            _ => return None,
        });
        rest = tail;
    }
    mods.sort_unstable();
    mods.dedup();
    let key = match rest.to_ascii_lowercase().as_str() {
        "" => return None,
        "sys_req" => "print".to_string(),
        k => k.to_string(),
    };
    Some((mods, key))
}

fn same_accel(a: &str, b: &str) -> bool {
    matches!((accel_key(a), accel_key(b)), (Some(x), Some(y)) if x == y)
}

/// Cinnamon's answer from its settings: `customs` = every custom
/// keybinding's `(bindings, command)`, `media` = the media key's bindings
/// (`None` = no Cinnamon settings: ours).
pub fn decide(what: What, clip: bool, customs: &[(Vec<String>, String)], media: Option<&[String]>) -> Action {
    let accel = media_key(what, clip).1;
    if let Some((_, command)) = customs.iter().find(|(bindings, command)| !command.trim().is_empty() && bindings.iter().any(|b| same_accel(b, accel))) {
        return Action::Command(command.clone());
    }
    match media {
        Some(bindings) if !bindings.iter().any(|b| same_accel(b, accel)) => Action::Nothing,
        _ => Action::Take,
    }
}

/// A GVariant text value as `gsettings get` prints it: `['a', "b'c"]`,
/// `@as []` or `'x'` → its strings.
fn gvariant_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\'' && c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(d) = chars.next() {
            match d {
                '\\' => {
                    if let Some(e) = chars.next() {
                        s.push(e);
                    }
                }
                d if d == c => break,
                d => s.push(d),
            }
        }
        out.push(s);
    }
    out
}

fn gsettings(args: &[&str]) -> Option<String> {
    let out = Command::new("gsettings").arg("get").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

const KEYBINDINGS: &str = "org.cinnamon.desktop.keybindings";

/// [`decide`] on the person's Cinnamon settings (none installed: ours).
pub fn cinnamon_action(what: What, clip: bool) -> Action {
    let media = gsettings(&[&format!("{KEYBINDINGS}.media-keys"), media_key(what, clip).0]).map(|v| gvariant_strings(&v));
    let customs: Vec<(Vec<String>, String)> = gsettings(&[KEYBINDINGS, "custom-list"])
        .map(|v| gvariant_strings(&v))
        .unwrap_or_default()
        .iter()
        .filter_map(|id| {
            let schema = format!("{KEYBINDINGS}.custom-keybinding:/org/cinnamon/desktop/keybindings/custom-keybindings/{id}/");
            let bindings = gvariant_strings(&gsettings(&[&schema, "binding"])?);
            let command = gvariant_strings(&gsettings(&[&schema, "command"])?).into_iter().next()?;
            Some((bindings, command))
        })
        .collect();
    decide(what, clip, &customs, media.as_deref())
}

/// The picture: `None` = nothing to take (the pick was cancelled, no
/// focused window, a protected one).
pub fn take(ede: &mut Client, what: What) -> Result<Option<(usize, usize, Vec<u8>)>, String> {
    let e = |e: ede_protocol::Error| e.to_string();
    if what == What::Window {
        let windows = ede.windows().map_err(e)?;
        let Some(w) = windows.iter().find(|w| w.focused && !w.protected && w.space.is_none()) else { return Ok(None) };
        let c = ede.capture(CaptureTarget::Window { window: w.id.clone() }).map_err(e)?;
        let (width, height) = (c.info.width.max(0) as usize, c.info.height.max(0) as usize);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let p = c.at(x, y);
                rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
            }
        }
        return Ok(Some((width, height, rgba)));
    }
    let region = match what {
        What::Area => match ede.pick_region(PickMode::Area).map_err(e)? {
            Some(r) => Some(shot::Rect { x: r.x, y: r.y, width: r.width, height: r.height }),
            None => return Ok(None),
        },
        _ => None,
    };
    let outputs: Vec<_> = ede.outputs().map_err(e)?.into_iter().filter(|o| o.is_persons()).collect();
    let mut shots = Vec::new();
    for o in &outputs {
        shots.push(ede.capture(CaptureTarget::Output { name: o.name.clone() }).map_err(e)?);
    }
    Ok(shot::compose(&outputs, &shots, region))
}

pub fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut w| w.write_image_data(rgba)).map_err(|e| e.to_string())?;
    Ok(png)
}

/// The Pictures folder as the person named it (`xdg-user-dir`, ~/Bilder on
/// a German desktop), else ~/Pictures, else home.
pub fn pictures_dir() -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let named = Command::new("xdg-user-dir").arg("PICTURES").output().ok().map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())).filter(|p| p.is_absolute() && *p != home && p.is_dir());
    named.or_else(|| Some(home.join("Pictures")).filter(|p| p.is_dir())).unwrap_or(home)
}

/// gnome-screenshot's file name pattern in the person's language (its
/// catalogue), English without one.
fn pattern(msgid: &str) -> String {
    Command::new("gettext")
        .env("TEXTDOMAIN", "gnome-screenshot")
        .arg(msgid)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .filter(|p| p.matches("%s").count() == msgid.matches("%s").count() && p.contains("%d") == msgid.contains("%d"))
        .unwrap_or_else(|| msgid.to_string())
}

/// `Screenshot from <stamp>.png`, or `… - 2.png` up from there while one exists.
pub fn free_path(dir: &Path, stamp: &str, first: &str, nth: &str) -> PathBuf {
    let path = dir.join(first.replacen("%s", stamp, 1).replacen("%s", "png", 1));
    if !path.exists() {
        return path;
    }
    (2..)
        .map(|n| dir.join(nth.replacen("%s", stamp, 1).replacen("%d", &n.to_string(), 1).replacen("%s", "png", 1)))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Writes the picture to the Pictures folder, gnome-screenshot's name.
pub fn save(png: &[u8]) -> Result<PathBuf, String> {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();
    let path = free_path(&pictures_dir(), &stamp, &pattern("Screenshot from %s.%s"), &pattern("Screenshot from %s - %d.%s"));
    std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The picture on the clipboard (`image/png`), as gnome-screenshot's
/// Ctrl variants put it.
pub fn copy(ede: &mut Client, png: Vec<u8>) -> Result<(), String> {
    ede.clipboard_set(vec![ClipboardItem { mime: "image/png".into(), data: png }], false).map(|_| ()).map_err(|e| e.to_string())
}

/// The shutter sound gnome-screenshot plays (the sound theme's
/// `screen-capture`), when the person has a player for it.
pub fn shutter() {
    let _ = Command::new("canberra-gtk-play").args(["-i", "screen-capture", "-d", "screenshot"]).spawn();
}

/// One key: take, then save or copy. `Ok(None)` = nothing taken.
pub fn run(ede: &mut Client, what: What, clip: bool) -> Result<Option<Option<PathBuf>>, String> {
    let Some((width, height, rgba)) = take(ede, what)? else { return Ok(None) };
    let png = encode_png(width, height, &rgba)?;
    if clip {
        copy(ede, png)?;
        return Ok(Some(None));
    }
    save(&png).map(|p| Some(Some(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkeys_name_cinnamons_six_keys() {
        assert_eq!(from_hotkey("screenshot"), Some((What::Screen, false)));
        assert_eq!(from_hotkey("screenshot.area-clip"), Some((What::Area, true)));
        assert_eq!(from_hotkey("screenshot.window"), Some((What::Window, false)));
        assert_eq!(from_hotkey("volume.up"), None);
    }

    /// Danny's mint: Print and Shift+Print run Flameshot (custom
    /// keybindings), area-screenshot cleared, the Ctrl variants default.
    #[test]
    fn cinnamon_settings_decide_first() {
        let customs = vec![(vec!["Print".to_string()], "flameshot gui".to_string()), (vec!["<Shift>Print".to_string()], "flameshot gui -d 5000".to_string())];
        let cleared: Vec<String> = vec![];
        assert_eq!(decide(What::Screen, false, &customs, Some(&cleared)), Action::Command("flameshot gui".into()));
        assert_eq!(decide(What::Area, false, &customs, Some(&cleared)), Action::Command("flameshot gui -d 5000".into()));
        assert_eq!(decide(What::Screen, true, &customs, Some(&["<Control>Print".to_string()])), Action::Take);
        assert_eq!(decide(What::Window, false, &[], Some(&cleared)), Action::Nothing, "a cleared binding does nothing");
        assert_eq!(decide(What::Window, true, &[], Some(&["<Primary><Alt>Print".to_string()])), Action::Take, "GTK's other spelling of Control");
        assert_eq!(decide(What::Window, false, &[], None), Action::Take, "no Cinnamon: ours");
    }

    #[test]
    fn gsettings_values_read_back() {
        assert_eq!(gvariant_strings("['custom0', 'custom1']"), ["custom0", "custom1"]);
        assert_eq!(gvariant_strings("@as []"), Vec::<String>::new());
        assert_eq!(gvariant_strings(r#"'sh -c "echo \'hi\'"'"#), [r#"sh -c "echo 'hi'""#]);
        assert_eq!(gvariant_strings(r#"["it's"]"#), ["it's"]);
    }

    #[test]
    fn names_follow_gnome_screenshot() {
        let dir = tempfile::tempdir().unwrap();
        let (first, nth) = ("Bildschirmfoto vom %s.%s", "Bildschirmfoto vom %s - %d.%s");
        let a = free_path(dir.path(), "2026-11-26 16-53-05", first, nth);
        assert_eq!(a.file_name().unwrap(), "Bildschirmfoto vom 2026-11-26 16-53-05.png");
        std::fs::write(&a, b"").unwrap();
        let b = free_path(dir.path(), "2026-11-26 16-53-05", first, nth);
        assert_eq!(b.file_name().unwrap(), "Bildschirmfoto vom 2026-11-26 16-53-05 - 2.png");
    }
}
