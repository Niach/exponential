//! EXP-920 — what an Exponential MCP tool's RESULT points at. The rule moved
//! to the agent core (EXP-1426 part 4b,
//! `exponential_agent_runtime::engine::exp_tool_refs`); this module keeps the
//! old name-keyed entry (the spec is Exponential's contract table), the
//! fixture lock and the pins of the values the core inlines.

use serde_json::Value;
use steer::EntityRef;

pub use exponential_agent_runtime::engine::exp_tool_refs::{list_ref_noun, tool_result_payload};

/// [`exponential_agent_runtime::engine::exp_tool_refs::exp_tool_refs`] for
/// one of Exponential's tools, its spec looked up by name.
pub fn exp_tool_refs(tool_name: &str, result: &Value, input: &Value) -> Vec<EntityRef> {
    exponential_agent_runtime::engine::exp_tool_refs::exp_tool_refs(
        steer::exp_tool::exp_tool_preview_spec(tool_name),
        result,
        input,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    const FIXTURE: &str = include_str!(
        "../../../../../packages/domain-contract/fixtures/tool-result-preview.json"
    );

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        tool: String,
        result: Value,
        #[serde(default)]
        input: Option<Value>,
        refs: Vec<EntityRef>,
    }

    /// The rule is the fixture's, field for field, for every case — the web
    /// port is locked by the same file.
    #[test]
    fn the_fixture_locks_every_case() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert!(fixture.cases.len() >= 40, "the fixture lost its cases");
        for case in &fixture.cases {
            let input = case.input.clone().unwrap_or_else(|| json!({}));
            let refs = exp_tool_refs(&case.tool, &case.result, &input);
            assert_eq!(refs, case.refs, "case: {}", case.name);
        }
    }

    /// EXP-1458: a sealed string (`exp-sealed:…`) is ciphertext: never a
    /// ref's id, identifier or title.
    #[test]
    fn sealed_strings_never_become_refs() {
        let result = json!([
            {"id": "cs-1", "title": "exp-sealed:v1:1:AAAA"},
            {"id": "exp-sealed:v1:1:BBBB", "title": "Hidden"},
        ]);
        let refs = exp_tool_refs("exponential_sessions_list", &result, &json!({}));
        let sessions: Vec<_> = refs.iter().filter(|r| r.kind == "session").collect();
        assert_eq!(sessions.len(), 1, "{refs:?}");
        assert_eq!(sessions[0].id, "cs-1");
        assert_eq!(sessions[0].title, None);
    }

    /// EXP-1426: the core inlines these contract values; a contract change
    /// fails here instead of drifting.
    #[test]
    fn the_core_caps_are_the_contract_caps() {
        use exponential_agent_runtime::engine::exp_tool_refs::{ENTITY_REF_KINDS, TEXT_MAX};
        assert_eq!(TEXT_MAX, domain::contract::EXP_TOOL_PREVIEW_TEXT_MAX);
        assert_eq!(ENTITY_REF_KINDS, domain::contract::ENTITY_REF_KIND_VALUES);
    }
}
