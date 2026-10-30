//! EXP-1240 (EXP-1259 D12) — THE Work card's paint: ONE card for a claude
//! workflow, the run's task list and a subagent, replacing the EXP-850
//! workflow card, the EXP-927 task-list block and the subagent group row.
//!
//! What the card SAYS is the pure model `steer::work_card` (the Rust mirror
//! of `packages/domain-contract/src/work-card.ts`, byte-locked by
//! `fixtures/work-card.json`); this module only paints it per the fixture's
//! `$comment` and geometry: a header (kind icon, name, kind word, progress ·
//! elapsed, a spinner while running), the segment bar, then hairline-divided
//! rows. No report text, no token counts, no JSON. A lane row (and a whole
//! subagent card) opens that lane's transcript through the caller's callback.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, relative, AnyElement, App, ClickEvent, ElementId,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, spinner::Spinner, v_flex, ActiveTheme as _, Icon, Sizable as _};
use steer::work_card::{geometry, WorkCard, WorkCardLane, WorkCardRow, WorkState};
use theme::tokens::transcript;

use crate::icons::registry;

/// Which lane a click landed on: `(row, lane)` into the card's rows, the
/// folded row's lanes counting as that row's.
pub(crate) type LaneClick = Rc<dyn Fn((usize, usize), &mut Window, &mut App)>;
/// A click with nothing to say (the folded row's chevron, a subagent card).
pub(crate) type CardClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// Everything one card paint needs beyond the model.
pub(crate) struct WorkCardProps<'a> {
    /// The id prefix every interactive part derives its element id from.
    pub id: SharedString,
    pub card: &'a WorkCard,
    /// The lanes behind each row (`steer::work_card::work_card_lanes`,
    /// row-aligned): the folded row's are drawn while `folded_open`.
    pub lanes: Vec<Vec<WorkCardLane>>,
    pub folded_open: bool,
    pub on_toggle_folded: Option<CardClick>,
    /// Opens a lane's transcript; `None` = lanes are plain lines.
    pub on_lane: Option<LaneClick>,
    /// A subagent card: the WHOLE card opens its transcript.
    pub on_card: Option<CardClick>,
    /// Rows that belong to the card but not to the model (the EXP-856
    /// duplicate warnings), drawn after its rows.
    pub footer: Vec<AnyElement>,
}

/// 12/18 — the transcript's tool rung, the card's text size.
fn tool_text<E: Styled>(element: E) -> E {
    element
        .text_size(px(transcript::TOOL_SIZE))
        .line_height(px(transcript::TOOL_LINE_HEIGHT))
}

/// The hairline between the card's rows.
fn hairline() -> gpui::Hsla {
    theme::tokens::glass::STROKE_ROW.to_hsla()
}

/// The state glyph of a phase or a task row: the 12px spinner while
/// running, a check once done, a rose x when it failed, the dashed circle
/// while pending.
fn state_glyph(state: WorkState, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    match state {
        WorkState::Running => Spinner::new().xsmall().into_any_element(),
        WorkState::Done => Icon::new(registry::UI_CHECK)
            .xsmall()
            .text_color(muted)
            .into_any_element(),
        WorkState::Failed => Icon::new(registry::UI_CLOSE)
            .xsmall()
            .text_color(cx.theme().danger)
            .into_any_element(),
        WorkState::Pending => Icon::new(registry::STATUS_BACKLOG)
            .xsmall()
            .text_color(muted.opacity(0.7))
            .into_any_element(),
    }
}

/// A lane's 6px dot: done = muted, running = green, error = rose, queued =
/// hollow.
fn lane_dot(state: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let dot = div().flex_shrink_0().size(px(6.)).rounded_full();
    match state {
        "running" => dot.bg(theme::tokens::GREEN.to_hsla()),
        "error" => dot.bg(cx.theme().danger),
        "queued" => dot.border_1().border_color(muted.opacity(0.7)),
        _ => dot.bg(muted.opacity(0.7)),
    }
    .into_any_element()
}

