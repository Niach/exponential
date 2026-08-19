//! EXP-551: the emoji picker popover — the ONE picker surface the description
//! toolbar and the comment composers share (web parity: `emoji-picker.tsx`).
//!
//! Search on top, the skin-tone row next to it, then a "Recent" section
//! followed by the nine dataset groups; a search replaces the grid with its
//! ranked results. A pick records the recent, applies the saved tone and hands
//! the host the UNICODE to insert — never `:shortcode:` text.
//!
//! The grid is a [`uniform_list`] of EQUAL-HEIGHT rows (section headers and
//! 8-cell emoji rows alike), so the 1906 cells cost only what is on screen.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, uniform_list, App, AppContext as _, ElementId, Entity,
    Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    input::{Input, InputEvent, InputState},
    popover::Popover,
    v_flex, ActiveTheme as _, Sizable as _,
};

use crate::emoji;

/// Pick callback: the (skin-toned) unicode the host inserts at its caret.
pub(crate) type OnPickEmoji = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// Emoji per grid row — 8 × 30px cells plus the popover's padding is the
/// PICKER_SEARCH_WIDTH class of width the other searchable pickers use.
const CELLS_PER_ROW: usize = 8;
const CELL_SIZE: f32 = 30.;
/// Every uniform_list row — headers included — is exactly this tall.
const ROW_HEIGHT: f32 = 30.;
const GRID_HEIGHT: f32 = 240.;
/// Popover width (web parity: the grid is the width driver).
pub(crate) const EMOJI_PICKER_WIDTH: f32 = CELLS_PER_ROW as f32 * CELL_SIZE + 16.;
/// Search result cap (web `SEARCH_LIMIT`).
const SEARCH_LIMIT: usize = 64;

/// One rendered row of the grid.
enum GridRow {
    /// A section header ("Recent" / a dataset group label).
    Header(SharedString),
    /// Up to [`CELLS_PER_ROW`] catalog indices.
    Cells(Vec<usize>),
}

pub(crate) struct EmojiPicker {
    query: Entity<InputState>,
    /// 0 = none, 1..=5 light → dark.
    tone: u8,
    /// Base unicodes, most recent first.
    recents: Vec<String>,
    rows: Vec<GridRow>,
    on_pick: OnPickEmoji,
    _subscription: Subscription,
}

impl EmojiPicker {
    pub(crate) fn new(
        on_pick: OnPickEmoji,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji…"));
        let subscription = cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.rebuild_rows(cx);
                cx.notify();
            }
        });
        let mut this = Self {
            query,
            tone: emoji::skin_tone(cx),
            recents: emoji::recent_emoji(cx),
            rows: Vec::new(),
            on_pick,
            _subscription: subscription,
        };
        this.rebuild_rows(cx);
        this
    }

    /// Fresh state for every open (web parity with the other searchable
    /// pickers): empty search, prefs re-read from the per-install file.
    pub(crate) fn reset(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.query
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.tone = emoji::skin_tone(cx);
        self.recents = emoji::recent_emoji(cx);
        self.rebuild_rows(cx);
        self.query.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// Recompute the row plan. Called on every state change rather than per
    /// frame — the group grid is ~250 rows and only changes when the query,
    /// the recents or the tone do.
    fn rebuild_rows(&mut self, cx: &App) {
        let query = self.query.read(cx).value().trim().to_string();
        let catalog = emoji::catalog();
        let mut rows = Vec::new();
        if !query.is_empty() {
            push_cells(&mut rows, &catalog.search(&query, SEARCH_LIMIT));
            self.rows = rows;
            return;
        }
        let recent_indices: Vec<usize> = self
            .recents
            .iter()
            .filter_map(|unicode| catalog.index_of_unicode(unicode))
            .collect();
        if !recent_indices.is_empty() {
            rows.push(GridRow::Header("Recent".into()));
            push_cells(&mut rows, &recent_indices);
        }
        for (group, label) in catalog.groups().iter().enumerate() {
            let members = catalog.group(group);
            if members.is_empty() {
                continue;
            }
            rows.push(GridRow::Header(SharedString::from(label.clone())));
            push_cells(&mut rows, members);
        }
        self.rows = rows;
    }

    fn choose_tone(&mut self, tone: u8, cx: &mut gpui::Context<Self>) {
        if self.tone == tone {
            return;
        }
        self.tone = tone;
        emoji::set_skin_tone(cx, tone);
        // Toned glyphs are baked into the row plan.
        self.rebuild_rows(cx);
        cx.notify();
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let catalog = emoji::catalog();
        let Some(record) = catalog.get(index) else {
            return;
        };
        let unicode = emoji::apply_skin_tone(record, self.tone).to_string();
        let base = record.unicode.clone();
        emoji::push_recent_emoji(cx, &base);
        self.recents = emoji::recent_emoji(cx);
        self.rebuild_rows(cx);
        (self.on_pick.clone())(&unicode, window, cx);
        cx.notify();
    }

    /// Enter in the search field takes the first result (web parity).
    fn on_enter(
        &mut self,
        _: &gpui_component::input::Enter,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let first = self.rows.iter().find_map(|row| match row {
            GridRow::Cells(cells) => cells.first().copied(),
            GridRow::Header(_) => None,
        });
        if let Some(index) = first {
            self.pick(index, window, cx);
            cx.stop_propagation();
        }
    }

    fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let catalog = emoji::catalog();
        let tone = self.tone;
        let muted = cx.theme().muted_foreground;
        let hover = cx.theme().accent;
        let radius = cx.theme().radius;
        range
            .filter_map(|row_index| {
                let row = self.rows.get(row_index)?;
                Some(match row {
                    GridRow::Header(label) => h_flex()
                        .h(px(ROW_HEIGHT))
                        .w_full()
                        .items_center()
                        .px_1()
                        .text_xs()
                        .text_color(muted)
                        .child(label.clone())
                        .into_any_element(),
                    GridRow::Cells(cells) => {
                        let mut grid = h_flex().h(px(ROW_HEIGHT)).w_full().items_center();
                        for &index in cells {
                            let Some(record) = catalog.get(index) else {
                                continue;
                            };
                            let glyph = emoji::apply_skin_tone(record, tone).to_string();
                            grid = grid.child(
                                div()
                                    .id(ElementId::from(("emoji-cell", index)))
                                    .size(px(CELL_SIZE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(radius)
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(hover))
                                    .tooltip({
                                        let label = SharedString::from(record.label.clone());
                                        move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(label.clone())
                                                .build(window, cx)
                                        }
                                    })
                                    .child(div().text_lg().child(SharedString::from(glyph)))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.pick(index, window, cx);
                                    })),
                            );
                        }
                        grid.into_any_element()
                    }
                })
            })
            .collect()
    }
}

