//! #EDE-95: `fixtures/text-highlight.json` `markdownCases` through the gpui
//! Markdown parser (each block of the normal form searched apart; the TS
//! twin is `markdownHighlightCount`), the mark helpers, and a highlighted
//! Text, Row and Markdown painting.

use std::rc::Rc;

use eui_core::text_highlight::{highlight_ranges, HighlightSpec};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::NestedNode;
use eui_gpui::host::HostPlugin;
use eui_gpui::paint::highlight::{joined_marks, line_marks, mark_runs, markdown_highlight_count, markdown_highlight_texts, markdown_marks, split_unit_marks, MarkColors};
use eui_gpui::paint::markdown::{parse, Inline, InlineChip, CODE_CHIP};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{font, hsla, TestAppContext, TextRun};
use serde_json::{json, Value};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/text-highlight.json");

#[test]
fn every_markdown_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json");
    let cases = f["markdownCases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let (name, text, query) = (c["name"].as_str().unwrap(), c["text"].as_str().unwrap(), c["query"].as_str().unwrap());
        assert_eq!(markdown_highlight_count(text, query) as u64, c["count"].as_u64().unwrap(), "{name}: count");
        let matches: Vec<Value> = markdown_highlight_texts(&parse(text)).iter().flat_map(|t| highlight_ranges(t, query).into_iter().map(|r| json!(&t[r])).collect::<Vec<_>>()).collect();
        assert_eq!(Value::Array(matches), c["matches"], "{name}: matches");
    }
}

fn colors() -> MarkColors {
    MarkColors { matched: hsla(0.1, 1.0, 0.5, 0.3), current: hsla(0.1, 1.0, 0.5, 0.6) }
}

#[test]
fn the_current_match_numbers_across_blocks() {
    let blocks = parse("foo\n\n- foo\n\n| foo |\n|---|\n| foo |");
    let spec = HighlightSpec { query: "foo".into(), current: Some(2), before: None };
    let marks = markdown_marks(&blocks, &spec, colors());
    let current: Vec<_> = {
        let mut keys: Vec<_> = marks.keys().copied().collect();
        keys.sort_unstable();
        keys.into_iter().filter(|k| marks[k].iter().any(|(_, c)| *c == colors().current)).collect()
    };
    assert_eq!(marks.len(), 4);
    // Paragraph, list item, then the header cell is the third.
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].1, 0, "the header row");
}

#[test]
fn runs_split_at_the_marks() {
    let run = |len| TextRun { len, font: font("Inter"), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let c = colors();
    let out = mark_runs(vec![run(4), run(6)], &[(2..6, c.matched), (8..10, c.current)]);
    let lens: Vec<_> = out.iter().map(|r| (r.len, r.background_color)).collect();
    assert_eq!(lens, vec![(2, None), (2, Some(c.matched)), (2, Some(c.matched)), (2, None), (2, Some(c.current))]);
}

#[test]
fn wrapped_lines_take_their_piece_of_a_match() {
    let c = colors();
    let source = "say hello world";
    let lines = vec!["say hello".to_string(), "world".to_string()];
    let per_line = line_marks(source, &lines, &[(4..15, c.matched)]);
    assert_eq!(per_line, vec![vec![(4..9, c.matched)], vec![(0..5, c.matched)]]);
    // Joined by one byte, the second line behind a 3-byte direction mark.
    assert_eq!(joined_marks(&per_line, &[9, 5], &[0, 3]), vec![(4..9, c.matched), (13..18, c.matched)]);
}

#[test]
fn inline_code_marks_its_label_and_a_chip_is_a_barrier() {
    let c = colors();
    let code = Inline { text: "\u{a0}\u{a0}".into(), code: true, chip: Some(InlineChip { kind: CODE_CHIP.into(), label: "foo()".into(), source: "foo()".into(), ..Default::default() }), ..Default::default() };
    let host = Inline { text: "\u{a0}".into(), chip: Some(InlineChip { kind: "issue".into(), label: "foo".into(), source: "#A-1".into(), ..Default::default() }), ..Default::default() };
    let spans = vec![Inline { text: "a foo ".into(), ..Default::default() }, code, host, Inline { text: " foo".into(), ..Default::default() }];
    let searched = eui_gpui::paint::highlight::inlines_highlight_text(&spans);
    assert_eq!(searched, "a foo foo()\u{FFFC} foo");
    let marks: Vec<_> = highlight_ranges(&searched, "foo").into_iter().map(|r| (r, c.matched)).collect();
    assert_eq!(marks.len(), 3, "the chip's label is never searched");
    let (text, chips) = split_unit_marks(&spans, &marks);
    // "a foo " + 4 bytes of code placeholder + 2 of the chip's + " foo".
    assert_eq!(text, vec![(2..5, c.matched), (13..16, c.matched)]);
    assert_eq!(chips.get(&0), Some(&vec![(0..3, c.matched)]));
    assert!(chips.get(&1).is_none());
}

struct Plain;
impl HostPlugin for Plain {}

#[gpui::test]
fn highlighted_text_row_and_markdown_paint(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: builtin_theme("neutral"), mode: Mode::Light, host: Rc::new(Plain), ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let tree = json!({
        "id": "root", "component": "Stack", "children": [
            {"id": "t", "component": "Text", "props": {"text": "Crème brûlée and creme", "highlight": {"query": "creme", "current": 1}}},
            {"id": "one", "component": "Text", "props": {"text": "one line creme", "lines": 1, "highlight": {"query": "CREME"}}},
            {"id": "r", "component": "Row", "props": {"title": "foo title", "subtitle": "foo subtitle", "highlight": {"query": "foo", "current": 1}}},
            {"id": "m", "component": "Markdown", "props": {"text": "# Foo\n\nSee `foo()` and **foo**\n\n```\nfoo\n```\n\n| foo |\n|---|\n| x |", "highlight": {"query": "foo", "current": 3}}}
        ]
    });
    view.update(vcx, |v, cx| {
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    for _ in 0..2 {
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
    }
    assert!(view.read_with(vcx, |v, _| v.index_of("m").is_some()));
    // The Row hands its title the highlight and its subtitle the title as `before`.
    let highlight = |id: &str| view.read_with(vcx, |v, _| v.placed_nodes().iter().find(|n| n.id == id && !n.removed).and_then(|n| n.props.get("highlight").cloned()));
    assert_eq!(highlight("r.body.title"), Some(json!({"query": "foo", "current": 1})));
    assert_eq!(highlight("r.body.subtitle"), Some(json!({"query": "foo", "current": 1, "before": "foo title"})));
}
