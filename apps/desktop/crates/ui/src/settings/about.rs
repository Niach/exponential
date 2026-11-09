//! Settings → About (EXP-262): version, project links, and the third-party
//! licence notice every distributed build must reproduce.
//!
//! The notice is [`crate::licenses::NOTICES`] — the same `include_str!` blob
//! whose embedding EXP-375/EXP-376 gated. EXP-416: it is split into ~18.8k
//! pre-wrapped rows ONCE per process and rendered through `v_virtual_list`.
//! A single `TextView` shaped the whole 822 KB blob as one text run, which
//! froze the window for seconds on every open. Virtual rows cannot be
//! drag-selected, so the card header carries a Copy button — the escape
//! hatch for "reproduce the licence".
//!
//! The list sits in a FIXED-height box on purpose: `SettingsView::render`
//! wraps every pane in `overflow_y_scroll`, so the inner list needs a bounded
//! viewport to virtualize against.
//!
//! #EXP-1629: the pane is a document on eUI's native host binding; the
//! notice is a windowed `List` of one mono `Text` per row (only the rows in
//! view are built), its links and Copy actions route back to the pane.

use std::sync::OnceLock;

use gpui::{div, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render, Styled, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use crate::licenses;

use super::doc::{faded, hex, mount, path, IdeToasts, Ink, HINT_ALPHA, LINE, XS};
use super::{open_url, row_stroke};

const SOURCE_URL: &str = "https://github.com/Niach/exponential";
const LICENSE_URL: &str = "https://github.com/Niach/exponential/blob/master/LICENSE";

/// The locked cross-client blurb — same wording on web, iOS and Android.
const LICENSES_BLURB: &str = "Exponential is built with open-source software. \
     These licenses cover the components bundled in this build.";

/// Uniform virtual-row height for the notice (the `diff.rs` code-row rhythm).
const NOTICE_ROW_H: f32 = 18.;
/// The licence-text convention, and what the 480px box fits without clipping.
const NOTICE_WRAP_COLS: usize = 80;
/// Mono size of a notice row (`diff.rs` `CODE_TEXT_SIZE` neighbourhood).
const NOTICE_TEXT_SIZE: f32 = 12.;

/// [`licenses::NOTICES`] split into display rows ONCE per process: one row per
/// line, long lines word-wrapped at [`NOTICE_WRAP_COLS`]. Rows are subslices
/// of the `include_str!` blob, so the cache costs one pointer pair per row and
/// rendering one never allocates.
fn notice_rows() -> &'static [&'static str] {
    static ROWS: OnceLock<Vec<&'static str>> = OnceLock::new();
    ROWS.get_or_init(|| wrap_notice_lines(licenses::NOTICES, NOTICE_WRAP_COLS))
}

/// Greedy word wrap at `cols` characters: break at the last space that fits,
/// hard-break runs with no space in them (URLs, rule lines). The break space
/// itself is dropped; every other byte survives in order.
fn wrap_notice_lines(text: &'static str, cols: usize) -> Vec<&'static str> {
    // A zero width would never advance — the loop below must always consume.
    let cols = cols.max(1);
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while rest.chars().count() > cols {
            // Byte index of the first char PAST the budget (char boundary, so
            // non-ASCII licence bodies can't panic the slice).
            let limit = rest
                .char_indices()
                .nth(cols)
                .map(|(ix, _)| ix)
                .unwrap_or(rest.len());
            let split = rest[..limit].rfind(' ').filter(|ix| *ix > 0).unwrap_or(limit);
            rows.push(&rest[..split]);
            rest = rest[split..].trim_start_matches(' ');
        }
        rows.push(rest);
    }
    rows
}

/// The About pane (`SettingsSection::About`) — everything it shows is compiled
/// in; the document is built once and never re-projected.
pub struct AboutPane {
    _binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

impl AboutPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let rows: Vec<Value> = notice_rows().iter().map(|row| json!({"t": row})).collect();
        let state = json!({"version": format!("Exponential · Version {}", domain::client_version::current_version()), "notice": rows});
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, state, |state: &Value, _| state.clone());
            binding.set_toasts(std::rc::Rc::new(IdeToasts));
            binding.on_action("open", |_, event, cx| {
                if let Some(url) = event.context.get("url").and_then(Value::as_str) {
                    open_url(cx, url.to_string());
                }
                None
            });
            binding.on_action("copy", |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(licenses::NOTICES.to_string()));
                None
            });
            binding
        });
        let theme = cx.theme();
        let frame = Frame { radius: f32::from(theme.radius), stroke: hex(row_stroke(cx)), foreground: hex(theme.foreground) };
        let view = mount("settings-about", document(&Ink::of(cx), &frame), &binding, window, cx);
        Self { _binding: binding, view }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    /// The notice scrolled to its last row (the pictures' end state).
    #[cfg(test)]
    pub(crate) fn show_end(&mut self, cx: &mut gpui::Context<Self>) {
        let last = notice_rows().len().saturating_sub(1) as u32;
        self.view.update(cx, |view, cx| view.scroll_to_index("notice_rows", last, Some("end"), cx));
    }
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// The notice box's chrome: gpui-component's `theme.radius`, the rows'
/// hairline (`row_stroke`) and the window's text colour.
struct Frame {
    radius: f32,
    stroke: String,
    foreground: String,
}

