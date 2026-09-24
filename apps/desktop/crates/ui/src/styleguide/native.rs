//! EXP-1039 — the IDE's OWN styleguide entries: the controls this app
//! already ships, each one filed under one of the four shared sections
//! (`apps/styleguide/src/sections/sections.json`) so the IDE's styleguide
//! reads as the same ordered page the web one does.
//!
//! This list is SEPARATE from [`super::entries::ENTRIES`] on purpose: that
//! one mirrors the shared section index one id for one id (a test gates the
//! drift) and holds the per-control leaf entries every platform fills. This
//! one is the desktop's starting set — a live demo of the real `ui` crate
//! recipes, built from the same helpers the app builds its screens from, so
//! a change to a recipe changes its entry with no second implementation to
//! keep in step.

use gpui::{div, px, AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};

use crate::composer::{self, GlassComposer};
use crate::icons::registry;
use crate::surface::{
    self, glass_card, glass_group_rows, glass_pill, glass_row_card, glass_section_band, BadgeTone,
    PillMode, PillSize,
};

/// One native entry: what it is called, what it is for, which section it
/// belongs to and the live demo.
pub(crate) struct NativeEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    /// The shared index's section `order` (1..=4).
    pub section: u8,
    pub demo: fn(&mut Window, &mut App) -> AnyElement,
}

/// Every native entry, in page order within its section.
pub(crate) const NATIVE_ENTRIES: &[NativeEntry] = &[
    NativeEntry {
        id: "native-theme-tokens",
        title: "Theme tokens",
        blurb: "The OKLCH zinc palette the whole IDE paints from (`theme::tokens`, generated ×4).",
        section: 1,
        demo: theme_tokens,
    },
    NativeEntry {
        id: "native-type-scale",
        title: "Type scale",
        blurb: "The four sizes a screen uses: a headline, body, the list's 14px and the 12px note.",
        section: 1,
        demo: type_scale,
    },
    NativeEntry {
        id: "native-icons",
        title: "Icon registry",
        blurb: "Icons are named by CONCEPT (`registry::NAV_*`), never by glyph — one set ×4.",
        section: 1,
        demo: icons,
    },
    NativeEntry {
        id: "native-buttons",
        title: "Buttons",
        blurb: "gpui-component `Button` in the variants the app uses: primary, outline, ghost, danger.",
        section: 2,
        demo: buttons,
    },
    NativeEntry {
        id: "native-pills",
        title: "Pills",
        blurb: "`surface::glass_pill` — THE capsule: every chip, tag, filter and picker trigger.",
        section: 2,
        demo: pills,
    },
    NativeEntry {
        id: "native-badges",
        title: "Badges and dots",
        blurb: "`count_badge` for a quantity, `pill_dot` for a colour, `live_dot` for a live run.",
        section: 2,
        demo: badges,
    },
    NativeEntry {
        id: "native-surfaces",
        title: "Glass surfaces",
        blurb: "`glass_card` (a card), `glass_row_card` (a list object), `glass_group_rows` (form rows).",
        section: 2,
        demo: surfaces,
    },
    NativeEntry {
        id: "native-list-rows",
        title: "List rows",
        blurb: "The ×4 list: a filled group band over FLAT rows — never a stack of cards.",
        section: 2,
        demo: list_rows,
    },
    NativeEntry {
        id: "native-issue-chip",
        title: "Issue chip",
        blurb: "The ONE issue badge (`issue_chip`): identifier, status glyph, optional remove.",
        section: 3,
        demo: issue_chips,
    },
    NativeEntry {
        id: "native-composer-card",
        title: "Composer card",
        blurb: "`composer::glass_composer` — the field, its tools and the round send, one card.",
        section: 3,
        demo: composer_card,
    },
    NativeEntry {
        id: "native-launcher-headline",
        title: "Launcher headline",
        blurb: "EXP-1019: the composer's main element — the contract verb plus the subject's chips.",
        section: 3,
        demo: launcher_headline,
    },
    NativeEntry {
        id: "native-banded-list",
        title: "Banded list view",
        blurb: "A whole list screen's shape: the section band, its rows, and the count on the band.",
        section: 4,
        demo: banded_list,
    },
];

