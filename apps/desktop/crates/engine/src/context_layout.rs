//! EXP-1051 — the `context_layout` frame's arithmetic.
//!
//! The wire frame breaks the context the agent starts a conversation with
//! into named segments (`base`, `tools`, `playbook`, `team`, `project`,
//! `task`) so a viewer can say WHY a window is half full before the person
//! has typed anything. It is LATEST-WINS state like `usage`, published ONCE
//! per conversation (again after a `/clear`) — never per turn.
//!
//! Only ONE segment can ever be measured: the agent reports the tokens its
//! first request carried and nothing else. Everything the launcher put there
//! is known in BYTES ([`coding::ContextLayers`]) and estimated at
//! `domain::contract::CONTEXT_LAYOUT_CHARS_PER_TOKEN` chars per token; `base`
//! is then the measured prefix MINUS those estimates — the agent's own system
//! prompt, its tool definitions and whatever else it prepends, which no
//! launcher can see. Getting it by subtraction is what keeps the bar's total
//! equal to the number the meter shows.
//!
//! `conversation` and `free` are NOT here: every client derives them from the
//! `usage` slot it already has, so they would go stale the moment the agent
//! spoke.
//!
//! The frame reaches the mapper as `_meta` on a no-op `session_info_update`
//! ([`CONTEXT_LAYOUT_META_KEY`], the rate-limit slot's carrier): ACP has no
//! shape for it and the engine adds no `SessionUpdate` variants of its own.

use serde_json::{json, Map, Value};

use steer::{ContextSegment, ContextSegmentKey, ContextSegmentSource};

/// The `_meta` key the adapters stamp the slot under, read by
/// [`crate::mapper::Mapper::on_update`]'s prologue beside the other
/// latest-wins `_meta` keys. Value: [`slot`]'s object.
pub const CONTEXT_LAYOUT_META_KEY: &str = "exponentialContextLayout";

/// Bytes → tokens, rounded (`(bytes + 2) / 4`, the integer spelling of the
/// `Math.round(bytes / 4)` every other client uses). An estimate on purpose:
/// the segments it covers are text the launcher wrote, and tokenizing them
/// for real would mean shipping a tokenizer per agent.
pub fn estimate_tokens(bytes: usize) -> i64 {
    let per = domain::contract::CONTEXT_LAYOUT_CHARS_PER_TOKEN.max(1);
    i64::try_from((bytes + per / 2) / per).unwrap_or(i64::MAX)
}

/// Every segment the LAUNCHER knows, in contract order, each `estimated`.
/// A segment worth zero tokens is omitted rather than drawn as a hairline
/// nobody can hit.
pub fn estimated_segments(layers: &coding::ContextLayers) -> Vec<ContextSegment> {
    let mut segments = Vec::new();
    let mut push = |key: ContextSegmentKey, bytes: usize, detail: Option<String>| {
        let tokens = estimate_tokens(bytes);
        if tokens > 0 {
            segments.push(ContextSegment {
                key,
                tokens,
                source: ContextSegmentSource::Estimated,
                detail,
            });
        }
    };
    if let Some(bytes) = layers.tools_bytes {
        push(ContextSegmentKey::Tools, bytes, None);
    }
    push(ContextSegmentKey::Playbook, layers.playbook_bytes, None);
    if let Some(bytes) = layers.team_bytes {
        push(ContextSegmentKey::Team, bytes, None);
    }
    if !layers.project.is_empty() {
        // ONE segment for the whole memory stack, rounded once: rounding each
        // file and summing would drift by up to half a token per file, and
        // the bar has one band for them anyway. The names go on `detail`.
        let bytes: usize = layers.project.iter().map(|file| file.bytes).sum();
        let detail = steer::truncate(
            &layers
                .project
                .iter()
                .map(|file| file.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            steer::CONTEXT_SEGMENT_DETAIL_MAX,
        );
        push(
            ContextSegmentKey::Project,
            bytes,
            (!detail.is_empty()).then_some(detail),
        );
    }
    if let Some(bytes) = layers.task_bytes {
        push(ContextSegmentKey::Task, bytes, None);
    }
    segments
}

/// Where the `base` segment's number comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BasePrefix {
    /// The agent just told us how many tokens its FIRST request carried
    /// (input + cache read + cache creation — never output, which is what
    /// the request produced rather than what it sent). `base` is that minus
    /// everything the launcher can account for.
    Measured(u64),
    /// EXP-1051 resume carry: the base a PREVIOUS run of this conversation
    /// measured, replayed verbatim so a resumed run draws its bar before its
    /// first request lands. Only ever used while the model is unchanged —
    /// a different model has a different system prompt.
    Carried(u64),
}

