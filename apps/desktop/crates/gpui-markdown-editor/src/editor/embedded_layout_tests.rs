//! EXP-261 vendoring: embedded-mode layout regressions.

use gpui::{TestAppContext, px, size};

use super::Editor;

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
        cx.update(|window, cx| window.draw(cx).clear());
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
