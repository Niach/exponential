//! The Display page's "Keep these display settings?" (#EDE-250), as
//! Cinnamon and GNOME ask it: a scale or resolution applies at once, and
//! unless the person keeps it within [`REVERT_AFTER`] seconds the output
//! goes back to what it was. Only a kept change is written to
//! `compositor.toml`, so a scale that leaves the person unable to reach the
//! button never outlives the countdown or a restart.

use serde_json::{json, Value};

/// Seconds until an unconfirmed change goes back (Cinnamon's 15).
pub const REVERT_AFTER: u32 = 15;

/// One output's settings: `mode` as `WxH@R`, `scale`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSetting {
    pub mode: Option<String>,
    pub scale: Option<f64>,
}

/// A change waiting for Keep.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub name: String,
    /// What it was before the first unconfirmed change.
    pub before: OutputSetting,
    /// What it is now (what Keep writes).
    pub after: OutputSetting,
    pub seconds_left: u32,
    /// Bumped by every change: a countdown of an older one stops.
    pub generation: u64,
}

/// What the page asked.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Apply `after` on `name` now; `persist` = a different output's
    /// unconfirmed change to write first (it counts as kept).
    Apply { name: String, after: OutputSetting, persist: Option<(String, OutputSetting)> },
}

#[derive(Debug, Default)]
pub struct Confirm {
    pub pending: Option<Pending>,
    generation: u64,
}

impl Confirm {
    /// The page changed `name` from `current` to `wanted` (each field
    /// `None` = unchanged). A second change of the same output keeps its
    /// first `before` (Revert goes back to where the person started); a
    /// change of another output keeps the first output's.
    pub fn change(&mut self, name: &str, current: OutputSetting, wanted: OutputSetting) -> Step {
        self.generation += 1;
        let mut persist = None;
        let (before, mut after) = match self.pending.take() {
            Some(p) if p.name == name => (p.before, p.after),
            Some(p) => {
                persist = Some((p.name, p.after));
                (current.clone(), current)
            }
            None => (current.clone(), current),
        };
        if wanted.mode.is_some() {
            after.mode = wanted.mode;
        }
        if wanted.scale.is_some() {
            after.scale = wanted.scale;
        }
        self.pending = Some(Pending { name: name.to_string(), before, after: after.clone(), seconds_left: REVERT_AFTER, generation: self.generation });
        Step::Apply { name: name.to_string(), after, persist }
    }

    /// One second of `generation`'s countdown went by: `Some(revert)` when
    /// it ran out (the output and what it goes back to), `None` while it
    /// runs or when it is no longer the pending one.
    pub fn tick(&mut self, generation: u64) -> Option<(String, OutputSetting)> {
        let p = self.pending.as_mut().filter(|p| p.generation == generation)?;
        p.seconds_left = p.seconds_left.saturating_sub(1);
        if p.seconds_left > 0 {
            return None;
        }
        self.revert()
    }

    /// Keep: what to write, and the question goes.
    pub fn keep(&mut self) -> Option<(String, OutputSetting)> {
        self.pending.take().map(|p| (p.name, p.after))
    }

    /// Revert (pressed, or the countdown ran out): where it goes back to.
    pub fn revert(&mut self) -> Option<(String, OutputSetting)> {
        self.pending.take().map(|p| (p.name, p.before))
    }

    pub fn generation(&self) -> u64 {
        self.pending.as_ref().map_or(0, |p| p.generation)
    }

    /// `/display/confirm` for the page.
    pub fn json(&self) -> Value {
        match &self.pending {
            Some(p) => json!({"open": true, "name": p.name, "seconds": p.seconds_left, "caption": format!("Reverting in {} s", p.seconds_left)}),
            None => json!({"open": false, "name": "", "seconds": 0, "caption": ""}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(scale: f64) -> OutputSetting {
        OutputSetting { mode: Some("2560x1440@144".into()), scale: Some(scale) }
    }
    fn scale(s: f64) -> OutputSetting {
        OutputSetting { mode: None, scale: Some(s) }
    }

    #[test]
    fn an_unkept_scale_goes_back_after_the_countdown() {
        let mut c = Confirm::default();
        let Step::Apply { after, persist, .. } = c.change("DP-2", at(1.0), scale(2.0));
        assert_eq!((after, persist), (at(2.0), None));
        let g = c.generation();
        for _ in 1..REVERT_AFTER {
            assert_eq!(c.tick(g), None);
        }
        assert_eq!(c.json()["seconds"], 1);
        assert_eq!(c.tick(g), Some(("DP-2".into(), at(1.0))));
        assert_eq!(c.json()["open"], false);
    }

    #[test]
    fn keep_writes_the_new_settings_and_stops_the_countdown() {
        let mut c = Confirm::default();
        c.change("DP-2", at(1.0), scale(1.5));
        let g = c.generation();
        assert_eq!(c.keep(), Some(("DP-2".into(), at(1.5))));
        assert_eq!(c.tick(g), None, "a kept change never reverts");
    }

    #[test]
    fn a_second_change_restarts_the_countdown_and_reverts_to_the_start() {
        let mut c = Confirm::default();
        c.change("DP-2", at(1.0), scale(1.5));
        let first = c.generation();
        for _ in 0..10 {
            c.tick(first);
        }
        c.change("DP-2", at(1.5), scale(2.0));
        assert_eq!(c.tick(first), None, "the old countdown stopped");
        assert_eq!(c.json()["seconds"], REVERT_AFTER, "a fresh countdown");
        assert_eq!(c.revert(), Some(("DP-2".into(), at(1.0))));
    }

    #[test]
    fn changing_another_output_keeps_the_first() {
        let mut c = Confirm::default();
        c.change("DP-2", at(1.0), scale(1.5));
        let other = OutputSetting { mode: Some("3840x2160@60".into()), scale: Some(1.0) };
        let Step::Apply { persist, .. } = c.change("HDMI-A-3", other, scale(1.5));
        assert_eq!(persist, Some(("DP-2".into(), at(1.5))));
        assert_eq!(c.revert().map(|r| r.0), Some("HDMI-A-3".into()));
    }
}
