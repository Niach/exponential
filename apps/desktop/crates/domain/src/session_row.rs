//! EXP-1248 — the BIG session row's caption, ×4 (web
//! `lib/session-row-caption.ts`, iOS `SessionRowCaption.swift`, Android
//! `SessionRowCaption.kt`), locked by
//! `packages/domain-contract/fixtures/list-item.json` (`elapsed`, `captions`;
//! the replay lives in [`crate::list_item`]'s tests).
//!
//! The live states and their tones are `session-display.json`'s; this only
//! words them for a list: `<State> · <device> · <age>`. First match wins:
//! ended → `Done · <device> · <when>` muted (when = now − ended, else the
//! heartbeat); paused → `Paused · <device>` muted; a usage wall → its badge
//! label, amber; else the live display state. A missing device or an
//! unparsable stamp drops that segment. Timestamps are epoch MILLISECONDS,
//! parsed by the caller (`None` = missing or unparsable).

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// The list's coarse age ladder: `now`, `5 min`, `21 h`, `2 d` (floored; a
/// negative span reads `now`).
pub fn list_elapsed(ms: i64) -> String {
    if ms < MINUTE {
        return "now".to_string();
    }
    if ms < HOUR {
        return format!("{} min", ms / MINUTE);
    }
    if ms < DAY {
        return format!("{} h", ms / HOUR);
    }
    format!("{} d", ms / DAY)
}

/// A live run's display state (`session-display.json` `state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRowState {
    Working,
    NeedsInput,
    Review,
    Done,
}

impl SessionRowState {
    /// The fixture's wire word (`working`, `needs_input`, `review`, `done`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "working" => Some(Self::Working),
            "needs_input" => Some(Self::NeedsInput),
            "review" => Some(Self::Review),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Working => "Building",
            Self::NeedsInput => "Needs input",
            Self::Review => "Ready for review",
            Self::Done => "Done",
        }
    }

    /// `session-display.json` statusTone.
    fn tone(self) -> SessionRowTone {
        match self {
            Self::Working => SessionRowTone::Muted,
            Self::NeedsInput => SessionRowTone::Amber,
            Self::Review => SessionRowTone::Emerald,
            Self::Done => SessionRowTone::Sky,
        }
    }
}

/// The caption's tone, the `session-display.json` statusTone palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRowTone {
    Muted,
    Amber,
    Emerald,
    Sky,
}

impl SessionRowTone {
    /// The fixture's wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Muted => "muted",
            Self::Amber => "amber",
            Self::Emerald => "emerald",
            Self::Sky => "sky",
        }
    }
}

/// What the caption reads (web `SessionRowCaptionInput`).
#[derive(Clone, Copy, Debug)]
pub struct SessionRowCaptionInput<'a> {
    /// `run_has_ended(session)`.
    pub ended: bool,
    /// An offline host (`session_is_paused`).
    pub paused: bool,
    /// The live display state; ignored once ended.
    pub state: SessionRowState,
    /// The resolved device label; `None`/blank drops the segment.
    pub device: Option<&'a str>,
    pub started_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    pub ended_ms: Option<i64>,
    /// The usage wall's badge label, or `None`.
    pub blocked_label: Option<&'a str>,
    pub now_ms: i64,
}

/// The big row's caption and its tone.
pub fn session_row_caption(input: SessionRowCaptionInput<'_>) -> (String, SessionRowTone) {
    let since = |stamp: Option<i64>| stamp.map(|at| list_elapsed(input.now_ms - at));
    let device = input
        .device
        .map(str::trim)
        .filter(|device| !device.is_empty());
    let join = |head: &str, tail: Option<String>| {
        [Some(head.to_string()), device.map(str::to_string), tail]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    };
    if input.ended {
        let when = since(input.ended_ms).or_else(|| since(input.updated_ms));
        return (join("Done", when), SessionRowTone::Muted);
    }
    if input.paused {
        return (join("Paused", None), SessionRowTone::Muted);
    }
    if let Some(label) = input.blocked_label.filter(|label| !label.is_empty()) {
        return (label.to_string(), SessionRowTone::Amber);
    }
    let live = match input.state {
        SessionRowState::Working | SessionRowState::NeedsInput => since(input.started_ms),
        SessionRowState::Review | SessionRowState::Done => since(input.updated_ms),
    };
    (join(input.state.word(), live), input.state.tone())
}
