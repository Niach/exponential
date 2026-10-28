//! #EXP-1380 — THE Chats list pieces (`fixtures/chats.json` LIST + BUBBLES
//! + WIDE), the gpui twin of `packages/ui/src/chat-list.tsx`: the agent
//! bubbles (66, a 3px ring around a background gap: amber = needs you,
//! foreground = unread, a glass hairline = read), the filter chips (select
//! Pills, `chat_filter_label`), the flat rows (the 52 lead disc, title +
//! time, the two-line muted preview, the row word and the unread badge, a
//! hairline under the text column only). The IDE's Chats page
//! ([`crate::threads_view`]) and the styleguide's `chat-list` entry paint
//! these same functions; the model is `domain::chats`.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement as _,
    ParentElement as _, Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::chats::{chat_filter_label, chat_selection_count, ChatSelectionAction, AgentBubble, AgentRing, ChatCounts, ChatFilter, ChatLead, ChatPreviewKind, ChatRow, CHATS};
use domain::session_tree::ThreadRowWord;

use crate::icons::registry;
use crate::issue_list::{select_cell, selectable_row_fill, ROW_GROUP, ROW_PAD, SELECT_CELL_GAP, SELECT_CELL_W};
use crate::surface::{glass_pill, PillMode, PillSize};

/// The glyph inside the lead disc / a bubble (web `size-6`).
const GLYPH_PX: f32 = 24.;

/// A filter pick.
pub(crate) type OnFilter = Rc<dyn Fn(ChatFilter, &mut Window, &mut App)>;

/// The glyph before a row word ×4 (Android `ChatComponents`, iOS
/// `wordIcon`): a stack = `pr-stack`, ready to merge = `pr-open`.
fn word_glyph(word: ThreadRowWord) -> Option<crate::icons::ExpIcon> {
    match word {
        ThreadRowWord::Stack => Some(registry::PR_STACK),
        ThreadRowWord::ReadyToMerge => Some(registry::PR_OPEN),
        ThreadRowWord::Merged | ThreadRowWord::Question => None,
    }
}

/// The 52 lead DISC (bg muted) and what it holds: the brand mark with the
/// amber badge, the working mark, the root's PR glyph, else the bare brand
/// mark. `agent` = the root's CLI. #EXP-1388: a `selected` row's disc steps
/// one grey notch up (the glass `fillActive` over muted, web
/// `ChatLeadDisc selected`) so it never blends into the selected fill.
pub(crate) fn chat_lead_disc(lead: ChatLead, agent: Option<coding::CodingAgent>, selected: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let ground = if selected {
        theme.muted.blend(theme::tokens::glass::FILL_ACTIVE.to_hsla())
    } else {
        theme.muted
    };
    let brand = || {
        crate::coding_selects::agent_mark(agent.unwrap_or_default())
            .with_size(px(GLYPH_PX))
            .into_any_element()
    };
    let body = match lead {
        ChatLead::Working => crate::coding_selects::agent_working_mark(agent.or(Some(Default::default())), GLYPH_PX)
            .into_any_element(),
        ChatLead::Open => Icon::new(registry::PR_OPEN)
            .with_size(px(GLYPH_PX))
            .text_color(theme::tokens::GREEN.to_hsla())
            .into_any_element(),
        ChatLead::Merged => Icon::new(registry::PR_MERGED)
            .with_size(px(GLYPH_PX))
            .text_color(crate::pr_rows::PR_MERGED_TINT.to_hsla())
            .into_any_element(),
        ChatLead::NeedsYou | ChatLead::Waiting => brand(),
    };
    div()
        .relative()
        .size(px(CHATS.shell.lead_disc))
        .flex_shrink_0()
        .rounded_full()
        .bg(ground)
        .flex()
        .items_center()
        .justify_center()
        .child(body)
        .when(lead == ChatLead::NeedsYou, |disc| {
            disc.child(
                div()
                    .absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .size(px(14.))
                    .rounded_full()
                    .border_2()
                    .border_color(ground)
                    .bg(theme::tokens::YELLOW.to_hsla()),
            )
        })
        .into_any_element()
}

