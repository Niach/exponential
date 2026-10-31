//! The pointer image when no client sets one: the xcursor theme's `default`
//! (`$XCURSOR_THEME`, `$XCURSOR_SIZE`), else a built-in arrow. Adapted from
//! smithay's anvil (MIT, Copyright (c) 2017 Victor Berger and Smithay
//! contributors); `resources/cursor.rgba` is anvil's fallback arrow.

use std::time::Duration;

use xcursor::{
    parser::{parse_xcursor, Image},
    CursorTheme,
};

static FALLBACK: &[u8] = include_bytes!("../../resources/cursor.rgba");

pub struct Cursor {
    icons: Vec<Image>,
    size: u32,
}

impl Cursor {
    pub fn load() -> Self {
        let theme = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(24);
        let icons = CursorTheme::load(&theme)
            .load_icon("default")
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|data| parse_xcursor(&data))
            .filter(|icons| !icons.is_empty())
            .unwrap_or_else(|| {
                tracing::warn!("xcursor theme {theme:?} has no default cursor; using the built-in arrow");
                vec![Image { size: 32, width: 64, height: 64, xhot: 1, yhot: 1, delay: 1, pixels_rgba: FALLBACK.to_vec(), pixels_argb: vec![] }]
            });
        Self { icons, size }
    }

    /// The frame to show at `time` (themes may animate).
    pub fn image(&self, time: Duration) -> Image {
        let nearest = self.icons.iter().min_by_key(|i| (self.size as i32 - i.size as i32).abs()).unwrap();
        let frames: Vec<&Image> = self.icons.iter().filter(|i| i.width == nearest.width && i.height == nearest.height).collect();
        let total: u32 = frames.iter().map(|i| i.delay).sum();
        if total == 0 {
            return frames[0].clone();
        }
        let mut ms = time.as_millis() as u32 % total;
        for f in &frames {
            if ms < f.delay {
                return (*f).clone();
            }
            ms -= f.delay;
        }
        frames[0].clone()
    }
}
