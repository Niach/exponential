//! # The host API (VAPP-91)
//!
//! What a surface needs from its host, the same on all four platforms
//! (`packages/exponential-ui/catalog/host.json`): the router turning server
//! messages into ops, the wire decoders (JSONL, SSE, A2UI-over-MCP), the
//! function / URL / media policy, binding sources, catalog negotiation and
//! declarative app packages. A line-by-line port of the TypeScript reference
//! (`packages/exponential-ui/src/host/`) over `serde_json::Value`; every
//! output is JSON-equal to it, locked by `fixtures/host-*.json`. The I/O
//! (transports, the runtime host) stays on each platform; the Swift and
//! Kotlin painters reach this module through `exponential-ui-ffi`.

pub mod actions;
pub mod components;
pub mod contract;
pub mod decoders;
pub mod exposes;
pub mod expressions;
pub mod package;
pub mod policy;
pub mod calendar;
pub mod query;
pub mod record_macros;
pub mod record_migrations;
pub mod record_policy;
pub mod record_transitions;
pub mod record_store;
pub mod host_params;
pub mod records;
pub mod router;
pub mod screens;
pub mod share;
pub mod signature;
pub mod sources;
pub mod tests;
pub mod tools;

pub use actions::{
    action_call_name, action_var_types, apply_record_batch, apply_record_batch_as, function_effect, is_action_name, run_action, validate_package_actions, validate_package_effects, ActionContext, ACTION_CALL_PREFIX, ACTION_LIMIT_ACTIONS, ACTION_LIMIT_MAX_STEPS, ACTION_LIMIT_STEPS, ACTION_NAME_PATTERN,
    ACTION_STEP_KINDS, RECORDS_BATCH_FUNCTION,
};
pub use components::validate_package_components;
pub use contract::*;
pub use expressions::{literal_type, record_row_type, records_value_type, schema_type, validate_package_expressions, validate_record_computed};
pub use exposes::{expose_action_access, expose_issues, expose_name_destructive, expose_template_paths, validate_package_exposes, validate_upstreams, ExposeIssue};
pub use decoders::{decode_jsonl, mcp_action_call, messages_from_mcp_result, DecodeIssue, Decoded, JsonlDecoder, SseDecoder};
pub use package::{
    available_extensions, client_capabilities, merge_data, package_sources, source_allowed, supported_catalog_ids, template_messages, validate_package, validate_package_in, validate_package_with, version_satisfies, AvailableExtension, PackageIssue,
    PACKAGE_FORMAT_VERSION,
};
pub use policy::{
    combine_decisions, decide_function, decide_url, expand_link_base, image_dimensions, image_frames, image_header, is_builtin_function, is_private_host, media_image_check, ImageHeader, matches_pattern, media_image_within_limits, media_request, media_within_limits, package_policy, svg_dimensions,
    police_text_links, safe_href, FunctionDecision, FunctionPolicy, MediaOptions, MediaRequest, MediaRule, TextLink, UrlDecision, UrlPolicy, BUILTIN_FUNCTIONS,
};
pub use router::{normalize_path, HostRouter, SurfaceInfo};
pub use screens::{cross_app_target, cross_app_target_value, fill_screen_params, nav_screens, navigate_target, navigate_target_value, package_screens, resolve_screen, screen_data, validate_package_screens, NavigateTarget, ResolvedScreen, MAX_SCREENS, MAX_SCREEN_TITLE, MAX_EXPOSED_DESCRIPTION, NAVIGATE_FUNCTION, SCREEN_PARAMS_KEY};
pub use query::{
    computed_data, expand_query_source, fill_query_templates, kind_computed, kind_refs, month_days, month_label, parse_records_json, parse_records_query, parse_records_source, records_value, records_value_owned, shift_month, RecordRow, RecordRowRef, RecordsAggregate, RecordsAggregateOp, RecordsBefore, RecordsContext, RecordsDay, RecordsFilter, RecordsFilterOp, RecordsFilterValue, RecordsGroupKey, RecordsMonthView, RecordsOrder, RecordsQueryIssue, RecordsRunning, RecordsRunningOp,
    RecordsQuery, RecordsSortKey, RECORDS_AGGREGATE_OPS, RECORDS_AGGREGATE_RESERVED, RECORDS_COMPUTED_MAX, RECORDS_COMPUTED_ROW_KEYS, RECORDS_FILTER_OPS, RECORDS_MONTHS, RECORDS_OP_ALIASES, RECORDS_RUNNING_OPS, RECORDS_RESERVED_PARAMS, RECORDS_ROW_KEYS, RECORDS_SCHEME, RECORDS_WEEKDAYS,
};
pub use record_migrations::{is_version_range, kind_changes, kind_changes_json, migrate_record, migration_refusal, package_migrations, range_holds, validate_migrations, KindChange, MigrateResult, MIGRATION_OP_NAMES, RECORD_MIGRATIONS_MAX, RECORD_MIGRATION_OPS_MAX};
pub use record_transitions::{initial_states, kind_transitions, transition_refusal, transitions_from, validate_record_transitions, RecordTransitions, TransitionRefusal, TransitionViewer, RECORD_TRANSITIONS_ROW_KEY, TRANSITION_LEVELS};
pub use record_policy::{
    readable_rows, record_allowed, record_author_of, record_policy, record_policy_copy, record_refusal, validate_record_policy, RecordOp, RecordPolicy, RecordViewer, APP_ACCESS_LEVELS, DEFAULT_RECORD_POLICY, RECORD_POLICY_LEVELS, RECORD_POLICY_OPS,
};
pub use record_store::RECORDS_FUNCTIONS;
pub use host_params::{fill_host_params, host_params, with_host_params};
pub use records::{is_record_kind, record_attachment_fields, record_attachment_id, record_attachment_ids, validate_record, validate_record_refs, validate_record_schema, RECORD_FIELD_TYPES, RECORD_FORMATS, RECORD_RESERVED_KEYS};
pub use signature::{canonical_json, package_file_signed_bytes, AUTHOR_KEY_PREFIX, AUTHOR_NAME_MAX, PACKAGE_FILE_SIGNATURE_ALG, PACKAGE_FILE_SIGNATURE_CONTEXT};
pub use sources::{parse_source, source_with_params, ParsedSource};
pub use tests::{
    package_test_issues, run_package_tests, test_failure, test_matches, validate_package_tests, PackageTestResult, PACKAGE_TEST_DEFAULT_AS, PACKAGE_TEST_DEFAULT_NOW, PACKAGE_TEST_DEFAULT_ROLE, PACKAGE_TEST_ROLES, PACKAGE_TEST_KINDS, PACKAGE_TEST_LIMIT_GOT_MAX, PACKAGE_TEST_LIMIT_TESTS,
};
pub use tools::{tool_call, tool_consent, tool_consent_key, validate_tools, ToolCall, ToolConsentRow, TOOL_KINDS};

/// JavaScript's WhiteSpace + LineTerminator (what `trim` drops, regex `\s`).
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

/// JavaScript's `String.prototype.trim` (WhiteSpace + LineTerminator).
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// JavaScript's default string order (`sort()`, `<`): by UTF-16 code units,
/// which differs from Rust's byte order above U+FFFF.
/// VAPP-202: when either side is ASCII the first difference is an ASCII
/// byte against a byte of the same or a later char, where byte order and
/// UTF-16 order agree, so the bytes compare directly.
pub(crate) fn utf16_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() || b.is_ascii() {
        return a.as_bytes().cmp(b.as_bytes());
    }
    a.encode_utf16().cmp(b.encode_utf16())
}

/// JavaScript truthiness of a JSON value.
pub(crate) fn js_truthy(value: &serde_json::Value) -> bool {
    use serde_json::Value;
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
