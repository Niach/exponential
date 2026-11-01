//! #EXP-1546 — the thread transcript scrolled over a VERY long plan card
//! (headings, prose, lists, code, tables, many pictures, an inline eUI
//! surface), timed frame by frame. The rows are the Conversation's own
//! ([`crate::thread_screen::card_row`], [`crate::thread_screen::agent_prose`])
//! in the Conversation's `gpui::list`, drawn HEADLESS under the platform's
//! real text system. Run with
//! `cargo test -p ui --release -- --ignored bench_long_plan_transcript --nocapture`.
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    div, px, size, AppContext as _, Entity, HeadlessAppContext, IntoElement, ListAlignment, ListState,
    ParentElement as _, Render, Styled as _, Window,
};
use gpui_component::v_flex;

use domain::session_tree::{CardSurface, SessionThreadRow};

use crate::markdown::ImageCache;
use crate::thread_screen::{agent_prose, card_row, message_markdown, ThreadText};

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
        root: serde_json::json!({"id": "s", "component": "Stack", "children": rows}),
        data: None,
    }
}

pub(crate) fn rows() -> Vec<SessionThreadRow> {
    let mut rows = Vec::new();
    for n in 0..12 {
        rows.push(SessionThreadRow::Message {
            id: format!("m{n}"),
            author: domain::contract::MESSAGE_AUTHOR_KIND_AGENT.into(),
            author_id: None,
            body: format!("Narration {n}: reading the code and the fixtures before the plan. ").repeat(6),
            queued: false,
            xref: None,
            author_session_id: None,
        });
    }
    rows.push(SessionThreadRow::Plan {
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
        rows.push(SessionThreadRow::Message {
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
    rows: Vec<SessionThreadRow>,
    list: ListState,
    images: Entity<ImageCache>,
    text: ThreadText,
}

impl Render for Transcript {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::list(
            self.list.clone(),
            cx.processor(|this, ix: usize, window, cx| {
                let row = this.rows[ix].clone();
                let content = match &row {
                    SessionThreadRow::Message { id, body, .. } => agent_prose(
                        message_markdown(format!("thread-message-{id}").into(), body, &this.text, &this.images),
                        cx,
                    ),
                    _ => card_row(&row, "root", &this.rows, &this.text, &this.images, window, cx)
                        .unwrap_or_else(|| div().into_any_element()),
                };
                // The Conversation's reading column (`render_list_row`).
                div()
                    .w_full()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(crate::thread_screen::CONVERSATION_W))
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

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.) * p).round() as usize]
}

/// One mode's numbers: the scroll frames' median + p95 (ms), the plan
/// card's height, the lazy children built per scroll frame and the picture
/// of the viewport halfway down the plan.
struct Run {
    median: f64,
    p95: f64,
    plan_height: f32,
    built_per_frame: f64,
    shot: Option<image::RgbaImage>,
}

fn run(eager: bool) -> Run {
    crate::markdown::lazy_block::FORCE_EAGER.with(|force| force.set(eager));
    let platform = gpui_platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(gpui_component_assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let rows = rows();
    let count = rows.len();
    let list = ListState::new(count, ListAlignment::Top, crate::thread_screen::CONVERSATION_OVERDRAW);
    let state = list.clone();
    let window = cx
        .open_window(size(px(VIEWPORT.0), px(VIEWPORT.1)), move |_, cx| {
            let images = cx.new(|_| ImageCache::new(None));
            cx.new(|_| Transcript { rows, list: state, images, text: ThreadText::team(None) })
        })
        .expect("a headless window");
    let frame = |cx: &mut HeadlessAppContext| {
        let started = Instant::now();
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("a frame draws");
        started.elapsed().as_secs_f64() * 1000.
    };
    // Warm: the plan card on screen, measured at its width.
    let plan_ix = 12;
    list.scroll_to(gpui::ListOffset { item_ix: plan_ix, offset_in_item: px(0.) });
    for _ in 0..4 {
        frame(&mut cx);
    }
    let plan_height = list.bounds_for_item(plan_ix).map(|b| f32::from(b.size.height)).unwrap_or(0.);
    // Scroll down THROUGH the plan card, one trackpad step per frame.
    let steps = ((plan_height / SCROLL_STEP) as usize).clamp(50, 400);
    crate::markdown::lazy_block::BUILT.with(|built| built.set(0));
    let mut times: Vec<f64> = (0..steps)
        .map(|_| {
            list.scroll_by(px(SCROLL_STEP));
            frame(&mut cx)
        })
        .collect();
    let built_per_frame = crate::markdown::lazy_block::BUILT.with(std::cell::Cell::get) as f64 / steps as f64;
    times.sort_by(f64::total_cmp);
    // The picture: halfway down the plan.
    list.scroll_to(gpui::ListOffset { item_ix: plan_ix, offset_in_item: px((plan_height / 2.).round()) });
    frame(&mut cx);
    frame(&mut cx);
    let shot = cx.capture_screenshot(window.into()).ok();
    // The headless app's leak check would fail the views still alive.
    std::mem::forget(cx);
    crate::markdown::lazy_block::FORCE_EAGER.with(|force| force.set(false));
    Run { median: percentile(&times, 0.5), p95: percentile(&times, 0.95), plan_height, built_per_frame, shot }
}

#[test]
#[ignore]
fn bench_long_plan_transcript() {
    let before = run(true);
    let after = run(false);
    for (label, run) in [("before (eager)", &before), ("after (lazy)", &after)] {
        eprintln!(
            "{label:<15} plan {:.0} px  scroll frame median {:.2} ms  p95 {:.2} ms  ({:.1} children built / frame)",
            run.plan_height, run.median, run.p95, run.built_per_frame
        );
    }
    // No visual change: the same card height, the same pixels.
    assert_eq!(before.plan_height, after.plan_height, "the lazy card keeps its height");
    if let (Some(a), Some(b)) = (&before.shot, &after.shot) {
        let same = a.dimensions() == b.dimensions() && a.as_raw() == b.as_raw();
        eprintln!("pictures identical: {same}");
        if let Ok(dir) = std::env::var("EXP_TRANSCRIPT_SHOTS") {
            a.save(format!("{dir}/long-plan-before.png")).expect("before saved");
            b.save(format!("{dir}/long-plan-after.png")).expect("after saved");
        }
        assert!(same, "the lazy transcript paints the same pixels");
    }
}