/// Chunk `indices` into [`CELLS_PER_ROW`]-wide rows.
fn push_cells(rows: &mut Vec<GridRow>, indices: &[usize]) {
    for chunk in indices.chunks(CELLS_PER_ROW) {
        rows.push(GridRow::Cells(chunk.to_vec()));
    }
}

impl Render for EmojiPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let row_count = self.rows.len();
        let tone = self.tone;
        // Own row under the search field: six 22px swatches plus a search box
        // do not both fit across an 8-cell grid.
        let mut tones = h_flex().gap_0p5().px_1().flex_shrink_0();
        for option in 0..=emoji::EMOJI_TONES as u8 {
            let selected = option == tone;
            tones = tones.child(
                div()
                    .id(ElementId::from(("emoji-tone", option as usize)))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(cx.theme().radius)
                    .cursor_pointer()
                    .when(selected, |el| el.bg(cx.theme().accent))
                    .hover(|style| style.bg(cx.theme().accent))
                    .tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(emoji::TONE_LABELS[option as usize])
                            .build(window, cx)
                    })
                    .child(
                        div()
                            .text_sm()
                            .child(SharedString::from(emoji::TONE_SWATCHES[option as usize])),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_tone(option, cx))),
            );
        }
        v_flex()
            .key_context("EmojiPicker")
            .w(px(EMOJI_PICKER_WIDTH))
            .gap_1()
            .capture_action(cx.listener(Self::on_enter))
            .child(
                Input::new(&self.query)
                    .small()
                    .appearance(false)
                    .cleanable(true),
            )
            .child(tones)
            .child(div().h(px(1.)).w_full().bg(cx.theme().border.opacity(0.5)))
            .map(|column| {
                if row_count == 0 {
                    column.child(
                        div()
                            .h(px(GRID_HEIGHT))
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("No emoji found."),
                    )
                } else {
                    column.child(
                        uniform_list("emoji-grid", row_count, cx.processor(Self::render_rows))
                            .h(px(GRID_HEIGHT))
                            .w_full(),
                    )
                }
            })
    }
}

/// The shared trigger+popover recipe: an icon button that opens the picker.
/// Open state is HOST-owned (like the toolbar's link editor) so a pick can
/// close the popover from the host's own insert path.
pub(crate) fn emoji_picker_popover(
    id: impl Into<ElementId>,
    trigger: Button,
    picker: Entity<EmojiPicker>,
    open: bool,
    on_open_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Popover {
    let picker_for_open = picker.clone();
    Popover::new(id)
        .open(open)
        .w(px(EMOJI_PICKER_WIDTH + 8.))
        .p_1()
        .trigger(trigger)
        .on_open_change(move |open, window, cx| {
            if *open {
                // Fresh search + focus, exactly like the label picker.
                picker_for_open.update(cx, |picker, cx| picker.reset(window, cx));
            }
            on_open_change(open, window, cx);
        })
        .content(move |_, _, _| picker.clone())
}