/// The `base` segment. Always `measured`: a carried base was measured too,
/// one run earlier, and calling it an estimate would understate it.
pub fn measure_base(prefix: BasePrefix, estimated: &[ContextSegment]) -> ContextSegment {
    let tokens = match prefix {
        BasePrefix::Measured(prefix) => {
            let prefix = i64::try_from(prefix).unwrap_or(i64::MAX);
            let accounted: i64 = estimated.iter().map(|segment| segment.tokens).sum();
            // SATURATING and floored at zero: an agent that reports a prefix
            // smaller than what we handed it (a cached request, a gateway
            // counting differently) must not produce a negative segment the
            // relay's zod would reject, dropping the frame whole.
            prefix.saturating_sub(accounted).max(0)
        }
        BasePrefix::Carried(tokens) => i64::try_from(tokens).unwrap_or(i64::MAX),
    };
    ContextSegment {
        key: ContextSegmentKey::Base,
        tokens,
        source: ContextSegmentSource::Measured,
        detail: None,
    }
}

/// The whole frame's segments, in contract order. Without a prefix there is
/// no `base` — the estimates alone are still worth drawing (codex on a
/// gateway that reports no input halves).
pub fn layout(layers: &coding::ContextLayers, prefix: Option<BasePrefix>) -> Vec<ContextSegment> {
    let estimated = estimated_segments(layers);
    match prefix {
        Some(prefix) => {
            let base = measure_base(prefix, &estimated);
            let mut segments = Vec::with_capacity(estimated.len() + 1);
            segments.push(base);
            segments.extend(estimated);
            segments
        }
        None => estimated,
    }
}

/// The slot's VALUE: an object rather than a bare array so the measured
/// `model` rides along — the mapper hands `(base, model)` back to the host,
/// which records it on `runs.json` for the next resume to carry.
pub fn slot(segments: &[ContextSegment], model: Option<&str>) -> Value {
    let mut slot = Map::new();
    slot.insert("segments".to_string(), json!(segments));
    if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
        slot.insert("model".to_string(), json!(model));
    }
    Value::Object(slot)
}

