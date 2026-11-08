//! `~/.config/exponential-desktop/compositor.toml`: the compositor's own settings.
//!
//! Every key is optional; a missing file is the defaults. A file that fails
//! to parse is reported and ignored (the session still starts), so a typo
//! never locks the person out of their desktop.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub keyboard: Keyboard,
    pub windows: Windows,
    /// The colour behind every window until the shell draws a wallpaper,
    /// `#rrggbb`. Defaults to the design-token `background` (oklch 0.145).
    pub background: String,
    /// Run X11 apps through Xwayland.
    pub xwayland: bool,
    pub lock: Lock,
    pub idle: Idle,
    pub protected: Protected,
    /// Per-output settings (#EDE-18): `[[outputs]]` with the connector name
    /// (`DP-1`, `HEADLESS-1`); an output not listed takes its preferred
    /// mode, scale 1 and the next place to the right.
    pub outputs: Vec<OutputSettings>,
    /// `[diagnostics] send`: crash, performance and log reports to the
    /// signed-in server (#LOGS-2; ede-observability reads it, Settings ›
    /// About writes it).
    pub diagnostics: Diagnostics,
    /// `[theme]`: the bridge's (#EDE-19, exponential-theme reads it), kept
    /// here only so the Theme page's writes do not make this file invalid.
    pub theme: toml::Table,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Diagnostics {
    /// On by default; `EXP_DIAGNOSTICS=0` turns it off too.
    pub send: bool,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self { send: true }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputSettings {
    pub name: String,
    /// `"2560x1440@60"` (the refresh is optional: `"2560x1440"`).
    pub mode: Option<String>,
    /// 1.0, 1.25, 1.5, 2.0…; the logical size is the mode divided by it.
    pub scale: Option<f64>,
    /// `[x, y]` in logical px.
    pub position: Option<[i32; 2]>,
}

impl OutputSettings {
    /// `(width, height, refresh in mHz)` of `mode`, refresh 0 = any.
    pub fn parsed_mode(&self) -> Option<(i32, i32, i32)> {
        parse_mode(self.mode.as_deref()?)
    }
}

/// `WxH[@R]` → `(w, h, refresh mHz)`; `R` may carry decimals (`59.94`).
pub fn parse_mode(text: &str) -> Option<(i32, i32, i32)> {
    let (size, refresh) = match text.split_once('@') {
        Some((s, r)) => (s, r.parse::<f64>().ok()?),
        None => (text, 0.0),
    };
    let (w, h) = size.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?, (refresh * 1000.0).round() as i32))
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lock {
    /// An ext-session-lock client for Super+L, idle and suspend, e.g.
    /// `["swaylock", "-f"]`. Empty (the default) = eDE's own sealed lock
    /// screen, `exponential-sealed lock` (#EDE-17).
    pub command: Vec<String>,
    /// Lock before the machine suspends (logind PrepareForSleep) and on
    /// `loginctl lock-session` (udev backend).
    pub on_suspend: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Idle {
    /// Seconds without input before the lock command runs; 0 = never.
    /// Idle inhibitors (a video playing) hold it off.
    pub lock_after: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Protected {
    /// More app ids (X11: WM_CLASS) that are never captured, lent or driven,
    /// on top of the built-in list (polkit, keyring and password prompts,
    /// password managers).
    pub app_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Keyboard {
    /// xkb layout, e.g. `"de"` or `"us,de"`; empty = the system's
    /// (`/etc/default/keyboard`, else xkb's own default, `us`).
    pub layout: String,
    pub variant: String,
    pub options: String,
    /// ms before a held key repeats.
    pub repeat_delay: i32,
    /// repeats per second.
    pub repeat_rate: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Windows {
    /// Window edges within this many px of a screen edge or another window
    /// snap flush while moving. 0 turns it off.
    pub snap_threshold: i32,
    /// Dropping a window at a screen edge tiles it (sides = half, corners =
    /// quarter, top = maximize).
    pub edge_tiling: bool,
    /// How close (px) the pointer must be to an edge to tile.
    pub edge_tiling_px: i32,
    /// Size (px) of the corner zones along each edge.
    pub corner_px: i32,
    /// The IDE look (#EDE-48): windows sit on a panel inside a frame, so
    /// the work area (maximize, tiling, Super+Up/Left/Right, new windows)
    /// is the output minus exclusive layer zones, inset by this many px on
    /// every side. 0 = flush with the panel and the screen edges.
    pub panel_margin: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyboard: Keyboard::default(),
            windows: Windows::default(),
            background: "#0a0a0a".into(),
            xwayland: true,
            lock: Lock::default(),
            idle: Idle::default(),
            protected: Protected::default(),
            outputs: Vec::new(),
            diagnostics: Diagnostics::default(),
            theme: toml::Table::new(),
        }
    }
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            layout: String::new(),
            variant: String::new(),
            options: String::new(),
            repeat_delay: 400,
            repeat_rate: 30,
        }
    }
}

