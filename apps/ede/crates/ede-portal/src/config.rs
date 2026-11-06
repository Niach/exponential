//! `~/.config/exponential-desktop/portal.toml`. Every key is optional; a file that fails to
//! parse is reported and the defaults apply. Re-read when the compositor
//! reloads its config (`config.reloaded`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub screencast: ScreenCast,
    pub screenshot: Screenshot,
    pub remote_desktop: RemoteDesktop,
    pub appearance: Appearance,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScreenCast {
    /// Frames per second a stream offers at most.
    pub framerate: u32,
    /// Picks what to share. Gets the choice as JSON on stdin (see the
    /// README) and prints the picks; empty output = cancelled. Empty =
    /// share the focused output or window and say so with a notification,
    /// until the shell's chooser exists (#NEW-108).
    pub chooser: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Screenshot {
    /// Where screenshots go; empty = `$XDG_PICTURES_DIR/Screenshots`.
    pub dir: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RemoteDesktop {
    /// App ids that may drive the keyboard and pointer without the chooser.
    /// With neither a chooser nor an entry here remote control is refused:
    /// it is never granted silently.
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    /// `"dark"` (Exponential's own look), `"light"` or `"default"`.
    pub color_scheme: String,
    /// `#rrggbb`; empty = none.
    pub accent: String,
    pub high_contrast: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { screencast: ScreenCast::default(), screenshot: Screenshot::default(), remote_desktop: RemoteDesktop::default(), appearance: Appearance::default() }
    }
}

impl Default for ScreenCast {
    fn default() -> Self {
        Self { framerate: 30, chooser: default_chooser() }
    }
}

impl Default for Screenshot {
    fn default() -> Self {
        Self { dir: String::new() }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self { color_scheme: "dark".into(), accent: String::new(), high_contrast: false }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("exponential-desktop").join("portal.toml"))
    }

    /// The file at `path`; missing = defaults, broken = defaults + a warning.
    pub fn load(path: Option<&Path>) -> Config {
        let Some(path) = path else { return Config::default() };
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("{}: {e}; using the defaults", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    /// `org.freedesktop.appearance` `color-scheme`: 0 none, 1 dark, 2 light.
    pub fn color_scheme(&self) -> u32 {
        match self.appearance.color_scheme.as_str() {
            "dark" => 1,
            "light" => 2,
            _ => 0,
        }
    }

    /// `accent-color` as linear 0..1 RGB.
    pub fn accent(&self) -> Option<(f64, f64, f64)> {
        let hex = self.appearance.accent.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let c = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| v as f64 / 255.0);
        Some((c(0)?, c(2)?, c(4)?))
    }

    pub fn screenshot_dir(&self) -> PathBuf {
        if !self.screenshot.dir.is_empty() {
            return expand_home(&self.screenshot.dir);
        }
        pictures_dir().join("Screenshots")
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

/// `XDG_PICTURES_DIR` from `user-dirs.dirs`, else `~/Pictures`.
/// `exponential-sealed share` when it is installed (next to this binary's
/// `/usr/bin`, or on PATH); else none (the focused output/window).
pub fn default_chooser() -> Vec<String> {
    let on_path = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("exponential-sealed").exists()));
    if on_path || Path::new("/usr/bin/exponential-sealed").exists() {
        vec!["exponential-sealed".into(), "share".into()]
    } else {
        Vec::new()
    }
}

pub fn pictures_dir() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
    if let Ok(text) = std::fs::read_to_string(config.join("user-dirs.dirs")) {
        for line in text.lines() {
            if let Some(v) = line.trim().strip_prefix("XDG_PICTURES_DIR=") {
                let v = v.trim_matches('"').replace("$HOME", &home().to_string_lossy());
                return PathBuf::from(v);
            }
        }
    }
    home().join("Pictures")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_dark_and_parse_partial_files() {
        assert_eq!(Config::default().color_scheme(), 1);
        let c: Config = toml::from_str("[appearance]\ncolor_scheme = \"light\"\naccent = \"#ff8000\"\n[screencast]\nframerate = 15\n").unwrap();
        assert_eq!((c.color_scheme(), c.screencast.framerate), (2, 15));
        assert_eq!(c.accent(), Some((1.0, 128.0 / 255.0, 0.0)));
        assert!(toml::from_str::<Config>("[screencast]\nfps = 3\n").is_err());
    }
}