/// [`slot`] under [`CONTEXT_LAYOUT_META_KEY`] — what an adapter hands
/// `notify_meta`.
pub fn meta(segments: &[ContextSegment], model: Option<&str>) -> Map<String, Value> {
    let mut meta = Map::new();
    meta.insert(
        CONTEXT_LAYOUT_META_KEY.to_string(),
        slot(segments, model),
    );
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> coding::ContextLayers {
        coding::ContextLayers {
            playbook_bytes: 4_000,
            team_bytes: Some(800),
            task_bytes: Some(400),
            project: vec![
                coding::ProjectMemoryFile {
                    label: "CLAUDE.md".to_string(),
                    bytes: 40_000,
                },
                coding::ProjectMemoryFile {
                    label: "~/.claude/CLAUDE.md".to_string(),
                    bytes: 2_000,
                },
            ],
            tools_bytes: Some(9_600),
            carried_base: None,
        }
    }

    #[test]
    fn bytes_round_to_tokens_the_way_every_other_client_does() {
        assert_eq!(estimate_tokens(0), 0);
        assert_eq!(estimate_tokens(1), 0);
        assert_eq!(estimate_tokens(2), 1);
        assert_eq!(estimate_tokens(4), 1);
        assert_eq!(estimate_tokens(6), 2);
        assert_eq!(estimate_tokens(4_000), 1_000);
    }

    #[test]
    fn the_estimates_come_out_in_contract_order() {
        let segments = estimated_segments(&layers());
        let keys: Vec<&str> = segments.iter().map(|segment| segment.key.as_str()).collect();
        assert_eq!(keys, vec!["tools", "playbook", "team", "project", "task"]);
        assert!(segments
            .iter()
            .all(|segment| segment.source == ContextSegmentSource::Estimated));
        assert_eq!(segments[0].tokens, 2_400);
        assert_eq!(segments[1].tokens, 1_000);
        assert_eq!(segments[2].tokens, 200);
        // ONE rounding over the whole stack, not one per file.
        assert_eq!(segments[3].tokens, estimate_tokens(42_000));
        assert_eq!(
            segments[3].detail.as_deref(),
            Some("CLAUDE.md, ~/.claude/CLAUDE.md")
        );
        assert_eq!(segments[4].tokens, 100);
    }

    #[test]
    fn a_zero_token_layer_is_omitted_rather_than_drawn() {
        let layers = coding::ContextLayers {
            playbook_bytes: 4_000,
            team_bytes: Some(0),
            task_bytes: Some(1),
            tools_bytes: None,
            ..Default::default()
        };
        let keys: Vec<&str> = estimated_segments(&layers)
            .iter()
            .map(|segment| segment.key.as_str())
            .collect();
        assert_eq!(keys, vec!["playbook"], "an empty team prompt, a one-byte task and an unknown tool surface all round to nothing");
    }

    #[test]
    fn a_long_project_list_is_cut_at_the_contract_max() {
        let mut layers = coding::ContextLayers::default();
        layers.project = (0..80)
            .map(|index| coding::ProjectMemoryFile {
                label: format!("packages/really-quite-long-name-{index}/CLAUDE.md"),
                bytes: 1_000,
            })
            .collect();
        let detail = estimated_segments(&layers)[0].detail.clone().expect("a detail");
        assert!(detail.len() <= steer::CONTEXT_SEGMENT_DETAIL_MAX);
    }

    #[test]
    fn the_base_is_the_measured_prefix_minus_what_the_launcher_accounted_for() {
        let layers = layers();
        let estimated = estimated_segments(&layers);
        let accounted: i64 = estimated.iter().map(|segment| segment.tokens).sum();
        let base = measure_base(BasePrefix::Measured(30_000), &estimated);
        assert_eq!(base.key, ContextSegmentKey::Base);
        assert_eq!(base.source, ContextSegmentSource::Measured);
        assert_eq!(base.tokens, 30_000 - accounted);
    }

    #[test]
    fn a_prefix_smaller_than_the_estimates_floors_at_zero() {
        let estimated = estimated_segments(&layers());
        assert_eq!(measure_base(BasePrefix::Measured(10), &estimated).tokens, 0);
    }

    #[test]
    fn a_carried_base_rides_verbatim_and_still_reads_as_measured() {
        let estimated = estimated_segments(&layers());
        let base = measure_base(BasePrefix::Carried(21_000), &estimated);
        assert_eq!(base.tokens, 21_000);
        assert_eq!(base.source, ContextSegmentSource::Measured);
    }

    #[test]
    fn a_layout_without_a_prefix_has_no_base() {
        let layers = layers();
        let with = layout(&layers, Some(BasePrefix::Measured(30_000)));
        assert_eq!(with[0].key, ContextSegmentKey::Base);
        let without = layout(&layers, None);
        assert!(without
            .iter()
            .all(|segment| segment.key != ContextSegmentKey::Base));
        assert_eq!(without.len(), with.len() - 1);
    }

    #[test]
    fn the_meta_carries_the_segments_and_the_model_under_the_one_key() {
        let segments = layout(&layers(), Some(BasePrefix::Measured(30_000)));
        let carried = meta(&segments, Some("claude-opus-5"));
        let slot = carried.get(CONTEXT_LAYOUT_META_KEY).expect("the slot");
        assert_eq!(slot["model"], json!("claude-opus-5"));
        assert_eq!(
            slot["segments"].as_array().expect("an array").len(),
            segments.len()
        );
        assert_eq!(slot["segments"][0]["key"], json!("base"));
        assert_eq!(slot["segments"][0]["source"], json!("measured"));
        // A blank model is no model: the resume carry keys on it.
        assert!(meta(&segments, Some("  ")).get(CONTEXT_LAYOUT_META_KEY).expect("the slot")
            .get("model")
            .is_none());
    }
}
