//! `~/.config/ede/compositor.toml`: the compositor's own settings.
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
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Keyboard {
    /// xkb layout, e.g. `"de"` or `"us,de"`; empty = the system default.
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyboard: Keyboard::default(),
            windows: Windows::default(),
            background: "#0a0a0a".into(),
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

impl Default for Windows {
    fn default() -> Self {
        Self { snap_threshold: 12, edge_tiling: true, edge_tiling_px: 4, corner_px: 120 }
    }
}

impl Config {
    /// `$XDG_CONFIG_HOME/ede/compositor.toml`, else `~/.config/ede/compositor.toml`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("ede").join("compositor.toml"))
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.background_rgba()?;
        Ok(cfg)
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
        assert_eq!(cfg.keyboard.layout, "de");
        assert_eq!(cfg.keyboard.repeat_rate, 30);
        let [r, g, b, a] = cfg.background_rgba().unwrap();
        assert_eq!(((r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round(), a), (16.0, 32.0, 48.0, 1.0));
    }

    #[test]
    fn typos_are_errors_not_silent_defaults() {
        assert!(Config::parse("[windows]\nsnap_treshold = 3\n").is_err());
        assert!(Config::parse("background = \"red\"\n").is_err());
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
}