/// The muted trailing text (a duration, a count) of a row.
fn trailing(text: Option<&str>, cx: &App) -> Option<AnyElement> {
    text.map(|text| {
        div()
            .flex_shrink_0()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(text.to_string()))
            .into_any_element()
    })
}

/// One ROW of the card: 32px, the hairline above it, padded like the header.
fn row_frame(height: f32) -> gpui::Div {
    tool_text(h_flex())
        .w_full()
        .min_w_0()
        .h(px(height))
        .px_3()
        .gap_2()
        .items_center()
}

/// One lane line, indented under its phase: dot, label, the muted model,
/// trailing its duration (or the spinner while it runs).
fn lane_row(
    props: &WorkCardProps<'_>,
    at: (usize, usize),
    lane: &WorkCardLane,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let running = lane.state == "running";
    let line = row_frame(geometry::LANE)
        .pl(px(geometry::LANE_INDENT))
        .child(lane_dot(&lane.state, cx))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(if lane.state == "error" {
                    cx.theme().danger
                } else {
                    cx.theme().foreground
                })
                .child(SharedString::from(lane.label.clone())),
        )
        .when_some(lane.model.clone(), |this, model| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_color(muted)
                    .child(SharedString::from(model)),
            )
        })
        .child(div().flex_1())
        .when(running, |this| this.child(Spinner::new().xsmall()))
        .children(trailing(lane.duration.as_deref(), cx));
    match props.on_lane.clone() {
        Some(on_lane) => line
            .id(ElementId::from(SharedString::from(format!(
                "{}-lane-{}-{}",
                props.id, at.0, at.1
            ))))
            .cursor_pointer()
            .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                on_lane(at, window, cx)
            })
            .into_any_element(),
        None => line.into_any_element(),
    }
}

/// EXP-1294: what ONE header row paints — the Work card's header and THE
/// session card share it.
pub(crate) struct HeaderRow {
    /// The kind / state glyph.
    pub(crate) icon: crate::icons::ExpIcon,
    /// The glyph's tint; `None` = muted.
    pub(crate) icon_tint: Option<gpui::Hsla>,
    pub(crate) name: SharedString,
    /// The muted word after the name (`Workflow`, `Working`).
    pub(crate) kind_word: Option<SharedString>,
    /// A 12px spinner in front of the trailing meta.
    pub(crate) running: bool,
    /// The muted trailing text (`progress · elapsed`, `2m 04s`).
    pub(crate) meta: Option<SharedString>,
    /// A trailing glyph after everything (the session card's chevron).
    pub(crate) trailing: Option<AnyElement>,
}

/// The header row: icon, name (medium, truncates), the muted kind word, then
/// trailing the spinner (while running), the meta and the trailing glyph, at
/// `geometry::HEADER`.
pub(crate) fn header_row(row: HeaderRow, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    tool_text(h_flex())
        .w_full()
        .min_w_0()
        .h(px(geometry::HEADER))
        .px_3()
        .gap_2()
        .items_center()
        .child(Icon::new(row.icon).xsmall().text_color(row.icon_tint.unwrap_or(muted)))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(row.name),
        )
        .when_some(row.kind_word, |this, word| {
            this.child(div().flex_shrink_0().text_color(muted).child(word))
        })
        .child(div().flex_1())
        .when(row.running, |this| this.child(Spinner::new().xsmall()))
        .when_some(row.meta, |this, meta| {
            this.child(div().flex_shrink_0().text_color(muted).child(meta))
        })
        .children(row.trailing)
        .into_any_element()
}