/// The preview's copy prefix (`Question: `, `Plan: `, `You: `), painted at
/// full emphasis before the muted rest (web `previewParts`).
fn preview_parts(row: &ChatRow) -> (Option<String>, String) {
    let copy = &CHATS.copy.preview;
    let template = match row.preview.kind {
        ChatPreviewKind::Question => Some(&copy.question),
        ChatPreviewKind::Plan => Some(&copy.plan),
        ChatPreviewKind::You => Some(&copy.you),
        _ => None,
    };
    if let Some(prefix) = template.and_then(|t| t.split("{text}").next()).filter(|p| !p.is_empty()) {
        if let Some(rest) = row.preview.text.strip_prefix(prefix) {
            return (Some(prefix.to_string()), rest.to_string());
        }
    }
    (None, row.preview.text.clone())
}

/// One chat (a ROOT session) as the flat list row: `lead` = its
/// [`chat_lead_disc`]. The caller makes it clickable and hangs the menu.
pub(crate) fn chat_row(row: &ChatRow, lead: AnyElement, cx: &App) -> Div {
    let theme = cx.theme();
    let unread = row.unread_count > 0;
    let time_color = if row.needs_you {
        theme::tokens::YELLOW.to_hsla()
    } else if unread {
        theme.foreground
    } else {
        theme.muted_foreground
    };
    let word = row
        .word
        .and_then(|word| word.text().map(|text| (word_glyph(word), text)));
    let badge = unread.then(|| {
        let (fill, text) = if row.needs_you {
            (theme::tokens::YELLOW.to_hsla(), gpui::black())
        } else {
            (theme.foreground, theme.background)
        };
        let count = if row.unread_count > 99 { "99+".to_string() } else { row.unread_count.to_string() };
        div()
            .flex_shrink_0()
            .min_w(px(20.))
            .h(px(20.))
            .px_1p5()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(fill)
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(text)
            .child(SharedString::from(count))
    });
    let (prefix, rest) = preview_parts(row);
    let preview = div()
        .flex_1()
        .min_w_0()
        .line_clamp(2)
        .text_sm()
        .text_color(theme.muted_foreground)
        .when(row.preview.kind == ChatPreviewKind::Working, |this| this.italic())
        .child(match prefix {
            Some(prefix) => gpui::StyledText::new(format!("{prefix}{rest}"))
                .with_highlights([(
                    0..prefix.len(),
                    gpui::HighlightStyle { color: Some(theme.foreground.opacity(0.85)), ..Default::default() },
                )])
                .into_any_element(),
            None => SharedString::from(rest).into_any_element(),
        });
    let second = h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(preview)
        .children(word.map(|(glyph, text)| {
            h_flex()
                .flex_shrink_0()
                .gap_1()
                .items_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .children(glyph.map(|glyph| Icon::new(glyph).with_size(px(14.)).text_color(theme.muted_foreground)))
                .child(text)
        }))
        .children(badge);
    // S-3: a DONE chat stays in the list, the whole row dimmed.
    chat_row_frame(lead, row.title.clone(), unread, row.time.clone(), time_color, Some(second), cx)
        .when(row.dim, |this| this.opacity(CHATS.shell.dim_opacity))
}

/// THE row's frame: the lead, then the text column (title + the trailing
/// time slot on line 1, `second` under it) over the row hairline.
fn chat_row_frame(
    lead: AnyElement,
    title: String,
    strong: bool,
    time: String,
    time_color: gpui::Hsla,
    second: Option<Div>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .gap_4()
        .items_center()
        .child(lead)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .py(px(14.))
                .gap_1()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .items_baseline()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(15.))
                                .font_weight(if strong { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                                .text_color(theme.foreground)
                                .child(SharedString::from(title)),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(time_color)
                                .child(SharedString::from(time)),
                        ),
                )
                .children(second),
        )
}

/// #EXP-1388 THE ARCHIVED ENTRY: the chat row whose lead cell holds the
/// PLAIN `ui-archive` glyph (`shell.archivedGlyph`, muted foreground, no
/// disc behind it) centred in the `leadDisc`-wide column so it lines up
/// with the chats' discs; `copy.archived.entry` as its title and the count
/// in the time slot (muted), no preview line, so the row is as tall as its
/// text line plus the chat row's vertical padding.
pub(crate) fn chat_archived_row(count: usize, cx: &App) -> Div {
    let theme = cx.theme();
    let lead = div()
        .w(px(CHATS.shell.lead_disc))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(registry::UI_ARCHIVE)
                .with_size(px(CHATS.shell.archived_glyph))
                .text_color(theme.muted_foreground),
        )
        .into_any_element();
    chat_row_frame(
        lead,
        CHATS.copy.archived.entry.clone(),
        false,
        count.to_string(),
        theme.muted_foreground,
        None,
        cx,
    )
}

