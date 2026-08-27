//! EXP-637 — how a finished run's OUTCOME reads and looks. Hand-mirrored on
//! all four clients (web `lib/coding-session-display.ts`, iOS `RunOutcome`,
//! Android `domain/RunOutcome.kt`); the labels must stay byte-equal.

use domain::contract::{
    CODING_SESSION_OUTCOME_BLOCKED, CODING_SESSION_OUTCOME_DONE, CODING_SESSION_OUTCOME_NO_CHANGES,
};
use gpui_component::ActiveTheme as _;

use crate::icons::registry;

/// The user-facing label for a run outcome. An ended run with no outcome (an
/// exit, a kill, an old server) reads "Ended"; so does an unknown future
/// value — a raw wire word must never reach the UI.
pub fn outcome_label(outcome: Option<&str>) -> &'static str {
    match outcome {
        Some(CODING_SESSION_OUTCOME_DONE) => "Done",
        Some(CODING_SESSION_OUTCOME_BLOCKED) => "Blocked",
        Some(CODING_SESSION_OUTCOME_NO_CHANGES) => "No changes",
        _ => "Ended",
    }
}

/// The registry glyph for a run outcome (`None` = the neutral one).
pub fn outcome_icon(outcome: Option<&str>) -> crate::icons::ExpIcon {
    match outcome {
        Some(CODING_SESSION_OUTCOME_DONE) => registry::RUN_OUTCOME_DONE,
        Some(CODING_SESSION_OUTCOME_BLOCKED) => registry::RUN_OUTCOME_BLOCKED,
        _ => registry::RUN_OUTCOME_NO_CHANGES,
    }
}

/// The outcome's tint: done reads as success, blocked as a warning,
/// everything else stays muted.
pub fn outcome_color(outcome: Option<&str>, cx: &gpui::App) -> gpui::Hsla {
    match outcome {
        Some(CODING_SESSION_OUTCOME_DONE) => cx.theme().info,
        Some(CODING_SESSION_OUTCOME_BLOCKED) => cx.theme().warning,
        _ => cx.theme().muted_foreground,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The labels are hand-mirrored across four clients — a rename here that
    /// the natives don't get makes one product read two ways.
    #[test]
    fn labels_are_the_shared_vocabulary() {
        assert_eq!(outcome_label(Some("done")), "Done");
        assert_eq!(outcome_label(Some("blocked")), "Blocked");
        assert_eq!(outcome_label(Some("no_changes")), "No changes");
        assert_eq!(outcome_label(None), "Ended");
        assert_eq!(outcome_label(Some("")), "Ended");
        assert_eq!(outcome_label(Some("DONE")), "Ended", "wire values are lowercase");
    }

    /// The contract constants ARE the accepted values — a generated rename
    /// must break here, not silently fall through to "Ended".
    #[test]
    fn every_contract_outcome_has_a_label_and_a_glyph() {
        for value in domain::contract::CODING_SESSION_OUTCOME_VALUES {
            assert_ne!(
                outcome_label(Some(value)),
                "Ended",
                "contract outcome {value} has no label"
            );
        }
        // The generated registry must actually carry the three EXP-637
        // concepts (`ExpIcon` is neither `Debug` nor `PartialEq`, so the
        // mapping itself is only checked by construction above).
        let _: [crate::icons::ExpIcon; 4] = [
            registry::RUN_OUTCOME_DONE,
            registry::RUN_OUTCOME_BLOCKED,
            registry::RUN_OUTCOME_NO_CHANGES,
            registry::RUN_RESUME,
        ];
        for value in domain::contract::CODING_SESSION_OUTCOME_VALUES {
            let _ = outcome_icon(Some(value));
        }
        let _ = outcome_icon(None);
    }
}
