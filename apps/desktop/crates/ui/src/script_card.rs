//! EXP-1279 (EXP-1259 D13) — the code-mode Script card's paint.
//!
//! A tool call whose name ends in `codemode__exec` (contract toolKind
//! `script`) renders as ONE card: "Ran a script · 59 calls · 3.0 s", one row
//! per tool the script called (grouped, first-call order) and the script
//! itself behind a "Script" disclosure. What it says is the pure model
//! `steer::script_card` (byte-locked by `fixtures/script-card.json`); this
//! module paints it. A script call never joins a generic tool fold.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, ElementId,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, spinner::Spinner, v_flex, ActiveTheme as _, Icon, Sizable as _};
use steer::script_card::{ScriptCard, SCRIPT_CARD_SEPARATOR};
use theme::tokens::transcript;

use crate::controls::WebText as _;
use crate::icons::registry;

/// What the disclosure toggle calls.
pub(crate) type SourceToggle = Rc<dyn Fn(&mut Window, &mut App)>;

pub(crate) struct ScriptCardProps<'a> {
    pub id: SharedString,
    pub card: &'a ScriptCard,
    /// While the call runs the card is its title alone, with the spinner.
    pub running: bool,
    pub failed: bool,
    /// The exec input's `script`, when this client has it.
    pub source: Option<String>,
    pub source_open: bool,
    pub on_toggle_source: Option<SourceToggle>,
}

fn tool_text<E: Styled>(element: E) -> E {
    element
        .text_size(px(transcript::TOOL_SIZE))
        .line_height(px(transcript::TOOL_LINE_HEIGHT))
}

/// THE Script card.
pub(crate) fn script_card(props: ScriptCardProps<'_>, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rose = cx.theme().danger;
    let card = props.card;
    let has_source = props.source.as_deref().is_some_and(|source| !source.trim().is_empty());
    let mut header = tool_text(h_flex())
        .w_full()
        .min_w_0()
        .h(px(36.))
        .px_3()
        .gap_2()
        .items_center()
        .child(
            Icon::new(registry::TOOL_SCRIPT)
                .xsmall()
                .text_color(if props.failed { rose } else { muted }),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(if props.failed { rose } else { cx.theme().foreground })
                .child(SharedString::from(card.title.clone())),
        )
        .when_some(card.meta.clone(), |this, meta| {
            this.child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(muted)
                    .child(SharedString::from(format!("{}{meta}", SCRIPT_CARD_SEPARATOR.trim_start()))),
            )
        })
        .child(div().flex_1())
        .when(props.running, |this| this.child(Spinner::new().xsmall()));
    if has_source && !props.running {
        let open = props.source_open;
        let disclosure = h_flex()
            .id(ElementId::from(SharedString::from(format!("{}-script", props.id))))
            .flex_shrink_0()
            .gap_1()
            .items_center()
            .cursor_pointer()
            .text_color(muted)
            .hover(|this| this.text_color(cx.theme().foreground))
            .child(SharedString::from(steer::script_card::copy::SCRIPT))
            .child(
                Icon::new(if open {
                    registry::UI_CHEVRON_DOWN
                } else {
                    registry::UI_CHEVRON_RIGHT
                })
                .xsmall(),
            );
        header = header.child(match props.on_toggle_source.clone() {
            Some(toggle) => disclosure
                .on_click(move |_: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    toggle(window, cx)
                })
                .into_any_element(),
            None => disclosure.into_any_element(),
        });
    }
    let mut column = v_flex()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(theme::tokens::radius::LG))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .child(header);
    if props.source_open && has_source && !props.running {
        column = column.child(
            div()
                .id(ElementId::from(SharedString::from(format!("{}-source", props.id))))
                .w_full()
                .max_h(px(240.))
                .overflow_y_scroll()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .text_size(px(11.))
                .line_height(px(16.))
                .font_family(theme::terminal::FONT_FAMILY)
                .text_color(muted)
                .child(SharedString::from(props.source.clone().unwrap_or_default())),
        );
    }
    if !card.groups.is_empty() || card.overflow.is_some() {
        let mut rows = v_flex().w_full().min_w_0().pb_1p5();
        for group in &card.groups {
            let mark = if group.exponential {
                // The app's own mark, the one our MCP rows already wear.
                Icon::from(crate::icons::ExpIcon::Logo).xsmall().text_color(muted)
            } else {
                Icon::new(registry::CODING_TOOL).xsmall().text_color(muted.opacity(0.6))
            };
            let subject: Option<String> = group.subject.as_ref().map(|subject| match &group.more {
                Some(more) => format!("{subject} {more}"),
                None => subject.clone(),
            });
            rows = rows.child(
                tool_text(h_flex())
                    .w_full()
                    .min_w_0()
                    .h(px(28.))
                    .px_3()
                    .gap_2()
                    .items_center()
                    .child(mark)
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(cx.theme().foreground)
                            .child(SharedString::from(group.label.clone())),
                    )
                    .when_some(subject, |this, subject| {
                        this.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_2xs()
                                .text_color(muted)
                                .child(SharedString::from(subject)),
                        )
                    })
                    .child(div().flex_1())
                    .when_some(group.failed.clone(), |this, failed| {
                        this.child(div().flex_shrink_0().text_color(rose).child(SharedString::from(failed)))
                    })
                    .when(group.failed.is_none(), |this| {
                        this.when_some(group.count.clone(), |this, count| {
                            this.child(
                                div().flex_shrink_0().text_color(muted).child(SharedString::from(count)),
                            )
                        })
                    }),
            );
        }
        if let Some(overflow) = card.overflow.clone() {
            rows = rows.child(
                tool_text(h_flex())
                    .w_full()
                    .h(px(28.))
                    .px_3()
                    .items_center()
                    .text_color(muted)
                    .child(SharedString::from(overflow)),
            );
        }
        column = column.child(rows);
    }
    column.into_any_element()
}
