//! EXP-261 vendoring: embedded-mode layout regressions.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    StatefulInteractiveElement as _, Styled, TestAppContext, Window, div, px, size,
};

use super::Editor;
use crate::MarkdownEditorEnvironment;
use crate::host::{ImageSourceResolution, ImageSourceResolver};

/// Embedded rows used to be laid out at a fake `100_000px` width clamped by
/// `max_w(relative(1.0))`. Taffy cannot resolve that percentage max against an
/// indefinite parent in its intrinsic pass, so every paragraph was MEASURED
/// unwrapped — one line tall — while paint wrapped it into three. Each block
/// then painted over the rows below it. Rows now take `w_full`.
#[test]
fn embedded_rows_reserve_room_for_their_wrapped_lines() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    // Long enough that every paragraph wraps in the narrow window below.
    let markdown = concat!(
        "i really like how on mobile the actions are merged with the agents view.\n\n",
        "lets adapt this on web as well. we need a new good UI for this agent page please, give me some ideas for this.\n\n",
        "on desktop web there is no need to see running live sessions anymore, they are already in the terminal tab dock. only mobile web needs them.\n\n",
        "the mobile web should look and feel exactly like the native mobile, that already quite good.\n",
    );
    let (editor, cx) = cx.add_window_view({
        let markdown = markdown.to_string();
        move |_window, cx| {
            let mut editor = Editor::from_markdown(cx, markdown.clone(), None);
            editor.embedded = true;
            editor
        }
    });
    cx.simulate_resize(size(px(600.), px(900.)));
    // Two frames: the first assigns bounds, the second reads them back.
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let bounds = editor.read_with(cx, |editor, cx| {
        editor
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.read(cx).last_bounds.expect("painted"))
            .collect::<Vec<_>>()
    });
    assert_eq!(bounds.len(), 4);
    // Every paragraph wraps, so none of them is a single line…
    for (index, bound) in bounds.iter().enumerate() {
        assert!(
            bound.size.height > px(30.),
            "block {index} was measured unwrapped: {:?}",
            bound.size
        );
    }
    // …and no block may start before the previous one ends.
    for pair in bounds.windows(2) {
        assert!(
            pair[1].top() >= pair[0].bottom(),
            "rows overlap: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

/// EXP-421: a host resolver that knows a natural size far wider than the
/// window, so the image row takes a definite `w(px(budget))` width. `resolve`
/// falls back to the default local classification — the missing file renders
/// the fallback placeholder, but gpui's `Img` keeps its OWN style for layout,
/// so the definite width still applies.
struct WideImageResolver;

impl ImageSourceResolver for WideImageResolver {
    fn resolve(&self, _src: &str) -> Option<ImageSourceResolution> {
        None
    }

    fn natural_size(&self, _src: &str) -> Option<(f32, f32)> {
        Some((2000.0, 800.0))
    }
}

/// EXP-421: mimics the host mount chain (`WysiwygDescription` inside a scroll
/// region): a fixed-width column NARROWER than the viewport (so the first
/// frame's viewport-derived image budget overshoots the slot) → scroll
/// container → stretched flex column → the embedded editor.
struct EmbeddedHostHarness {
    editor: Entity<Editor>,
}

impl Render for EmbeddedHostHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().w(px(400.)).h_full().child(
            div().id("host-scroll").flex_1().overflow_y_scroll().child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .child(self.editor.clone()),
            ),
        )
    }
}

/// EXP-421 image shrink ratchet: under a fit-content host slot the recorded
/// width followed the editor's CONTENT, so a wide image row fed its own
/// `measured - 2` budget back into the next frame's measurement and shrank by
/// 2px per frame (reproduced: 850 → 848 → 846 → …). The listener now measures
/// the editor's SLOT (an outer `w_full().min_w(0)` wrapper), so under a
/// width-constrained host the recorded width IS the slot width and never
/// derives from an image child. A host that mounts the editor without a
/// constrained width still degrades to fit-content — which is why every host
/// slot must be `w_full().min_w(px(0.))` (EXP-417 ruling 4).
#[test]
fn embedded_wide_image_recorded_width_is_stable_across_frames() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let markdown = "alpha\n\n![wide](/missing/wide.png)\n\nbeta\n";
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let environment = MarkdownEditorEnvironment {
                image_source_resolver: Some(Arc::new(WideImageResolver)),
                ..Default::default()
            };
            let mut editor = Editor::with_environment(markdown.to_string(), environment, cx);
            editor.embedded = true;
            editor
        });
        EmbeddedHostHarness { editor }
    });
    cx.simulate_resize(size(px(900.), px(900.)));

    let mut recorded = Vec::new();
    for _ in 0..6 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        recorded.push(harness.read_with(cx, |harness, cx| {
            let editor = harness.editor.read(cx);
            f32::from_bits(editor.environment.layout_width.load(Ordering::Relaxed))
        }));
    }

    let settled = recorded[2];
    assert!(
        settled > 1.0,
        "the slot width was never recorded: {recorded:?}"
    );
    // The recorded width is the 400px SLOT, not the (initially wider)
    // viewport-derived image width…
    assert!(
        (settled - 400.0).abs() <= 1.0,
        "recorded width is not the slot width: {recorded:?}"
    );
    // …and frames 3..6 must agree — a monotonic decline is the ratchet.
    for width in recorded.iter().skip(2) {
        assert!(
            (width - settled).abs() <= 0.5,
            "recorded slot width ratcheted across frames: {recorded:?}"
        );
    }
}


