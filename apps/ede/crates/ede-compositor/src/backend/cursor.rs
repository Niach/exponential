//! The pointer image when no client sets one: the xcursor theme's `default`
//! (`$XCURSOR_THEME`, `$XCURSOR_SIZE`), else a built-in arrow; a named one
//! the compositor shows itself (the screen pick's crosshair) from the same
//! theme, else that arrow. Adapted from
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
    theme: String,
    /// The named icons looked up so far (`None` = the theme has none).
    named: std::collections::HashMap<&'static str, Option<Vec<Image>>>,
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
        Self { icons, size, theme, named: Default::default() }
    }

    /// [`Cursor::image`] of the theme's icon for `icon` (its CSS name, then
    /// its alternatives), the default arrow when it has none.
    pub fn named_image(&mut self, icon: smithay::input::pointer::CursorIcon, buffer_scale: i32, time: Duration) -> Image {
        if icon == smithay::input::pointer::CursorIcon::Default {
            return self.image(buffer_scale, time);
        }
        let theme = &self.theme;
        let icons = self.named.entry(icon.name()).or_insert_with(|| {
            let theme = CursorTheme::load(theme);
            std::iter::once(icon.name())
                .chain(icon.alt_names().iter().copied())
                .find_map(|name| theme.load_icon(name).and_then(|path| std::fs::read(path).ok()).and_then(|data| parse_xcursor(&data)).filter(|icons| !icons.is_empty()))
        });
        match icons {
            Some(icons) => frame(icons, self.size, buffer_scale, time),
            None => self.image(buffer_scale, time),
        }
    }

    /// The frame to show at `time` (themes may animate) on an output whose
    /// cursor buffers have `buffer_scale`: the theme's size nearest
    /// `$XCURSOR_SIZE` times that, so a 200% output gets the 48 px image,
    /// not the 24 px one stretched.
    pub fn image(&self, buffer_scale: i32, time: Duration) -> Image {
        frame(&self.icons, self.size, buffer_scale, time)
    }

    /// The buffer scale `image` is uploaded at: how many times
    /// `$XCURSOR_SIZE` it is (a theme without a 48 px image gives the 24 px
    /// one at 1, stretched, rather than a cursor half the size).
    pub fn buffer_scale(&self, image: &Image) -> i32 {
        ((image.size as f64 / self.size.max(1) as f64).round() as i32).max(1)
    }
}

/// The frame of `icons` (one icon's images, every size) for `buffer_scale`
/// times `size` at `time`.
fn frame(icons: &[Image], size: u32, buffer_scale: i32, time: Duration) -> Image {
    let want = size as i32 * buffer_scale;
    let nearest = icons.iter().min_by_key(|i| (want - i.size as i32).abs()).unwrap();
    let frames: Vec<&Image> = icons.iter().filter(|i| i.width == nearest.width && i.height == nearest.height).collect();
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