/// #EXP-1388 (owner on #1283): THE selectable chat row = the issue row's
/// frame, its pieces shared with [`crate::issue_list`]: the [`ROW_PAD`]
/// gutter, THE select cell ([`select_cell`]: the checkbox, revealed on row
/// hover, pinned while `selecting`; its click = `on_toggle`), the
/// [`SELECT_CELL_GAP`], then `row`; the flat full-width selected / hover
/// fill ([`selectable_row_fill`]). `select` = `(selected, selecting)`;
/// `None` = a row that is never selected (the Archived entry): the cell's
/// room only, so its disc lines up. `key` = the row's session id. The
/// caller hangs the row's click and menu.
pub(crate) fn chat_selectable(
    id: impl Into<ElementId>,
    key: &str,
    select: Option<(bool, bool)>,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    row: Div,
    cx: &App,
) -> Stateful<Div> {
    let cell = match select {
        Some((selected, selecting)) => select_cell(key, selected, selecting, on_toggle, cx).into_any_element(),
        None => div().w(px(SELECT_CELL_W)).flex_shrink_0().into_any_element(),
    };
    let frame = div()
        .id(id)
        .group(ROW_GROUP)
        .w_full()
        .min_w_0()
        .px(px(ROW_PAD))
        .flex()
        .items_center()
        .gap(px(SELECT_CELL_GAP))
        .cursor_pointer()
        .child(cell)
        .child(row.flex_1().min_w_0());
    selectable_row_fill(frame, select.is_some_and(|(selected, _)| selected), cx)
}

/// A selection bar action pick.
pub(crate) type OnSelectionAction = Rc<dyn Fn(ChatSelectionAction, Vec<String>, &mut Window, &mut App)>;

/// The panel width the LABELED chat bar needs on one line — the issue
/// bar's rule (`issue_list::bulk_bar_label_min_width`, EXP-698) summed over
/// the chat bar's controls: the same chrome (32 the hosting row's px_4
/// gutters + 20 bar padding + 32 the ✕ + 16 the count), per action a ghost
/// `web_sm` icon + label (44 + ~6.5 per character, the issue bar's measured
/// Status / Delete) and its 8px gap, and the 9px hairline before Delete.
pub(crate) fn chat_bar_label_min_width(actions: &[ChatSelectionAction]) -> Pixels {
    let buttons: f32 = actions
        .iter()
        .map(|action| 44. + 6.5 * action.label().chars().count() as f32 + 8.)
        .sum();
    let rule = if actions.iter().any(|action| action.destructive()) { 9. } else { 0. };
    px(32. + 20. + 32. + 16. + buttons + rule)
}

/// #EXP-1388 THE BAR = the issue list's bulk bar: its capsule
/// ([`crate::issue_list::bulk_bar_capsule`]: ✕ + the bare count,
/// `copy.selection.count`), `chat_selection_actions` as its buttons
/// ([`crate::issue_list::bulk_bar_button`], labels only while `labels`:
/// [`chat_bar_label_min_width`]), the hairline before the destructive
/// Delete. The caller docks it ([`crate::issue_list::bulk_bar_dock`]).
pub(crate) fn chat_selection_bar(
    count: usize,
    actions: Vec<(ChatSelectionAction, Vec<String>)>,
    busy: bool,
    labels: bool,
    on_clear: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    on_action: Option<OnSelectionAction>,
    cx: &App,
) -> AnyElement {
    use gpui_component::Disableable as _;
    let mut bar = crate::issue_list::bulk_bar_capsule(chat_selection_count(count).into(), on_clear, cx);
    for (action, ids) in actions {
        if action.destructive() {
            bar = bar.child(crate::issue_list::bulk_bar_rule());
        }
        let mut button = crate::issue_list::bulk_bar_button(
            SharedString::from(format!("chat-bulk-{}", action.key())),
            registry::concept_by_name(action.icon()).unwrap_or(registry::UI_CHECK),
            action.label(),
            labels,
            action.destructive(),
            cx,
        )
        .tooltip(action.label())
        .disabled(busy);
        if let Some(on_action) = on_action.clone() {
            button = button.on_click(move |_, window, cx| on_action(action, ids.clone(), window, cx));
        }
        bar = bar.child(button);
    }
    bar.into_any_element()
}