/// EXP-421 soft wrap (a): an unbroken 400-char run must wrap at the slot —
/// the measured-layout MaxContent branch now resolves to the recorded slot
/// width instead of the viewport, so the run cannot report a near-window
/// intrinsic width.
#[test]
fn embedded_unbroken_run_wraps_at_the_slot() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let markdown = format!("{}\n\nbeta\n", "x".repeat(400));
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let mut editor =
                Editor::with_environment(markdown.clone(), MarkdownEditorEnvironment::default(), cx);
            editor.embedded = true;
            editor
        });
        EmbeddedHostHarness { editor }
    });
    cx.simulate_resize(size(px(900.), px(900.)));
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let bounds = harness.read_with(cx, |harness, cx| {
        let editor = harness.editor.read(cx);
        editor
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.read(cx).last_bounds.expect("painted"))
            .collect::<Vec<_>>()
    });
    assert_eq!(bounds.len(), 2);
    assert!(
        bounds[0].size.width <= px(401.),
        "the unbroken run claimed more than the 400px slot: {:?}",
        bounds[0].size
    );
    assert!(
        bounds[0].size.height > px(30.),
        "the unbroken run did not wrap: {:?}",
        bounds[0].size
    );
}

/// EXP-421 soft wrap (b): the same unbroken run BESIDE inline math rides the
/// mixed-inline flex_wrap path, where `inline_word_chunks` keeps it one
/// unsplittable flex item — the recorded-px `max_w` cap makes it wrap inside
/// its own element instead of overflowing the row as one line.
#[test]
fn embedded_unbroken_run_beside_inline_math_wraps_at_the_slot() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let markdown = format!("$E=mc^2$ {}\n\nbeta\n", "y".repeat(300));
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let mut editor =
                Editor::with_environment(markdown.clone(), MarkdownEditorEnvironment::default(), cx);
            editor.embedded = true;
            editor
        });
        EmbeddedHostHarness { editor }
    });
    cx.simulate_resize(size(px(900.), px(900.)));
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    // The mixed-inline block renders div runs (no BlockTextElement), so read
    // the FOLLOWING paragraph: if the run wrapped, the mixed block is several
    // lines tall and `beta` starts well below it.
    let beta_bounds = harness.read_with(cx, |harness, cx| {
        let editor = harness.editor.read(cx);
        let blocks = editor.document.visible_blocks();
        assert_eq!(blocks.len(), 2);
        blocks[1]
            .entity
            .read(cx)
            .last_bounds
            .expect("beta painted")
    });
    assert!(
        beta_bounds.size.width <= px(401.),
        "beta claimed more than the 400px slot: {beta_bounds:?}"
    );
    assert!(
        beta_bounds.top() > px(100.),
        "the mixed-inline run did not wrap (beta sits too high): {beta_bounds:?}"
    );
}