/// The Work card's header: kind icon, name, kind word, then trailing the
/// progress and the elapsed time (a spinner in front while running).
fn header(card: &WorkCard, cx: &App) -> AnyElement {
    let meta: Vec<&str> = [card.progress.as_deref(), card.elapsed.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    header_row(
        HeaderRow {
            icon: registry::concept_by_name(&card.icon).unwrap_or(registry::CODING_SUBAGENT),
            icon_tint: None,
            name: SharedString::from(card.name.clone()),
            kind_word: card.kind_word.clone().map(SharedString::from),
            running: card.running,
            meta: (!meta.is_empty())
                .then(|| SharedString::from(meta.join(steer::work_card::WORK_CARD_SEPARATOR))),
            trailing: None,
        },
        cx,
    )
}

/// The segment bar: equal capsules over the muted track, each filled in the
/// foreground (rose when failed).
fn segment_bar(card: &WorkCard, cx: &App) -> Option<AnyElement> {
    if card.segments.is_empty() {
        return None;
    }
    let track = cx.theme().muted_foreground.opacity(0.25);
    let mut bar = h_flex()
        .w_full()
        .px_3()
        .pb_2()
        .gap(px(geometry::SEGMENT_GAP));
    for segment in &card.segments {
        let fill = if segment.state == WorkState::Failed {
            cx.theme().danger
        } else {
            cx.theme().foreground.opacity(0.8)
        };
        bar = bar.child(
            div()
                .flex_1()
                .h(px(geometry::SEGMENT_HEIGHT))
                .rounded_full()
                .bg(track)
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(relative(segment.fill.clamp(0.0, 1.0) as f32))
                        .rounded_full()
                        .bg(fill),
                ),
        );
    }
    Some(bar.into_any_element())
}

/// THE Work card.
pub(crate) fn work_card(props: WorkCardProps<'_>, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let card = props.card;
    let mut column = v_flex()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(crate::dock::card_fill())
        .child(header(card, cx))
        .children(segment_bar(card, cx));
    for (index, row) in card.rows.iter().enumerate() {
        let frame = row_frame(geometry::ROW).border_t_1().border_color(hairline());
        let lanes = props.lanes.get(index).map(Vec::as_slice).unwrap_or(&[]);
        match row {
            WorkCardRow::Folded { label, lanes: count, duration } => {
                let open = props.folded_open;
                let line = frame
                    .child(Icon::new(registry::UI_CHECK).xsmall().text_color(muted))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(label.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(SharedString::from(count.clone())),
                    )
                    .child(div().flex_1())
                    .children(trailing(duration.as_deref(), cx))
                    .child(
                        Icon::new(if open {
                            registry::UI_CHEVRON_DOWN
                        } else {
                            registry::UI_CHEVRON_RIGHT
                        })
                        .xsmall()
                        .text_color(muted.opacity(0.7)),
                    );
                let line = match props.on_toggle_folded.clone() {
                    Some(toggle) => line
                        .id(ElementId::from(SharedString::from(format!("{}-folded", props.id))))
                        .cursor_pointer()
                        .on_click(move |_: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            toggle(window, cx)
                        })
                        .into_any_element(),
                    None => line.into_any_element(),
                };
                column = column.child(line);
                if open {
                    for (lane_ix, lane) in lanes.iter().enumerate() {
                        column = column.child(lane_row(&props, (index, lane_ix), lane, cx));
                    }
                }
            }
            WorkCardRow::Phase { title, state, detail, duration, lanes: shown } => {
                column = column.child(
                    frame
                        .child(state_glyph(*state, cx))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(if *state == WorkState::Pending {
                                    muted
                                } else {
                                    cx.theme().foreground
                                })
                                .child(SharedString::from(title.clone())),
                        )
                        .when_some(detail.clone(), |this, detail| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(muted)
                                    .child(SharedString::from(detail)),
                            )
                        })
                        .child(div().flex_1())
                        .children(trailing(duration.as_deref(), cx)),
                );
                for (lane_ix, lane) in shown.iter().enumerate() {
                    column = column.child(lane_row(&props, (index, lane_ix), lane, cx));
                }
            }
            WorkCardRow::Task { label, state } => {
                column = column.child(
                    frame.child(state_glyph(*state, cx)).child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(label.clone())),
                    ),
                );
            }
            WorkCardRow::Status { label } => {
                column = column.child(
                    frame.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(muted)
                            .child(SharedString::from(label.clone())),
                    ),
                );
            }
        }
    }
    for footer in props.footer {
        column = column.child(
            div()
                .w_full()
                .px_3()
                .py_1p5()
                .border_t_1()
                .border_color(hairline())
                .child(footer),
        );
    }
    match props.on_card {
        Some(on_card) => column
            .id(ElementId::from(SharedString::from(format!("{}-card", props.id))))
            .cursor_pointer()
            .hover(|this| this.border_color(theme::tokens::glass::STROKE_STRONG.to_hsla()))
            .on_click(move |_: &ClickEvent, window, cx| on_card(window, cx))
            .into_any_element(),
        None => column.into_any_element(),
    }
}