impl Default for Lock {
    fn default() -> Self {
        Self { command: Vec::new(), on_suspend: true }
    }
}

impl Default for Idle {
    fn default() -> Self {
        Self { lock_after: 600 }
    }
}

impl Default for Windows {
    fn default() -> Self {
        Self { snap_threshold: 12, edge_tiling: true, edge_tiling_px: 4, corner_px: 120, panel_margin: 10 }
    }
}

/// Debian's and Ubuntu's keyboard file (`keyboard(5)`): what the console
/// and X11 use, so a German keyboard types German at the lock screen too.
pub const SYSTEM_KEYBOARD: &str = "/etc/default/keyboard";

/// `XKBLAYOUT="de"`, `XKBVARIANT=""`, `XKBOPTIONS="..."` lines (quotes
/// optional, `#` comments) → a [`Keyboard`] with those three; `None` when
/// the file names no layout.
pub fn parse_system_keyboard(text: &str) -> Option<Keyboard> {
    let value = |key: &str| {
        text.lines().map(str::trim).filter(|l| !l.starts_with('#')).find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
    };
    let layout = value("XKBLAYOUT").filter(|l| !l.is_empty())?;
    Some(Keyboard { layout, variant: value("XKBVARIANT").unwrap_or_default(), options: value("XKBOPTIONS").unwrap_or_default(), ..Keyboard::default() })
}

impl Keyboard {
    /// An empty layout takes `system`'s layout, variant and (when empty)
    /// options; a set layout is left alone.
    pub fn or_system(mut self, system: Option<&Keyboard>) -> Self {
        if let (true, Some(system)) = (self.layout.is_empty(), system) {
            self.layout = system.layout.clone();
            self.variant = system.variant.clone();
            if self.options.is_empty() {
                self.options = system.options.clone();
            }
        }
        self
    }
}

impl Config {
    /// The system's keyboard ([`SYSTEM_KEYBOARD`]) for an empty `[keyboard]
    /// layout`: every path that loads a config goes through here (start,
    /// safe mode, reload).
    pub fn with_system_keyboard(mut self) -> Self {
        if self.keyboard.layout.is_empty() {
            let system = std::fs::read_to_string(SYSTEM_KEYBOARD).ok().and_then(|t| parse_system_keyboard(&t));
            self.keyboard = std::mem::take(&mut self.keyboard).or_system(system.as_ref());
        }
        self
    }

