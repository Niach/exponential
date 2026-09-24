//! EXP-1051: where a run's context window actually GOES — the stacked bar and
//! its legend, folded from the `usage` meter (how full the window is) plus the
//! device's `context_layout` state (what the launcher put in it before the
//! first turn).
//!
//! Hand-mirrored ×4 and byte-locked by the ONE contract fixture every client's
//! test iterates (`packages/domain-contract/fixtures/context-layout.json`):
//!   web      apps/web/src/lib/context-layout.ts
//!   iOS      apps/ios/ExpCore/Sources/Domain/ContextLayoutPresentation.swift
//!   Android  apps/android/.../domain/ContextLayoutPresentation.kt
//! Changing a rule or a string here means changing it in all four and in the
//! fixture — the fixture IS the spec.
//!
//! Two rules the renderers depend on:
//!  * `conversation` and `free` are DERIVED here, never on the wire: the device
//!    publishes only what it can attribute, and everything else the window
//!    holds is the conversation.
//!  * The segments are NEVER scaled to fit. When the device's estimates
//!    overshoot the measured `context_used` the percents still add past 100 and
//!    the renderer CLIPS — a silently rescaled bar would lie about the layer
//!    sizes to hide a rounding error.

use domain::contract::{
    CONTEXT_LAYOUT_COMPACT_MIN_PERCENT, CONTEXT_LAYOUT_DERIVED_KEYS, CONTEXT_LAYOUT_DERIVED_LABELS,
    CONTEXT_LAYOUT_DERIVED_TONES, CONTEXT_LAYOUT_SEGMENT_KEYS, CONTEXT_LAYOUT_SEGMENT_LABELS,
    CONTEXT_LAYOUT_SEGMENT_TONES,
};

use crate::usage_bar::{self, Severity, DANGER_PERCENT, WARNING_PERCENT};

/// One slice of the stacked bar. `percent` is of the WHOLE window (size), to
/// two decimals — small layers (a 600-token task prompt in a 200k window) are
/// a hairline rather than nothing. `free` is not a slice: it is the track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextBarSlice {
    pub key: &'static str,
    pub tone: &'static str,
    pub percent: f64,
}

/// One legend row. Both numbers are already FORMATTED — the ×4 lock is on the
/// strings, not on the arithmetic behind them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextLegendRow {
    pub key: &'static str,
    pub label: &'static str,
    pub tone: &'static str,
    /// `21k`, `37.4k`, `600` — see [`tokens_compact`].
    pub tokens: String,
    /// `10.5%`, `0.3%` — one decimal, always.
    pub percent: String,
    /// The device guessed this layer rather than measuring it; the UI prefixes
    /// `≈`. Always false for the derived rows.
    pub estimated: bool,
    /// What the layer is made of, when the device named it (`CLAUDE.md,
    /// ~/.claude/CLAUDE.md`).
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextWindowView {
    /// `65k / 200k (32%)` — [`usage_bar::format_context_usage`], unchanged.
    pub headline: String,
    /// 0-100, floored ([`usage_bar::context_percent`]).
    pub percent: u8,
    pub severity: Severity,
    /// Where the bar draws its marks: the compaction floor, then the two usage
    /// thresholds.
    pub ticks: [u8; 3],
    pub bar: Vec<ContextBarSlice>,
    pub legend: Vec<ContextLegendRow>,
}

/// The derived `conversation` row's contract index.
const CONVERSATION: usize = 0;
/// The derived `free` row's contract index.
const FREE: usize = 1;

/// `21k`, `37.4k`, `1.5k`, `134.7k`, and the raw number under 1000 (`600`).
/// One decimal, with a trailing `.0` dropped — `21.0k` reads as false
/// precision on a number the device estimated.
pub(crate) fn tokens_compact(tokens: i64) -> String {
    let value = tokens.max(0);
    if value < 1000 {
        return value.to_string();
    }
    let text = format!("{:.1}", value as f64 / 1000.);
    format!("{}k", text.strip_suffix(".0").unwrap_or(&text))
}