/// EXP-421 image hit-testing: the standalone-image branch never mounts a
/// `BlockTextElement`, so the image row painted without `last_bounds` and the
/// selection resolvers skipped it (gap clicks around images mis-resolved).
/// The in-flow prepaint recorder now writes the row bounds.
#[test]
fn embedded_image_rows_record_bounds() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let markdown = "alpha\n\n![a](/missing/p.png)\n\nbeta\n";
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let mut editor =
                Editor::with_environment(markdown.to_string(), MarkdownEditorEnvironment::default(), cx);
            editor.embedded = true;
            editor
        });
        EmbeddedHostHarness { editor }
    });
    cx.simulate_resize(size(px(900.), px(900.)));
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let bounds = harness.read_with(cx, |harness, cx| {
        let editor = harness.editor.read(cx);
        editor
            .document
            .visible_blocks()
            .iter()
            .enumerate()
            .map(|(index, visible)| {
                visible
                    .entity
                    .read(cx)
                    .last_bounds
                    .unwrap_or_else(|| panic!("block {index} painted without bounds"))
            })
            .collect::<Vec<_>>()
    });
    // alpha, the image row, beta — every one has bounds…
    assert_eq!(bounds.len(), 3);
    // …and they are vertically ordered (the resolvers walk them top-down).
    for pair in bounds.windows(2) {
        assert!(
            pair[1].top() >= pair[0].top(),
            "rows out of order: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

/// EXP-520 (ex EXP-436): the block-hop width leak is fixed by Taffy 0.12.
/// This host chain is the ORIGINAL failure shape — the embedded editor's
/// percent width reaching a display-BLOCK `max_w + mx_auto` centered column
/// inside a wide window. Under the old pinned taffy the percent resolved
/// against the UNCLAMPED window width, so a paragraph that fits unwrapped at
/// 1920px settled as one clipped line far past the column; the (now deleted)
/// wrap-budget clamp in `BlockTextElement`'s measure closure papered over it.
/// At the new pin the stretch resolves against the 640px column with no clamp.
struct CenteredBlockHostHarness {
    editor: Entity<Editor>,
}

impl Render for CenteredBlockHostHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Deliberately display-BLOCK hops (no `.flex()`), mirroring the
        // EXP-436 repro chain.
        div().w_full().h_full().child(
            div()
                .max_w(px(640.))
                .mx_auto()
                .child(self.editor.clone()),
        )
    }
}

#[test]
fn embedded_paragraph_wraps_at_a_centered_block_column_in_a_wide_window() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    // Fits on ONE unwrapped line at 1920px, but must wrap inside 640px.
    let markdown = concat!(
        "the quick brown fox jumps over the lazy dog and keeps running ",
        "until the centered column finally makes it wrap onto more lines.\n",
    );
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let mut editor = Editor::with_environment(
                markdown.to_string(),
                MarkdownEditorEnvironment::default(),
                cx,
            );
            editor.embedded = true;
            editor
        });
        CenteredBlockHostHarness { editor }
    });
    cx.simulate_resize(size(px(1920.), px(900.)));
    for _ in 0..4 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let bounds = harness.read_with(cx, |harness, cx| {
        let editor = harness.editor.read(cx);
        let blocks = editor.document.visible_blocks();
        assert_eq!(blocks.len(), 1);
        blocks[0].entity.read(cx).last_bounds.expect("painted")
    });
    assert!(
        bounds.size.width <= px(641.),
        "the paragraph claimed more than the 640px centered column: {:?}",
        bounds.size
    );
    assert!(
        bounds.size.height > px(30.),
        "the paragraph did not wrap at the column (the EXP-436 leak is back): {:?}",
        bounds.size
    );
}

/// EXP-520 (ex EXP-335): an image sized EXACTLY to the row's content width no
/// longer trips the old taffy fit-content edge case that re-measured the whole
/// host column at min-content width (~4× its real height). The 2px safety
/// margin in `container_image_width_budget` is gone — this locks its absence:
/// with a row-width image, neighbor paragraphs must still lay out one line
/// tall (a min-content re-measure would stack them one word per line).
#[test]
fn embedded_row_width_image_does_not_collapse_neighbors_to_min_content() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let markdown =
        "alpha words that fit on one line\n\n![wide](/missing/wide.png)\n\nbeta words that fit on one line\n";
    let (harness, cx) = cx.add_window_view(move |_window, cx| {
        let editor = cx.new(|cx| {
            let environment = MarkdownEditorEnvironment {
                image_source_resolver: Some(Arc::new(WideImageResolver)),
                ..Default::default()
            };
            let mut editor = Editor::with_environment(markdown.to_string(), environment, cx);
            editor.embedded = true;
            editor
        });
        EmbeddedHostHarness { editor }
    });
    cx.simulate_resize(size(px(900.), px(900.)));
    for _ in 0..6 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }

    let bounds = harness.read_with(cx, |harness, cx| {
        let editor = harness.editor.read(cx);
        editor
            .document
            .visible_blocks()
            .iter()
            .map(|visible| visible.entity.read(cx).last_bounds.expect("painted"))
            .collect::<Vec<_>>()
    });
    assert_eq!(bounds.len(), 3);
    // The paragraphs stay one line tall — a min-content collapse would make
    // them several hundred px…
    for index in [0usize, 2] {
        assert!(
            bounds[index].size.height <= px(30.),
            "paragraph {index} collapsed toward min-content: {:?}",
            bounds[index].size
        );
    }
    // …and the rows stay vertically ordered below the image row.
    for pair in bounds.windows(2) {
        assert!(
            pair[1].top() >= pair[0].bottom(),
            "rows overlap: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}