/// `settings::section` (`gap_2`) of a header block (the band over
/// `section_description`) and its body.
fn section(ink: &Ink, id: &str, label: &str, trailing: Option<Value>, description: Value, body: Value) -> Value {
    let mut text = faded(ink.text(&format!("{id}_description"), description, XS, &ink.hint), HINT_ALPHA);
    text["style"]["paddingHorizontal"] = json!(ink.r(0.25));
    text["style"]["paddingBottom"] = json!(ink.r(0.5));
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            {"id": format!("{id}_head"), "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [ink.band(&format!("{id}_band"), label, trailing), text]},
            body,
        ]})
}

/// The pane's tree: About (version + the two link pills), then the
/// third-party licences (Copy in the band, the notice in a 480px box).
fn document(ink: &Ink, frame: &Frame) -> Value {
    let link = |id: &str, label: &str, url: &str| {
        let [pill, _] = ink.pill_button(id, Some("ui-external-link"), json!(label), json!(false), json!({"name": "open", "context": {"url": url}}));
        pill
    };
    let links = json!({"id": "links", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": ink.r(0.5)},
        "children": [link("source", "Source code", SOURCE_URL), link("license", "License (Apache-2.0)", LICENSE_URL)]});
    let about = section(ink, "about", "About", None, path("/pane/version"), links);

    let [copy, _] = ink.pill_button("copy", None, json!("Copy"), json!(false), json!({"name": "copy"}));
    let copy = json!({"id": "copy_tip", "component": "Tooltip", "props": {"content": "Copy the full notice"}, "children": [copy]});
    let size = NOTICE_TEXT_SIZE;
    let row = json!({"id": "notice_row", "component": "Text", "props": {"text": path("t")},
        "style": {"height": NOTICE_ROW_H, "flexShrink": 0, "fontSize": size, "lineHeight": size * LINE, "fontFamily": ink.mono, "color": frame.foreground}});
    let notice = json!({"id": "notice", "component": "Box",
        "style": {"height": 480, "width": "100%", "overflow": "hidden", "borderRadius": frame.radius, "borderWidth": 1, "borderColor": frame.stroke, "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5)},
        "children": [{"id": "notice_rows", "component": "List", "props": {"gap": "none", "style": {"height": "100%", "width": "100%"}},
            "template": {"component": "notice_row", "path": "/pane/notice"},
            "children": [row]}]});
    let licenses = section(ink, "licenses", "Third-party licenses", Some(copy), json!(LICENSES_BLURB), notice);

    // The pane: `v_flex().gap_6()`.
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%"},
        "children": [about, licenses]})
}

impl Render for AboutPane {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AboutPane").entered();
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lines_pass_through_untouched() {
        let rows = wrap_notice_lines("MIT License\n\nCopyright (c) 2026\n", 80);
        assert_eq!(rows, vec!["MIT License", "", "Copyright (c) 2026"]);
    }

    #[test]
    fn long_lines_break_at_a_word_boundary() {
        let text = "aaaa bbbb cccc dddd eeee";
        let rows = wrap_notice_lines(text, 10);
        assert_eq!(rows, vec!["aaaa bbbb", "cccc dddd", "eeee"]);
        // Only the break spaces are lost — rejoining restores the line.
        assert_eq!(rows.join(" "), text);
        assert!(rows.iter().all(|row| !row.starts_with(' ')));
    }

    #[test]
    fn unbroken_runs_hard_break_at_the_column() {
        static RUN: &str = "xxxxxxxxxxxxxxxxxxxxxxxxx";
        let rows = wrap_notice_lines(RUN, 10);
        assert_eq!(rows, vec!["xxxxxxxxxx", "xxxxxxxxxx", "xxxxx"]);
        assert_eq!(rows.concat(), RUN);
    }

    #[test]
    fn every_row_fits_the_budget() {
        // The real blob: the wrap is what makes the 480px box readable without
        // horizontal scrolling.
        assert!(notice_rows()
            .iter()
            .all(|row| row.chars().count() <= NOTICE_WRAP_COLS));
    }
}