/// The native entries filed under the section with this `order`.
pub(crate) fn in_section(order: u8) -> impl Iterator<Item = &'static NativeEntry> {
    NATIVE_ENTRIES
        .iter()
        .filter(move |entry| entry.section == order)
}

// ── 1 Style ───────────────────────────────────────────────────────────────

fn theme_tokens(_window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let swatches = [
        ("background", theme.background),
        ("foreground", theme.foreground),
        ("muted", theme.muted),
        ("muted_foreground", theme.muted_foreground),
        ("border", theme.border),
        ("primary", theme.primary),
        ("danger", theme.danger),
        ("warning", theme.warning),
        ("success", theme.success),
        ("info", theme.info),
    ];
    let border = theme.border;
    let muted = theme.muted_foreground;
    h_flex()
        .flex_wrap()
        .gap_3()
        .children(swatches.map(|(name, color)| {
            v_flex()
                .gap_1()
                .child(
                    div()
                        .w(px(64.))
                        .h(px(32.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(border)
                        .bg(color),
                )
                .child(div().text_xs().text_color(muted).child(name))
        }))
        .into_any_element()
}

fn type_scale(_window: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .gap_1()
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child("text_lg semibold — a headline"),
        )
        .child(div().text_sm().child("text_sm — body and every list row"))
        .child(div().text_xs().text_color(muted).child("text_xs muted — a note under a control"))
        .into_any_element()
}

fn icons(_window: &mut Window, cx: &mut App) -> AnyElement {
    let concepts = [
        ("nav-search", registry::NAV_SEARCH),
        ("nav-support", registry::NAV_SUPPORT),
        ("nav-drafts", registry::NAV_DRAFTS),
        ("ui-add", registry::UI_ADD),
        ("ui-close", registry::UI_CLOSE),
        ("ui-more", registry::UI_MORE),
        ("ui-submit", registry::UI_SUBMIT),
        ("editor-issue-ref", registry::EDITOR_ISSUE_REF),
    ];
    let muted = cx.theme().muted_foreground;
    h_flex()
        .flex_wrap()
        .gap_4()
        .children(concepts.map(|(name, icon)| {
            v_flex()
                .items_center()
                .gap_1()
                .child(Icon::from(icon).text_color(muted))
                .child(div().text_xs().text_color(muted).child(name))
        }))
        .into_any_element()
}

// ── 2 General components ──────────────────────────────────────────────────

fn buttons(_window: &mut Window, cx: &mut App) -> AnyElement {
    let _ = cx;
    h_flex()
        .flex_wrap()
        .gap_2()
        .items_center()
        .child(Button::new("sg-button-primary").primary().small().label("Primary"))
        .child(Button::new("sg-button-outline").outline().small().label("Outline"))
        .child(Button::new("sg-button-ghost").ghost().small().label("Ghost"))
        .child(Button::new("sg-button-danger").danger().small().label("Danger"))
        .child(
            Button::new("sg-button-disabled")
                .outline()
                .small()
                .label("Disabled")
                .disabled(true),
        )
        .into_any_element()
}

fn pills(_window: &mut Window, cx: &mut App) -> AnyElement {
    h_flex()
        .flex_wrap()
        .gap_2()
        .items_center()
        .child(
            glass_pill("sg-pill-action", PillSize::Md, PillMode::Action, cx)
                .child(Icon::from(registry::UI_ADD).xsmall())
                .child("Action · Md"),
        )
        .child(
            glass_pill(
                "sg-pill-selected",
                PillSize::Sm,
                PillMode::Select { selected: true },
                cx,
            )
            .child("Selected · Sm"),
        )
        .child(
            glass_pill(
                "sg-pill-unselected",
                PillSize::Sm,
                PillMode::Select { selected: false },
                cx,
            )
            .child("Option · Sm"),
        )
        .child(
            glass_pill("sg-pill-readonly", PillSize::Sm, PillMode::Readonly, cx)
                .child("Readonly · Sm"),
        )
        .into_any_element()
}

fn badges(_window: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let primary = cx.theme().primary;
    h_flex()
        .flex_wrap()
        .gap_3()
        .items_center()
        .children(surface::count_badge(3, BadgeTone::Muted, cx))
        .children(surface::count_badge(128, BadgeTone::Primary, cx))
        .child(surface::pill_dot(primary))
        .child(surface::live_dot(primary, true))
        .child(div().text_xs().text_color(muted).child("count · count · dot · live"))
        .into_any_element()
}

fn surfaces(_window: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .flex_wrap()
        .items_start()
        .gap_3()
        .child(
            glass_card()
                .w(px(180.))
                .p_3()
                .gap_1()
                .child(div().text_sm().child("glass_card"))
                .child(div().text_xs().text_color(muted).child("a real card")),
        )
        .child(
            glass_row_card()
                .w(px(180.))
                .p_3()
                .child(div().text_sm().child("glass_row_card")),
        )
        .child(
            glass_group_rows(vec![
                crate::surface::glass_row_shell()
                    .child(div().text_sm().child("Field"))
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(muted).child("Value")),
                crate::surface::glass_row_shell()
                    .child(div().text_sm().child("Another"))
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(muted).child("Value")),
            ])
            .w(px(240.)),
        )
        .into_any_element()
}