/// EXP-1294: a session card tone (`fixtures/session-card.json` `tones`) →
/// the glyph tint: `foreground`, `amber` (needs you), `rose` (usage limit),
/// anything else muted.
pub(crate) fn session_tone_color(tone: &str, cx: &App) -> gpui::Hsla {
    match tone {
        "foreground" => cx.theme().foreground,
        "amber" => theme::tokens::YELLOW.to_hsla(),
        "rose" => cx.theme().danger,
        _ => cx.theme().muted_foreground,
    }
}

/// EXP-1294 — THE session card (`fixtures/session-card.json`): the Work
/// card's header row in the glass card frame — the state glyph (tinted by
/// its tone), the name, the muted state word, trailing the spinner while
/// working, the elapsed time and a chevron. The WHOLE card opens the session
/// (`on_click`); its element id is the fixture's test id plus the session id.
pub(crate) fn session_card(
    card: &domain::session_tree::SessionCard,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    session_card_framed(card, true, Some(registry::UI_CHEVRON_RIGHT), Some(Box::new(on_click)), cx)
}

/// #EXP-1464 (thread-row-action.json): THE session card as a conversation
/// row — `chevron` = the trailing glyph (right = opens, down = expanded),
/// `None` with no `on_click` = a row that does nothing (no chevron, no
/// hover, no press).
pub(crate) fn session_card_row(
    card: &domain::session_tree::SessionCard,
    chevron: Option<crate::icons::ExpIcon>,
    on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    cx: &App,
) -> AnyElement {
    session_card_framed(card, true, chevron, on_click, cx)
}

/// EXP-1323: THE session card without its frame — a started suggestion
/// inside the suggestions card (web `rounded-none border-0 bg-transparent`).
pub(crate) fn session_card_bare(
    card: &domain::session_tree::SessionCard,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    session_card_framed(card, false, Some(registry::UI_CHEVRON_RIGHT), Some(Box::new(on_click)), cx)
}

fn session_card_framed(
    card: &domain::session_tree::SessionCard,
    framed: bool,
    chevron: Option<crate::icons::ExpIcon>,
    on_click: Option<Box<dyn Fn(&mut Window, &mut App) + 'static>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let interactive = on_click.is_some();
    let header = header_row(
        HeaderRow {
            icon: registry::concept_by_name(&card.icon).unwrap_or(registry::CODING_SUBAGENT),
            icon_tint: Some(session_tone_color(&card.tone, cx)),
            name: SharedString::from(card.name.clone()),
            kind_word: Some(SharedString::from(card.kind_word.clone())),
            running: card.running,
            meta: card.elapsed.clone().map(SharedString::from),
            trailing: chevron.map(|chevron| Icon::new(chevron).xsmall().text_color(muted.opacity(0.7)).into_any_element()),
        },
        cx,
    );
    v_flex()
        .id(ElementId::from(SharedString::from(format!(
            "{}-{}",
            card.test_id, card.session_id
        ))))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .when(framed, |this| {
            this.rounded(px(theme::tokens::radius::LG))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(crate::dock::card_fill())
                .when(interactive, |this| {
                    this.hover(|this| this.border_color(theme::tokens::glass::STROKE_STRONG.to_hsla()))
                })
        })
        .when(!framed && interactive, |this| this.hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla())))
        .when_some(on_click, |this, on_click| {
            this.cursor_pointer().on_click(move |_: &ClickEvent, window, cx| on_click(window, cx))
        })
        .child(header)
        .into_any_element()
}