/// Percent of the whole window, to TWO decimals — the bar's geometry. The
/// multiply happens BEFORE the divide so the four clients agree on the
/// rounding of an exact half.
fn bar_percent(tokens: i64, size: i64) -> f64 {
    ((tokens as f64 * 10_000.) / size as f64).round() / 100.
}

/// Percent of the whole window, to ONE decimal plus the sign — the legend's
/// string. Rounded from the same arithmetic as [`bar_percent`], so a row
/// reading `0.0%` is a row whose slice is a hairline, never a rounding
/// disagreement between the two.
fn legend_percent(tokens: i64, size: i64) -> String {
    format!("{:.1}%", ((tokens as f64 * 1_000.) / size as f64).round() / 10.)
}

/// The wire segments this client KNOWS, in the contract's render order: the
/// FIRST of a duplicate key wins, a negative count reads as zero and a
/// zero-token layer never draws at all. (An unknown key cannot reach here at
/// all: [`steer::ContextSegmentKey`] is an enum, so the wire decoder drops the
/// frame's unknown layers before the view ever sees them — the web's
/// "unknown key is dropped" rule, enforced one step earlier.)
fn known_segments(segments: &[steer::ContextSegment]) -> Vec<(usize, i64, Option<&str>, bool)> {
    let mut out = Vec::with_capacity(steer::ContextSegmentKey::ALL.len());
    for (index, key) in steer::ContextSegmentKey::ALL.into_iter().enumerate() {
        let Some(segment) = segments.iter().find(|entry| entry.key == key) else {
            continue;
        };
        let tokens = segment.tokens.max(0);
        if tokens == 0 {
            continue;
        }
        out.push((
            index,
            tokens,
            segment.detail.as_deref(),
            segment.source == steer::ContextSegmentSource::Estimated,
        ));
    }
    out
}