fn list_rows(_window: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .w_full()
        .gap_1()
        .child(glass_section_band(None, "In progress", None, cx))
        .children((0..3).map(|ix| {
            surface::flat_row()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .child(div().text_sm().child(format!("A row that names something · {ix}")))
                .child(div().flex_1())
                .child(div().text_xs().text_color(muted).child("trailing"))
        }))
        .into_any_element()
}

// ── 3 Special components ──────────────────────────────────────────────────

fn issue_chips(_window: &mut Window, cx: &mut App) -> AnyElement {
    let _ = cx;
    h_flex()
        .flex_wrap()
        .gap_2()
        .items_center()
        .child(crate::issue_chip::issue_chip(
            "sg-issue-chip",
            "EXP-1037",
            "Open the composer in a dialog",
        ))
        .child(crate::issue_chip::issue_chip("sg-issue-chip-bare", "EXP-1019", ""))
        .into_any_element()
}

fn composer_card(_window: &mut Window, cx: &mut App) -> AnyElement {
    let field = div()
        .w_full()
        .min_w_0()
        .py_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(
            domain::contract::COMPOSER_UI_INSTRUCTIONS_PLACEHOLDER,
        ))
        .into_any_element();
    let composer = GlassComposer::new(field)
        .tool(composer::composer_tool("sg-composer-issues", registry::EDITOR_ISSUE_REF, cx))
        .tool(composer::composer_tool("sg-composer-attach", registry::UI_ADD, cx))
        .submit(composer::composer_submit(
            "sg-composer-send",
            registry::UI_SUBMIT,
            false,
            cx,
        ));
    div()
        .w(px(440.))
        .child(composer::glass_composer(composer))
        .into_any_element()
}

fn launcher_headline(_window: &mut Window, cx: &mut App) -> AnyElement {
    let foreground = cx.theme().foreground;
    v_flex()
        .gap_2()
        .child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(foreground)
                        .child(domain::contract::COMPOSER_UI_IMPLEMENT_HEADLINE),
                )
                .child(crate::issue_chip::issue_chip(
                    "sg-headline-chip",
                    "EXP-1037",
                    "Open the composer in a dialog",
                )),
        )
        .child(
            h_flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(foreground)
                        .child(domain::contract::COMPOSER_UI_RUN_HEADLINE),
                )
                .child(
                    glass_pill("sg-headline-action", PillSize::Sm, PillMode::Readonly, cx)
                        .child(Icon::from(registry::UI_MORE).xsmall())
                        .child("Release all"),
                ),
        )
        .into_any_element()
}

// ── 4 Views ───────────────────────────────────────────────────────────────

fn banded_list(window: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .w(px(440.))
        .gap_3()
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(glass_section_band(
                    Some(Icon::from(registry::NAV_DRAFTS).xsmall().into_any_element()),
                    "Backlog",
                    surface::count_badge(2, BadgeTone::Muted, cx).map(IntoElement::into_any_element),
                    cx,
                ))
                .child(list_rows(window, cx)),
        )
        .child(div().text_xs().text_color(muted).child(
            "Every list ×4 is this: one filled band over flat rows, cards only in settings.",
        ))
        .into_any_element()
}
