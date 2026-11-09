//! #EXP-1546 / #VAPP-258 — the read-only Markdown timed HEADLESS under the
//! platform's real text system: the chat transcript scrolled over a VERY
//! long plan card (headings, prose, lists, code, tables, many pictures, an
//! inline eUI surface), a long description laid out cold, and an agent reply
//! streamed in chunk by chunk. The rows are the Conversation's own
//! ([`crate::session_chat::card_row`], [`crate::session_chat::transcript_message`])
//! in the Conversation's `gpui::list`. Run with
//! `cargo test -p ui --release -- --ignored bench_ --nocapture`.
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    div, px, size, AppContext as _, Entity, HeadlessAppContext, IntoElement, ListAlignment, ListState,
    ParentElement as _, Render, Styled as _, Window,
};
use gpui_component::v_flex;

use domain::session_tree::{CardSurface, SessionChatRow};

use crate::markdown::ImageCache;
use crate::session_chat::{card_row, transcript_message, ChatText};

/// The plan's sections: one = a heading, prose with marks and refs, a
/// list, a code block, a table and two pictures.
const SECTIONS: usize = 40;
const SCROLL_STEP: f32 = 48.;
const VIEWPORT: (f32, f32) = (1100., 900.);

pub(crate) fn long_plan(sections: usize) -> String {
    let mut out = String::from("# Plan\n\nThe whole change, step by step.\n\n");
    for n in 0..sections {
        out.push_str(&format!("## {n}. Step number {n}\n\n"));
        for p in 0..3 {
            out.push_str(&format!(
                "Paragraph {p} of step {n} explains **why** the change lands in `crates/ui/src/file_{n}.rs`, \
                 what it touches (see #EXP-{n}) and how [the docs](https://example.com/{n}) read. \
                 It wraps over several lines at the reading column's measure, like real prose does.\n\n"
            ));
        }
        for i in 0..6 {
            out.push_str(&format!("- item {i}: a bullet with `code` and *emphasis* in step {n}\n"));
        }
        out.push_str("\n```rust\n");
        for line in 0..12 {
            out.push_str(&format!("fn step_{n}_line_{line}() -> usize {{ {line} * {n} }}\n"));
        }
        out.push_str("```\n\n| Column | Before | After |\n| --- | --- | --- |\n");
        for row in 0..4 {
            out.push_str(&format!("| row {row} | {} ms | {} ms |\n", row * 3 + n, row + 1));
        }
        out.push_str(&format!(
            "\n![before {n}](/api/attachments/00000000-0000-4000-8000-{n:012})\n\n\
             ![after {n}](/api/attachments/00000000-0000-4000-9000-{n:012})\n\n"
        ));
    }
    out
}

fn surface() -> CardSurface {
    let rows: Vec<serde_json::Value> = (0..24)
        .map(|n| {
            serde_json::json!({
                "id": format!("r{n}"),
                "component": "Row",
                "props": {"title": format!("Screen {n}"), "subtitle": "before → after"},
            })
        })
        .collect();
    CardSurface {
        version: None,
        root: serde_json::json!({"id": "s", "component": "Stack", "children": rows}),
        data: None,
    }
}

pub(crate) fn rows() -> Vec<SessionChatRow> {
    let mut rows = Vec::new();
    for n in 0..12 {
        rows.push(SessionChatRow::Message {
            id: format!("m{n}"),
            author: domain::contract::MESSAGE_AUTHOR_KIND_AGENT.into(),
            author_id: None,
            body: format!("Narration {n}: reading the code and the fixtures before the plan. ").repeat(6),
            queued: false,
            xref: None,
            author_session_id: None,
        });
    }
    rows.push(SessionChatRow::Plan {
        id: "plan".into(),
        session_id: "s".into(),
        plan: long_plan(SECTIONS),
        questions: Vec::new(),
        surface: Some(surface()),
        decision: None,
        fresh: false,
        answers: None,
        feedback: None,
        surface_data: None,
    });
    for n in 12..18 {
        rows.push(SessionChatRow::Message {
            id: format!("m{n}"),
            author: domain::contract::MESSAGE_AUTHOR_KIND_AGENT.into(),
            author_id: None,
            body: format!("Narration {n} after the plan. ").repeat(8),
            queued: false,
            xref: None,
            author_session_id: None,
        });
    }
    rows
}

struct Transcript {
    rows: Vec<SessionChatRow>,
    list: ListState,
    images: Entity<ImageCache>,
    text: ChatText,
}

impl Render for Transcript {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Transcript").entered();
        gpui::list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| {
                let row = this.rows[ix].clone();
                // #EXP-1736: a message = THE bubble, every card as it is.
                let content = match transcript_message(&row, None, "chat-message", None, &this.text, &this.images, cx) {
                    Some(bubble) => bubble,
                    None => card_row(&row, "root", &this.rows, &this.text, &this.images, window, cx)
                        .unwrap_or_else(|| div().into_any_element()),
                };
                // The Conversation's reading column (`render_list_row`).
                div()
                    .w_full()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(crate::session_chat::CONVERSATION_W))
                            .mx_auto()
                            .px(px(crate::work_header::WORK_GUTTER))
                            .pt_5()
                            .child(content),
                    )
                    .into_any_element()
            }),
        )
        .size_full()
    }
}

