//! Round 2 (docs/round-2-contract.md): the Rust core replays every round-2
//! fixture with the test names of `src/round2.test.ts` (the case lists live
//! in `support/round2.rs`, shared with the conformance runner).

mod support;

use support::round2;
use support::Case;

fn all_pass(label: &str, cases: Vec<Case>, at_least: usize) {
    assert!(cases.len() >= at_least, "{label}: {} cases", cases.len());
    let failed: Vec<String> = cases.into_iter().filter_map(|(n, r)| r.err().map(|e| format!("{n}: {e}"))).collect();
    assert!(failed.is_empty(), "{label}:\n  {}", failed.join("\n  "));
}

#[test]
fn format_json_every_call_through_the_english_fallback_and_every_display_string() {
    all_pass("format", round2::format_cases(), 86);
}

#[test]
fn template_items_json_keys_row_keys_accumulated_suffixes_and_lifted_templates() {
    all_pass("template-items", round2::template_item_cases(), 17);
}

#[test]
fn text_direction_json_per_node_direction_and_physical_alignment() {
    all_pass("text-direction", round2::text_direction_cases(), 2);
}

#[test]
fn resizable_json_normalize_resize_keys_extents_drag_within_1e_6() {
    all_pass("resizable", round2::resizable_cases(), 35);
}

#[test]
fn virtual_list_json_windows_scroll_to_index_sections_sticky_headers() {
    all_pass("virtual-list", round2::virtual_list_cases(), 24);
}

#[test]
fn animations_json_timing_frames_reduced_motion_and_css_per_theme() {
    all_pass("animations", round2::animation_cases(), 25);
}

#[test]
fn the_kitchen_sinks_list_template_renders_only_per_item() {
    let expanded = support::fixture("kitchen-sink.expanded.json");
    let templates: Vec<&String> = expanded["templates"].as_object().unwrap().keys().collect();
    assert_eq!(templates, ["list-item"]);
    let sink: exponential_ui::NestedNode = serde_json::from_value(support::fixture("kitchen-sink.json")).unwrap();
    let reduced = exponential_ui::reducer::reduce_nested(&sink, &exponential_ui::reducer::ReduceOptions::new(exponential_ui::CORE_CATALOG_ID));
    assert!(!exponential_ui::reducer::preorder(&reduced.root).contains(&"list-item".to_string()));
}

#[test]
fn the_bench_surface_reduces_cleanly_and_its_template_is_lifted() {
    let bench = support::fixture("bench-list.json");
    let components: Vec<exponential_ui::FlatComponent> = serde_json::from_value(bench["components"].clone()).unwrap();
    let result = exponential_ui::reducer::reduce_surface(&components, &exponential_ui::reducer::ReduceOptions::new(bench["catalogId"].as_str().unwrap()));
    assert_eq!(result.issues, vec![]);
    assert_eq!(result.templates.unwrap().keys().collect::<Vec<_>>(), ["bench-row"]);
    assert_eq!(bench["rows"]["count"], 100_000);
}