/// The whole context-window view, or `None` when there is no window to draw:
/// the engine has published no `usage` yet, or it reported a zero size
/// ("unknown"). A layout WITHOUT a usage is nothing — the bar has no scale.
///
/// `conversation` = what the window holds that the device could not attribute
/// (clamped at 0 when its estimates overshoot), `free` = the rest of the
/// window (clamped at 0 once a run runs past its own size).
pub(crate) fn context_window_view(
    usage: Option<&steer::SessionUsage>,
    segments: Option<&[steer::ContextSegment]>,
) -> Option<ContextWindowView> {
    let percent = usage_bar::context_percent(usage)?;
    let usage = usage?;
    let size = usage.context_size;
    let used = usage.context_used.max(0);

    let known = known_segments(segments.unwrap_or_default());
    let attributed: i64 = known.iter().map(|(_, tokens, _, _)| *tokens).sum();
    let conversation = (used - attributed).max(0);
    let free = (size - used).max(0);

    let mut bar: Vec<ContextBarSlice> = known
        .iter()
        .map(|(index, tokens, _, _)| ContextBarSlice {
            key: CONTEXT_LAYOUT_SEGMENT_KEYS[*index],
            tone: CONTEXT_LAYOUT_SEGMENT_TONES[*index],
            percent: bar_percent(*tokens, size),
        })
        .collect();
    // Always drawn, even at zero: the conversation is the slice that GROWS,
    // and a bar that gains a segment mid-run would re-key its slices.
    bar.push(ContextBarSlice {
        key: CONTEXT_LAYOUT_DERIVED_KEYS[CONVERSATION],
        tone: CONTEXT_LAYOUT_DERIVED_TONES[CONVERSATION],
        percent: bar_percent(conversation, size),
    });

    let mut legend: Vec<ContextLegendRow> = known
        .iter()
        .map(|(index, tokens, detail, estimated)| ContextLegendRow {
            key: CONTEXT_LAYOUT_SEGMENT_KEYS[*index],
            label: CONTEXT_LAYOUT_SEGMENT_LABELS[*index],
            tone: CONTEXT_LAYOUT_SEGMENT_TONES[*index],
            tokens: tokens_compact(*tokens),
            percent: legend_percent(*tokens, size),
            estimated: *estimated,
            detail: detail.map(str::to_string),
        })
        .collect();
    for (index, tokens) in [(CONVERSATION, conversation), (FREE, free)] {
        legend.push(ContextLegendRow {
            key: CONTEXT_LAYOUT_DERIVED_KEYS[index],
            label: CONTEXT_LAYOUT_DERIVED_LABELS[index],
            tone: CONTEXT_LAYOUT_DERIVED_TONES[index],
            tokens: tokens_compact(tokens),
            percent: legend_percent(tokens, size),
            // Derived from the engine's own measurement — never a guess.
            estimated: false,
            detail: None,
        });
    }

    Some(ContextWindowView {
        headline: usage_bar::format_context_usage(Some(usage)),
        percent,
        severity: usage_bar::severity(percent),
        // The compaction floor first: below it `exponential_sessions_compact`
        // refuses, so the tick is what makes "not yet" legible.
        ticks: [
            CONTEXT_LAYOUT_COMPACT_MIN_PERCENT,
            WARNING_PERCENT,
            DANGER_PERCENT,
        ],
        bar,
        legend,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The contract fixture, byte-locked ×4 (web `context-layout.test.ts`,
    /// iOS `ContextLayoutPresentationTests`, Android
    /// `ContextLayoutPresentationTest`).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/context-layout.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureUsage {
        context_used: i64,
        context_size: i64,
    }

    #[derive(Deserialize)]
    struct FixtureSegment {
        /// A STRING, not the enum: one case feeds a key no client knows.
        key: String,
        tokens: i64,
        source: String,
        #[serde(default)]
        detail: Option<String>,
    }

    #[derive(Deserialize)]
    struct FixtureSlice {
        key: String,
        tone: String,
        percent: f64,
    }

    #[derive(Deserialize)]
    struct FixtureRow {
        key: String,
        label: String,
        tone: String,
        tokens: String,
        percent: String,
        estimated: bool,
        #[serde(default)]
        detail: Option<String>,
    }

    #[derive(Deserialize)]
    struct FixtureExpected {
        headline: String,
        percent: u8,
        severity: String,
        ticks: Vec<u8>,
        bar: Vec<FixtureSlice>,
        legend: Vec<FixtureRow>,
    }

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        usage: Option<FixtureUsage>,
        segments: Option<Vec<FixtureSegment>>,
        expected: Option<FixtureExpected>,
    }

    fn cases() -> Vec<FixtureCase> {
        serde_json::from_str(FIXTURE).expect("the fixture parses")
    }

    fn severity_name(severity: Severity) -> &'static str {
        match severity {
            Severity::Normal => "normal",
            Severity::Warning => "warning",
            Severity::Danger => "danger",
        }
    }

    /// An unknown wire key never reaches this module (the decoder's enum drops
    /// it), so the harness drops it here — the one rule this client enforces
    /// one step earlier than the web.
    fn segment(fixture: &FixtureSegment) -> Option<steer::ContextSegment> {
        let key = steer::ContextSegmentKey::ALL
            .into_iter()
            .find(|key| key.as_str() == fixture.key)?;
        let source = steer::ContextSegmentSource::ALL
            .into_iter()
            .find(|source| source.as_str() == fixture.source)
            .expect("a known source");
        Some(steer::ContextSegment {
            key,
            tokens: fixture.tokens,
            source,
            detail: fixture.detail.clone(),
        })
    }

    #[test]
    fn every_fixture_case_folds_byte_exact() {
        let cases = cases();
        assert!(cases.len() >= 14, "the ×4 fixture lost cases");
        for case in &cases {
            let name = &case.name;
            let usage = case.usage.as_ref().map(|usage| steer::SessionUsage {
                context_used: usage.context_used,
                context_size: usage.context_size,
                cost_usd: None,
            });
            let segments = case
                .segments
                .as_ref()
                .map(|segments| segments.iter().filter_map(segment).collect::<Vec<_>>());
            let view = context_window_view(usage.as_ref(), segments.as_deref());
            let Some(expected) = case.expected.as_ref() else {
                assert!(view.is_none(), "{name}: expected no view");
                continue;
            };
            let view = view.unwrap_or_else(|| panic!("{name}: expected a view"));
            assert_eq!(view.headline, expected.headline, "{name}: headline");
            assert_eq!(view.percent, expected.percent, "{name}: percent");
            assert_eq!(
                severity_name(view.severity),
                expected.severity,
                "{name}: severity"
            );
            assert_eq!(view.ticks.to_vec(), expected.ticks, "{name}: ticks");
            assert_eq!(view.bar.len(), expected.bar.len(), "{name}: bar length");
            for (slice, want) in view.bar.iter().zip(&expected.bar) {
                assert_eq!(slice.key, want.key, "{name}: bar key");
                assert_eq!(slice.tone, want.tone, "{name}: bar tone of {}", want.key);
                assert!(
                    (slice.percent - want.percent).abs() < 0.005,
                    "{name}: bar percent of {}: {} != {}",
                    want.key,
                    slice.percent,
                    want.percent
                );
            }
            assert_eq!(
                view.legend.len(),
                expected.legend.len(),
                "{name}: legend length"
            );
            for (row, want) in view.legend.iter().zip(&expected.legend) {
                assert_eq!(row.key, want.key, "{name}: legend key");
                assert_eq!(row.label, want.label, "{name}: label of {}", want.key);
                assert_eq!(row.tone, want.tone, "{name}: tone of {}", want.key);
                assert_eq!(row.tokens, want.tokens, "{name}: tokens of {}", want.key);
                assert_eq!(row.percent, want.percent, "{name}: percent of {}", want.key);
                assert_eq!(
                    row.estimated, want.estimated,
                    "{name}: estimated of {}",
                    want.key
                );
                assert_eq!(
                    row.detail.as_deref(),
                    want.detail.as_deref(),
                    "{name}: detail of {}",
                    want.key
                );
            }
        }
    }

    /// The wire enum and the generated contract are the same list in the same
    /// order — the index is what pairs a segment with its label and tone.
    #[test]
    fn the_wire_keys_are_the_contract_keys_in_order() {
        assert_eq!(
            steer::ContextSegmentKey::ALL
                .map(steer::ContextSegmentKey::as_str)
                .as_slice(),
            CONTEXT_LAYOUT_SEGMENT_KEYS,
        );
        assert_eq!(CONTEXT_LAYOUT_SEGMENT_LABELS.len(), CONTEXT_LAYOUT_SEGMENT_KEYS.len());
        assert_eq!(CONTEXT_LAYOUT_SEGMENT_TONES.len(), CONTEXT_LAYOUT_SEGMENT_KEYS.len());
        assert_eq!(CONTEXT_LAYOUT_DERIVED_KEYS, &["conversation", "free"]);
    }

    /// The first tick IS the compaction floor the engine refuses below: a
    /// reader who sees the bar short of it knows why
    /// `exponential_sessions_compact` says "not yet".
    #[test]
    fn the_first_tick_is_the_engines_own_compaction_floor() {
        assert_eq!(
            engine::compaction::COMPACT_MIN_CONTEXT_FRACTION,
            f64::from(CONTEXT_LAYOUT_COMPACT_MIN_PERCENT) / 100.
        );
    }

    /// The ladder the fixture names, in one place: the rounding rule is the
    /// ×4 lock's most-copied line.
    #[test]
    fn the_compact_ladder() {
        for (tokens, want) in [
            (-1_i64, "0"),
            (0, "0"),
            (600, "600"),
            (999, "999"),
            (1_000, "1k"),
            (1_500, "1.5k"),
            (61_800, "61.8k"),
            (134_700, "134.7k"),
            (300_000, "300k"),
        ] {
            assert_eq!(tokens_compact(tokens), want, "{tokens}");
        }
    }
}
