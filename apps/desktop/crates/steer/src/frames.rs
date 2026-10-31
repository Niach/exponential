//! The frozen wire protocol's Rust mirror. EXP-1426: the module lives in
//! `eagent::wire::frames` now and is re-exported here
//! whole, so `steer::frames::*` and the flat `steer::*` re-exports keep
//! resolving and the serde wire is the same bytes.

pub use eagent::wire::frames::*;

/// The contract owns every vocabulary the wire carries. These checks lived
/// beside the types; the core cannot depend on `domain`, so they pin the
/// moved types (and the caps it inlines) from here.
#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn the_wire_vocabularies_are_the_contract_vocabularies() {
        // `Ended` is the DEFAULT: a client that has seen no turn event never
        // pulses.
        assert_eq!(
            [TurnState::Started.id(), TurnState::Ended.id()].as_slice(),
            domain::contract::TURN_STATE_VALUES
        );
        // EXP-850 §1: the ORDER is the lock (`wait` before `other`).
        let wire: Vec<&str> = ToolKind::ALL.iter().map(|kind| kind.as_str()).collect();
        assert_eq!(wire, domain::contract::TOOL_KIND_VALUES);
        let wire: Vec<&str> = SubagentStatus::ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(wire, domain::contract::SUBAGENT_STATUS_VALUES);
        assert_eq!(
            TaskListStatus::ALL.map(TaskListStatus::as_str).as_slice(),
            domain::contract::TASK_LIST_STATUS_VALUES
        );
        assert_eq!(
            ContextSegmentKey::ALL.map(ContextSegmentKey::as_str).as_slice(),
            domain::contract::CONTEXT_LAYOUT_SEGMENT_KEYS
        );
        assert_eq!(
            ContextSegmentSource::ALL.map(ContextSegmentSource::as_str).as_slice(),
            domain::contract::CONTEXT_LAYOUT_SOURCE_VALUES
        );
        let wire: Vec<&str> = BackgroundTaskKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(wire, domain::contract::BACKGROUND_TASK_KIND_VALUES);
    }

    #[test]
    fn the_inlined_caps_are_the_contract_caps() {
        assert_eq!(CONTEXT_SEGMENT_DETAIL_MAX, domain::contract::CONTEXT_LAYOUT_DETAIL_MAX);
        assert_eq!(TOOL_PREVIEW_MAX_REFS, domain::contract::EXP_TOOL_PREVIEW_MAX_REFS);
        assert_eq!(
            crate::TOOL_OUTPUT_MAX_LINES,
            domain::contract::STEER_FEED_TOOL_OUTPUT_MAX_LINES
        );
        assert_eq!(
            crate::TOOL_OUTPUT_MAX_BYTES,
            domain::contract::STEER_FEED_TOOL_OUTPUT_MAX_BYTES
        );
        assert_eq!(crate::TOOL_DIFF_MAX_LINES, domain::contract::STEER_FEED_TOOL_DIFF_MAX_LINES);
        assert_eq!(crate::TOOL_DIFF_MAX_BYTES, domain::contract::STEER_FEED_TOOL_DIFF_MAX_BYTES);
    }
}