pub(crate) fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.) * p).round() as usize]
}

fn headless() -> HeadlessAppContext {
        let mut cx = HeadlessAppContext::with_platform(
        crate::test_text_system(),
        Arc::new(gpui_component_assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    cx
}

pub(crate) fn draw(window: gpui::AnyWindowHandle, cx: &mut HeadlessAppContext) -> f64 {
    let started = Instant::now();
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .expect("a frame draws");
    started.elapsed().as_secs_f64() * 1000.
}

fn report(label: &str, mut times: Vec<f64>) {
    times.sort_by(f64::total_cmp);
    eprintln!("{label:<34} median {:>8.2} ms  p95 {:>8.2} ms  ({} frames)", percentile(&times, 0.5), percentile(&times, 0.95), times.len());
}

#[test]
#[ignore]
fn bench_long_plan_transcript() {
    let mut cx = headless();
    let rows = rows();
    let count = rows.len();
    let list = ListState::new(count, ListAlignment::Top, crate::session_chat::CONVERSATION_OVERDRAW);
    let state = list.clone();
    let window = cx
        .open_window(size(px(VIEWPORT.0), px(VIEWPORT.1)), move |_, cx| {
            let images = cx.new(|_| ImageCache::new(None));
            cx.new(|_| Transcript { rows, list: state, images, text: ChatText::team(None) })
        })
        .expect("a headless window");
    let plan_ix = 12;
    list.scroll_to(gpui::ListOffset { item_ix: plan_ix, offset_in_item: px(0.) });
    for _ in 0..4 {
        draw(window.into(), &mut cx);
    }
    let plan_height = list.bounds_for_item(plan_ix).map(|b| f32::from(b.size.height)).unwrap_or(0.);
    let steps = ((plan_height / SCROLL_STEP) as usize).clamp(50, 400);
    let times: Vec<f64> = (0..steps)
        .map(|_| {
            list.scroll_by(px(SCROLL_STEP));
            draw(window.into(), &mut cx)
        })
        .collect();
    eprintln!("plan card {plan_height:.0} px");
    report("transcript scroll frame", times);
    std::mem::forget(cx);
}

/// One document view at the reading column's width.
struct Doc {
    id: usize,
    text: String,
    chat: bool,
    streaming: bool,
    images: Entity<ImageCache>,
}

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Doc").entered();
        let view = crate::markdown::MarkdownView::new(format!("bench-doc-{}", self.id), self.text.clone())
            .selectable(true)
            .chat(self.chat)
            .autolink(self.chat)
            .images(self.images.clone())
            .streaming(self.streaming); // BENCH-STREAMING
        div().size_full().child(div().w(px(crate::session_chat::CONVERSATION_W)).child(view))
    }
}

/// A long description (the plan's sections as a document), laid out and
/// painted COLD (a fresh view id each frame: nothing memoized), then warm.
#[test]
#[ignore]
fn bench_long_description() {
    let mut cx = headless();
    let text = long_plan(SECTIONS);
    let window = cx
        .open_window(size(px(VIEWPORT.0), px(VIEWPORT.1)), move |_, cx| {
            let images = cx.new(|_| ImageCache::new(None));
            cx.new(|_| Doc { id: 0, text, chat: false, streaming: false, images })
        })
        .expect("a headless window");
    draw(window.into(), &mut cx);
    let cold: Vec<f64> = (1..=20)
        .map(|n| {
            cx.update_window(window.into(), |root, _, cx| {
                let doc = root.downcast::<Doc>().expect("the doc");
                doc.update(cx, |doc, _| doc.id = n);
            })
            .expect("the doc updates");
            draw(window.into(), &mut cx)
        })
        .collect();
    let warm: Vec<f64> = (0..40).map(|_| draw(window.into(), &mut cx)).collect();
    eprintln!("long description: {} bytes", long_plan(SECTIONS).len());
    report("long description, cold frame", cold);
    report("long description, warm frame", warm);
    std::mem::forget(cx);
}

/// An agent reply streamed in: 40-byte chunks, one frame per chunk.
#[test]
#[ignore]
fn bench_streamed_reply() {
    let mut cx = headless();
    let full = long_plan(6);
    let window = cx
        .open_window(size(px(VIEWPORT.0), px(VIEWPORT.1)), move |_, cx| {
            let images = cx.new(|_| ImageCache::new(None));
            cx.new(|_| Doc { id: 0, text: String::new(), chat: true, streaming: true, images })
        })
        .expect("a headless window");
    let mut times = Vec::new();
    let mut at = 0;
    while at < full.len() {
        at = (at + 40).min(full.len());
        while !full.is_char_boundary(at) {
            at += 1;
        }
        let prefix = full[..at].to_string();
        cx.update_window(window.into(), |root, _, cx| {
            let doc = root.downcast::<Doc>().expect("the doc");
            doc.update(cx, |doc, _| doc.text = prefix);
        })
        .expect("the doc updates");
        times.push(draw(window.into(), &mut cx));
    }
    eprintln!("streamed reply: {} bytes", full.len());
    report("streamed reply, frame per chunk", times);
    std::mem::forget(cx);
}