/// One agent bubble: the 66 circle (a 3px ring around a 3px background gap
/// around the agent's icon on bg muted; the working mark while busy) with
/// the name under it. The caller hangs the click and the agent menu.
pub(crate) fn agent_bubble(bubble: &AgentBubble, cx: &App) -> Div {
    let theme = cx.theme();
    let shell = &CHATS.shell;
    let ring = match bubble.ring {
        Some(AgentRing::NeedsYou) => theme::tokens::YELLOW.to_hsla(),
        Some(AgentRing::Unread) => theme.foreground,
        None => gpui::transparent_black(),
    };
    let agent = &bubble.agent;
    let glyph = if agent.busy {
        crate::coding_selects::agent_working_mark(None, GLYPH_PX).into_any_element()
    } else {
        crate::icons::agent_icon(Some(&agent.icon), Some(&agent.color))
            .with_size(px(GLYPH_PX))
            .into_any_element()
    };
    v_flex()
        .w(px(shell.bubble + 6.))
        .flex_shrink_0()
        .items_center()
        .gap_1p5()
        .child(
            div()
                .relative()
                .size(px(shell.bubble))
                .flex_shrink_0()
                .rounded_full()
                .border(px(shell.bubble_ring))
                .border_color(ring)
                .p(px(shell.bubble_ring))
                .when(bubble.ring.is_none(), |this| {
                    this.child(
                        div()
                            .absolute()
                            .top(px(-shell.bubble_ring))
                            .left(px(-shell.bubble_ring))
                            .right(px(-shell.bubble_ring))
                            .bottom(px(-shell.bubble_ring))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla()),
                    )
                })
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(theme.muted)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(glyph),
                ),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .text_center()
                .text_xs()
                .when(bubble.ring.is_some(), |this| this.font_weight(FontWeight::MEDIUM))
                .text_color(if bubble.ring.is_some() { theme.foreground } else { theme.muted_foreground })
                .child(SharedString::from(agent.name.clone())),
        )
}

/// The Chats header's agents row: the bubbles in `list_agents` order,
/// scrolling sideways. `wrap` hangs each bubble's click + menu (its index
/// and agent id) — the styleguide passes none.
pub(crate) fn agent_bubbles_row(
    bubbles: &[AgentBubble],
    wrap: impl Fn(usize, &AgentBubble, Div) -> AnyElement,
    cx: &App,
) -> Option<AnyElement> {
    if bubbles.is_empty() {
        return None;
    }
    Some(
        h_flex()
            .id("chat-agent-bubbles")
            .w_full()
            .gap_3()
            .pb_1()
            .overflow_x_scroll()
            .children(bubbles.iter().enumerate().map(|(index, bubble)| wrap(index, bubble, agent_bubble(bubble, cx))))
            .into_any_element(),
    )
}

/// The filter chips over the list: select Pills labelled by
/// `chat_filter_label` (a count after a non-zero one).
pub(crate) fn chat_filter_chips(counts: &ChatCounts, selected: ChatFilter, on_select: Option<OnFilter>, cx: &App) -> AnyElement {
    h_flex()
        .gap_2()
        .children(ChatFilter::ALL.into_iter().enumerate().map(|(index, filter)| {
            let pill = glass_pill(
                (SharedString::from(CHATS.test_ids.chat_filter.clone()), index),
                PillSize::Md,
                PillMode::Select { selected: filter == selected },
                cx,
            )
            .h(px(36.))
            .px_4()
            .child(SharedString::from(chat_filter_label(filter, counts)));
            match on_select.clone() {
                Some(on_select) => pill.on_click(move |_, window, cx| on_select(filter, window, cx)),
                None => pill,
            }
        }))
        .into_any_element()
}