    /// `$XDG_CONFIG_HOME/exponential-desktop/compositor.toml`, else `~/.config/exponential-desktop/compositor.toml`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("exponential-desktop").join("compositor.toml"))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.background_rgba()?;
        if cfg.windows.panel_margin < 0 {
            return Err("[windows] panel_margin must be 0 or more".into());
        }
        for o in &cfg.outputs {
            if o.name.is_empty() {
                return Err("[[outputs]] needs a name".into());
            }
            if o.mode.as_deref().is_some_and(|m| parse_mode(m).is_none()) {
                return Err(format!("output {}: mode must be WxH or WxH@R, got {:?}", o.name, o.mode));
            }
            if o.scale.is_some_and(|s| !(0.5..=4.0).contains(&s)) {
                return Err(format!("output {}: scale must be between 0.5 and 4", o.name));
            }
        }
        Ok(cfg)
    }

    /// The settings for output `name`, if listed.
    pub fn output(&self, name: &str) -> Option<&OutputSettings> {
        self.outputs.iter().find(|o| o.name == name)
    }

    /// Reads `path`; missing = defaults, unreadable or invalid = defaults plus
    /// the reason (logged by the caller).
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::parse(&text) {
                Ok(cfg) => (cfg, None),
                Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    pub fn background_rgba(&self) -> Result<[f32; 4], String> {
        let hex = self.background.strip_prefix('#').unwrap_or(&self.background);
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("background must be #rrggbb, got {:?}", self.background));
        }
        let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f32 / 255.0;
        Ok([ch(0), ch(2), ch(4), 1.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn keys_override_only_what_they_name() {
        let cfg = Config::parse("background = \"#102030\"\n[windows]\nsnap_threshold = 0\n[keyboard]\nlayout = \"de\"\n").unwrap();
        assert_eq!(cfg.windows.snap_threshold, 0);
        assert!(cfg.windows.edge_tiling);
        assert_eq!(cfg.windows.panel_margin, 10, "the IDE look's default");
        assert_eq!(Config::parse("[windows]\npanel_margin = 0\n").unwrap().windows.panel_margin, 0);
        assert!(Config::parse("[windows]\npanel_margin = -1\n").is_err());
        assert_eq!(cfg.keyboard.layout, "de");
        assert_eq!(cfg.keyboard.repeat_rate, 30);
        let [r, g, b, a] = cfg.background_rgba().unwrap();
        assert_eq!(((r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round(), a), (16.0, 32.0, 48.0, 1.0));
    }

    #[test]
    fn diagnostics_are_on_unless_switched_off() {
        assert!(Config::default().diagnostics.send);
        assert!(!Config::parse("[diagnostics]\nsend = false\n").unwrap().diagnostics.send);
    }

    #[test]
    fn the_theme_pages_table_does_not_invalidate_the_file() {
        let cfg = Config::parse("[keyboard]\nlayout = \"de\"\n[theme]\nbridge = false\nicons = \"Papirus-Dark\"\n").unwrap();
        assert_eq!(cfg.keyboard.layout, "de");
    }

    #[test]
    fn lock_idle_and_protected_apps() {
        let cfg = Config::parse("[lock]\ncommand = [\"swaylock\", \"-f\"]\n[idle]\nlock_after = 0\n[protected]\napp_ids = [\"org.example.Bank\"]\n").unwrap();
        assert_eq!(cfg.lock.command, ["swaylock", "-f"]);
        assert!(cfg.lock.on_suspend);
        assert_eq!(cfg.idle.lock_after, 0);
        assert_eq!(cfg.protected.app_ids, ["org.example.Bank"]);
    }

    #[test]
    fn outputs_name_a_mode_a_scale_and_a_place() {
        let cfg = Config::parse("[[outputs]]\nname = \"DP-1\"\nmode = \"5120x2880@60\"\nscale = 2.0\nposition = [0, 0]\n[[outputs]]\nname = \"DP-2\"\nscale = 1.5\n").unwrap();
        assert_eq!(cfg.output("DP-1").unwrap().parsed_mode(), Some((5120, 2880, 60_000)));
        assert_eq!(cfg.output("DP-2").unwrap().scale, Some(1.5));
        assert!(cfg.output("HDMI-A-1").is_none());
        assert_eq!(parse_mode("1920x1080@59.94"), Some((1920, 1080, 59_940)));
        assert_eq!(parse_mode("1920x1080"), Some((1920, 1080, 0)));
        assert!(Config::parse("[[outputs]]\nname = \"DP-1\"\nmode = \"big\"\n").is_err());
        assert!(Config::parse("[[outputs]]\nname = \"DP-1\"\nscale = 9\n").is_err());
    }

    #[test]
    fn typos_are_errors_not_silent_defaults() {
        assert!(Config::parse("[windows]\nsnap_treshold = 3\n").is_err());
        assert!(Config::parse("background = \"red\"\n").is_err());
        assert!(Config::parse("[lock]\ncomand = []\n").is_err());
    }

    #[test]
    fn a_broken_file_falls_back_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("compositor.toml");
        std::fs::write(&path, "windows = 3").unwrap();
        let (cfg, err) = Config::load(&path);
        assert_eq!(cfg, Config::default());
        assert!(err.unwrap().contains("compositor.toml"));
        let (cfg, err) = Config::load(&dir.path().join("missing.toml"));
        assert_eq!((cfg, err), (Config::default(), None));
    }

    #[test]
    fn the_system_keyboard_fills_an_empty_layout_only() {
        let debian = "# KEYBOARD CONFIGURATION FILE\n\nXKBMODEL=\"pc105\"\nXKBLAYOUT=\"de\"\nXKBVARIANT=\"\"\nXKBOPTIONS=\"grp:alt_shift_toggle\"\n\nBACKSPACE=\"guess\"\n";
        let system = parse_system_keyboard(debian).unwrap();
        assert_eq!((system.layout.as_str(), system.variant.as_str(), system.options.as_str()), ("de", "", "grp:alt_shift_toggle"));
        assert_eq!(parse_system_keyboard("XKBLAYOUT=us,de\nXKBVARIANT=,nodeadkeys\n").map(|k| (k.layout, k.variant)), Some(("us,de".into(), ",nodeadkeys".into())));
        assert_eq!(parse_system_keyboard("XKBMODEL=\"pc105\"\nXKBLAYOUT=\"\"\n"), None);
        assert_eq!(parse_system_keyboard(""), None);

        let unset = Keyboard::default().or_system(Some(&system));
        assert_eq!((unset.layout.as_str(), unset.options.as_str(), unset.repeat_delay), ("de", "grp:alt_shift_toggle", 400));
        let set = Keyboard { layout: "us".into(), ..Keyboard::default() }.or_system(Some(&system));
        assert_eq!((set.layout.as_str(), set.variant.as_str(), set.options.as_str()), ("us", "", ""), "a configured layout wins");
        assert_eq!(Keyboard::default().or_system(None).layout, "", "no file: xkb's default");
    }
}
