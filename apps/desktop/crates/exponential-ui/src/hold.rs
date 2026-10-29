//! VAPP-162 (docs/round-5-contract.md §9): the Button `hold` gesture as a
//! pure state machine every painter shares (the core drives it through
//! `Surface::hold`; gpui may also run it directly). With `hold: true` a
//! press never fires `press`: pointer down (or Space / Enter key down, key
//! repeat ignored) starts the hold (`holdStart`, the `pressed` state, the
//! `holdingLabel`); release within [`HOLD_SLOP`] px of the box (or key up of
//! the key that started it) ends it (`holdEnd`); the pointer leaving the box
//! by more than the slop, a pointer cancel, Escape or losing focus cancel it
//! (`holdCancel`); after a cancel nothing fires until the next down.
//! `fixtures/interactions-round1.json` `buttonHold` locks it.

use serde::{Deserialize, Serialize};

/// How far (px) the pointer may wander outside the box while held.
pub const HOLD_SLOP: f32 = 24.0;

/// One input to a hold. Coordinates are px relative to the button's
/// top-left; `width`/`height` = its laid-out box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "camelCase")]
pub enum HoldInput {
    #[serde(rename = "pointerdown")]
    PointerDown { x: f32, y: f32 },
    #[serde(rename = "pointermove")]
    PointerMove { x: f32, y: f32 },
    #[serde(rename = "pointerup")]
    PointerUp { x: f32, y: f32 },
    #[serde(rename = "pointercancel")]
    PointerCancel,
    #[serde(rename = "keydown")]
    KeyDown {
        key: String,
        #[serde(default)]
        repeat: bool,
    },
    #[serde(rename = "keyup")]
    KeyUp { key: String },
    #[serde(rename = "blur")]
    Blur,
}

/// What a step fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HoldEvent {
    HoldStart,
    HoldEnd,
    HoldCancel,
}

impl HoldEvent {
    /// The catalog event name (`holdStart` / `holdEnd` / `holdCancel`).
    pub fn name(self) -> &'static str {
        match self {
            Self::HoldStart => "holdStart",
            Self::HoldEnd => "holdEnd",
            Self::HoldCancel => "holdCancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Source {
    #[default]
    None,
    Pointer,
    Key(String),
}

/// One button's hold.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HoldTracker {
    source: Source,
}

fn within_slop(x: f32, y: f32, width: f32, height: f32) -> bool {
    x >= -HOLD_SLOP && y >= -HOLD_SLOP && x <= width + HOLD_SLOP && y <= height + HOLD_SLOP
}

fn hold_key(key: &str) -> bool {
    matches!(key, " " | "Space" | "Spacebar" | "Enter")
}

impl HoldTracker {
    /// Whether the button is held now (paint `pressed` + `holdingLabel`).
    pub fn active(&self) -> bool {
        self.source != Source::None
    }

    /// Feed one input on a button `width` × `height`; the event it fires.
    pub fn input(&mut self, input: &HoldInput, width: f32, height: f32) -> Option<HoldEvent> {
        let active = self.active();
        let cancel = |s: &mut Self| {
            s.source = Source::None;
            Some(HoldEvent::HoldCancel)
        };
        match input {
            HoldInput::PointerDown { .. } if !active => {
                self.source = Source::Pointer;
                Some(HoldEvent::HoldStart)
            }
            HoldInput::PointerMove { x, y } if self.source == Source::Pointer && !within_slop(*x, *y, width, height) => cancel(self),
            HoldInput::PointerUp { x, y } if self.source == Source::Pointer => {
                self.source = Source::None;
                Some(if within_slop(*x, *y, width, height) { HoldEvent::HoldEnd } else { HoldEvent::HoldCancel })
            }
            HoldInput::PointerCancel if self.source == Source::Pointer => cancel(self),
            HoldInput::KeyDown { key, .. } if active && key == "Escape" => cancel(self),
            HoldInput::KeyDown { key, repeat } if !active && !repeat && hold_key(key) => {
                self.source = Source::Key(key.clone());
                Some(HoldEvent::HoldStart)
            }
            HoldInput::KeyUp { key } if self.source == Source::Key(key.clone()) => {
                self.source = Source::None;
                Some(HoldEvent::HoldEnd)
            }
            HoldInput::Blur if active => cancel(self),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slide_away_cancels_and_the_release_fires_nothing() {
        let mut t = HoldTracker::default();
        assert_eq!(t.input(&HoldInput::PointerDown { x: 60.0, y: 20.0 }, 120.0, 40.0), Some(HoldEvent::HoldStart));
        assert_eq!(t.input(&HoldInput::PointerMove { x: -24.0, y: 20.0 }, 120.0, 40.0), None, "the slop is inclusive");
        assert_eq!(t.input(&HoldInput::PointerMove { x: -30.0, y: 20.0 }, 120.0, 40.0), Some(HoldEvent::HoldCancel));
        assert_eq!(t.input(&HoldInput::PointerUp { x: 60.0, y: 20.0 }, 120.0, 40.0), None);
        assert!(!t.active());
    }
}
