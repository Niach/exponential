//! The recovery screen (#EDE-16, Tier A): when the shell is not running
//! (never came up, or died and did not come back) the compositor itself
//! draws what to do, so a broken shell never leaves a black screen:
//! Super+Shift+R puts the stock shell documents back and restarts the
//! shell's unit, Ctrl+Alt+T opens a terminal, Super+Shift+E logs out. The
//! shell is "running" while its root grant is subscribed to events.

use std::time::{Duration, Instant};

use smithay::{
    backend::renderer::{element::texture::TextureRenderElement, ImportMem, Renderer},
    output::Output,
    utils::{Logical, Point},
};

use crate::{
    state::Ede,
    text::{self, colour, Canvas, TextureCache},
};

/// A shell that never subscribed gets this long after start.
pub const START_GRACE: Duration = Duration::from_secs(20);
/// A shell that was there and left gets this long to come back.
pub const RETURN_GRACE: Duration = Duration::from_secs(8);

pub struct Recovery {
    pub shell_seen: bool,
    pub shell_lost_at: Option<Instant>,
    pub start_grace: Duration,
    pub return_grace: Duration,
    cache: TextureCache,
}

impl Default for Recovery {
    fn default() -> Self {
        // `EDE_RECOVERY_GRACE_MS` shortens both graces (the dev harness, a picture).
        let grace = std::env::var("EDE_RECOVERY_GRACE_MS").ok().and_then(|v| v.parse::<u64>().ok()).map(Duration::from_millis);
        Self { shell_seen: false, shell_lost_at: None, start_grace: grace.unwrap_or(START_GRACE), return_grace: grace.unwrap_or(RETURN_GRACE), cache: TextureCache::default() }
    }
}

const LINES: [&str; 4] = ["The desktop shell is not running.", "Super+Shift+R puts the stock shell back and restarts it", "Ctrl+Alt+T opens a terminal", "Super+Shift+E logs out"];
const WIDTH: i32 = 520;
const HEIGHT: i32 = 150;

impl Ede {
    /// The shell subscribed (its root grant): no recovery.
    pub fn shell_arrived(&mut self) {
        self.recovery.shell_seen = true;
        self.recovery.shell_lost_at = None;
    }

    /// The shell's last subscribed connection went.
    pub fn shell_left(&mut self) {
        if self.recovery.shell_lost_at.is_none() {
            self.recovery.shell_lost_at = Some(Instant::now());
        }
    }

    pub fn recovery_visible(&self) -> bool {
        if self.shell_listening() || self.is_locked() {
            return false;
        }
        match (self.recovery.shell_seen, self.recovery.shell_lost_at) {
            (false, _) => self.start_time.elapsed() >= self.recovery.start_grace,
            (true, Some(at)) => at.elapsed() >= self.recovery.return_grace,
            (true, None) => false,
        }
    }

    /// The message centred on `output`, when it shows.
    pub fn recovery_elements<R>(&mut self, renderer: &mut R, output: &Output) -> Vec<TextureRenderElement<R::TextureId>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Clone + 'static,
    {
        if !self.recovery_visible() {
            return vec![];
        }
        let Some(geo) = self.space.output_geometry(output) else { return vec![] };
        let scale = output.current_scale().fractional_scale();
        let at: Point<i32, Logical> = ((geo.size.w - WIDTH) / 2, (geo.size.h - HEIGHT) / 2).into();
        let element = self.recovery.cache.element(renderer, text::key_of(&scale.to_bits()), scale.round().max(1.0) as i32, at.to_f64().to_physical(scale), || draw(scale));
        element.into_iter().collect()
    }
}

fn draw(scale: f64) -> Canvas {
    let s = scale.max(0.5) as f32;
    let (w, h) = ((WIDTH as f32 * s).round() as u32, (HEIGHT as f32 * s).round() as u32);
    let mut c = Canvas::new(w, h);
    c.fill_rounded_top(w as i32, h as i32, 12.0 * s, colour::CARD);
    c.fill_rect(0, h as i32 - 1, w as i32, 1, colour::BORDER_ON_CARD);
    let mut y = 24.0 * s + text::ascent(16.0 * s);
    for (i, line) in LINES.iter().enumerate() {
        let (size, fg) = if i == 0 { (16.0 * s, colour::FOREGROUND) } else { (14.0 * s, colour::MUTED_FOREGROUND) };
        text::draw(&mut c, 24.0 * s, y, line, size, fg);
        y += if i == 0 { 34.0 * s } else { 26.0 * s };
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_draws() {
        assert_eq!(draw(1.0).width, WIDTH as u32);
        assert_eq!(draw(2.0).height, (HEIGHT * 2) as u32);
    }
}
