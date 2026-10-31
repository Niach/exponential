//! #EXP-1493 THE Outline (styleguide `outline`, eUI `Outline`; web
//! `packages/ui/src/outline.tsx`): an anchor list. The caption (2xs
//! uppercase muted), one row per entry (`plan-page.json`
//! `layout.outlineRowHeight`, the title on one line truncated, a muted
//! `· n` when it counts something), ONE active row (the accent fill + a
//! `layout.outlineActiveBar` px foreground bar on its leading edge), a
//! press selects; then a separator and the footer rows (the plan page's
//! Questions row). Scroll-spy belongs to the host.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::controls::WebText as _;

use domain::plan_page::PLAN_PAGE;

/// One row of THE Outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlineEntry {
    pub id: String,
    pub title: String,
    /// A muted `· n` after the title (0 = none).
    pub count: usize,
}

/// A row press: the entry's id.
pub(crate) type OnOutlineSelect = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// THE Outline. `row_id` prefixes each row's element id (`<row_id>-<id>`).
pub(crate) fn outline(
    row_id: &str,
    caption: Option<&str>,
    entries: &[OutlineEntry],
    active: Option<&str>,
    on_select: Option<OnOutlineSelect>,
    footer: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let layout = &PLAN_PAGE.layout;
    let (muted, foreground, accent, border) = {
        let theme = cx.theme();
        (theme.muted_foreground, theme.foreground, theme.accent, theme.border)
    };
    let mut column = v_flex().w_full().min_w_0().gap_0p5();
    if let Some(caption) = caption {
        column = column.child(
            div()
                .px_2()
                .pb_2()
                .text_2xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(muted)
                .child(SharedString::from(caption.to_uppercase())),
        );
    }
    for entry in entries {
        let on = active == Some(entry.id.as_str());
        let select = on_select.clone();
        let id = entry.id.clone();
        column = column.child(
            h_flex()
                .id(SharedString::from(format!("{row_id}-{}", entry.id)))
                .relative()
                .w_full()
                .min_w_0()
                .min_h(px(layout.outline_row_height))
                .items_center()
                .gap_1()
                .px_2()
                .rounded(px(theme::tokens::radius::MD))
                .overflow_hidden()
                .text_xs()
                .cursor_pointer()
                .map(|row| {
                    if on {
                        row.bg(accent).text_color(foreground).child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(layout.outline_active_bar))
                                .bg(foreground),
                        )
                    } else {
                        row.text_color(muted)
                            .hover(move |style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()).text_color(foreground))
                    }
                })
                .child(div().min_w_0().truncate().child(SharedString::from(entry.title.clone())))
                .when(entry.count > 0, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(SharedString::from(format!("· {}", entry.count))),
                    )
                })
                .when_some(select, move |row, select| {
                    row.on_click(move |_, window, cx| select(&id, window, cx))
                }),
        );
    }
    if let Some(footer) = footer {
        column = column.child(div().my_2().w_full().h(px(1.)).bg(border)).child(footer);
    }
    column.into_any_element()
}

/// A footer row of THE Outline (the plan page's Questions row): the label
/// and its muted `· n`, inert.
pub(crate) fn outline_footer_row(label: &str, count: usize, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .min_h(px(PLAN_PAGE.layout.outline_row_height))
        .items_center()
        .gap_1()
        .px_2()
        .text_xs()
        .text_color(muted)
        .child(SharedString::from(label.to_string()))
        .child(SharedString::from(format!("· {count}")))
        .into_any_element()
}
