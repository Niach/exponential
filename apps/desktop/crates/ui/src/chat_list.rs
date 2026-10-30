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

use domain::chats::{chat_filter_label, chat_selection_count, ChatSelectionAction, AgentBubble, ChatCounts, ChatFilter, ChatLead, ChatPreviewKind, ChatRow, CHATS};
use domain::session_tree::ThreadRowWord;
use domain::stories::{story_row, StoryRing, STORIES};

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

/// The lead's BARE brand mark (the root's CLI, Claude when none): what a
/// waiting row's disc holds, and what a chat's story ring wears
/// (stories.json AVATARS: no badge, no PR glyph).
pub(crate) fn chat_brand_mark(agent: Option<coding::CodingAgent>, glyph: f32) -> AnyElement {
    crate::coding_selects::agent_mark(agent.unwrap_or_default())
        .with_size(px(glyph))
        .into_any_element()
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
    // S-3: the lead is the plain chat glyph (`shell.chatLead`), never the
    // brand mark (that is the Assistant's avatar now).
    let chat_glyph = || {
        Icon::new(registry::concept_by_name(&CHATS.shell.chat_lead).unwrap_or(registry::ACTION_CHAT))
            .with_size(px(GLYPH_PX))
            .text_color(theme.foreground)
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
        ChatLead::NeedsYou | ChatLead::Waiting => chat_glyph(),
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

/// S-3: THE agent avatar glyph: the working mark while busy, the
/// Exponential brand mark (foreground) for the viewer's Assistant, else the
/// agent's icon in its colour.
pub(crate) fn agent_avatar_glyph(icon: &str, color: &str, assistant: bool, busy: bool, size: f32, cx: &App) -> AnyElement {
    if busy {
        crate::coding_selects::agent_working_mark(None, size).into_any_element()
    } else if assistant {
        Icon::from(crate::icons::ExpIcon::Logo)
            .with_size(px(size))
            .text_color(cx.theme().foreground)
            .into_any_element()
    } else {
        crate::icons::agent_icon(Some(icon), Some(color)).with_size(px(size)).into_any_element()
    }
}

/// THE agent circle: `size` px, a `ring_width` ring (`ring` None = the glass
/// hairline) around a `ring_width` background gap around `glyph` on bg
/// muted. The bubble (66) and the composer agent row (40) both paint it.
pub(crate) fn agent_circle(size: f32, ring_width: f32, ring_gap: f32, ring: Option<gpui::Hsla>, glyph: AnyElement, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .border(px(ring_width))
        .border_color(ring.unwrap_or(gpui::transparent_black()))
        .p(px(ring_gap))
        .when(ring.is_none(), |this| {
            this.child(
                div()
                    .absolute()
                    .top(px(-ring_width))
                    .left(px(-ring_width))
                    .right(px(-ring_width))
                    .bottom(px(-ring_width))
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
        )
}

/// One agent bubble of THE STORY ROW (#EXP-1461, stories.json THE STORY
/// ROW): the agent's icon in its colour (the working mark while busy, the
/// brand mark for the viewer's Assistant) inside its agent's story ring
/// (the bubble size, the muted disc), the name `row.labelGap` under it,
/// medium while the ring is unseen. A bubble without a ring is never drawn.
/// The caller hangs the click and the agent menu.
pub(crate) fn agent_bubble(bubble: &AgentBubble, story: &StoryRing, cx: &App) -> Div {
    let theme = cx.theme();
    let agent = &bubble.agent;
    let glyph = agent_avatar_glyph(&agent.icon, &agent.color, agent.assistant, agent.busy, GLYPH_PX, cx);
    let disc = div()
        .size_full()
        .rounded_full()
        .bg(theme.muted)
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .into_any_element();
    crate::story_ring::story_row_item(&story.stories, disc, &agent.name, story.unseen, cx)
}

/// S-3 — one AGENT row pinned on top of the Chats list (chats.json
/// `agentChatRows`): THE chat row with the agent's avatar in the lead disc
/// (the brand mark for the viewer's Assistant), its name, the
/// conversation's preview, time and unread badge. AGENT ROWS: the disc
/// ALWAYS holds the avatar (never the working mark or a PR glyph); needs
/// you = the amber badge on it, busy reads only in the preview.
pub(crate) fn agent_chat_row(row: &domain::chats::AgentChatRow, cx: &App) -> Div {
    let lead_glyph = agent_avatar_glyph(&row.icon, &row.color, row.assistant, false, GLYPH_PX, cx);
    let lead = div()
        .relative()
        .size(px(CHATS.shell.lead_disc))
        .flex_shrink_0()
        .rounded_full()
        .bg(cx.theme().muted)
        .flex()
        .items_center()
        .justify_center()
        .child(lead_glyph)
        .when(row.needs_you, |disc| {
            disc.child(
                div()
                    .absolute()
                    .top(px(8.))
                    .right(px(8.))
                    .size(px(14.))
                    .rounded_full()
                    .border_2()
                    .border_color(cx.theme().muted)
                    .bg(theme::tokens::YELLOW.to_hsla()),
            )
        })
        .into_any_element();
    let as_chat = ChatRow {
        session_id: row.session_id.clone().unwrap_or_else(|| row.agent_id.clone()),
        team_id: String::new(),
        title: row.name.clone(),
        device_id: None,
        group: if row.needs_you {
            domain::session_tree::ThreadsListKey::NeedsYou
        } else if row.busy {
            domain::session_tree::ThreadsListKey::Running
        } else {
            domain::session_tree::ThreadsListKey::Recent
        },
        lead: ChatLead::Waiting,
        preview: row.preview.clone(),
        activity_at: row.activity_at.clone().unwrap_or_default(),
        time: row.time.clone(),
        unread_count: row.unread_count,
        needs_you: row.needs_you,
        busy: row.busy,
        word: None,
        dim: false,
    };
    chat_row(&as_chat, lead, cx)
}

/// S-3: a Chats list section header (`copy.sections`), shown only while an
/// agent row exists.
pub(crate) fn chat_section_header(label: &str, cx: &App) -> AnyElement {
    div()
        .px(px(ROW_PAD))
        .pt_2()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

/// A composer agent row tap (the option id; the temporary id = Temporary
/// session).
pub(crate) type OnAgentPick = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// S-3 — THE composer agent row (composer-home.json `agentRow`, styleguide
/// `composer-agent-row`): the muted label line (`To <name>` / `Temporary
/// session`) `labelToRow` over the single-select circles = the agent bubble
/// at `size` (selected = the foreground ring, others the glass hairline, no
/// name under it: the name is the tooltip), the dotted circle
/// (`temporaryIcon`) last; scrolls sideways; `rowToCard` over the card.
pub(crate) fn composer_agent_row(
    state: &domain::composer_agent_row::ComposerAgentRowState<'_>,
    on_pick: OnAgentPick,
    cx: &App,
) -> AnyElement {
    let spec = &*domain::composer_agent_row::COMPOSER_AGENT_ROW;
    let theme = cx.theme();
    let defaults = &domain::session_tree::THREADS_LIST.agent_defaults;
    let glyph_size = 18.;
    let mut circles: Vec<AnyElement> = Vec::new();
    let mut option = |id: String, name: String, glyph: AnyElement| {
        let selected = state.selected == id;
        let ring = selected.then_some(theme.foreground);
        let on_pick = on_pick.clone();
        let pick = id.clone();
        circles.push(
            div()
                .id(SharedString::from(spec.test_ids.option.replace("{id}", &id)))
                .flex_shrink_0()
                .cursor_pointer()
                .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(name.clone()).build(window, cx))
                .on_click(move |_, window, cx| on_pick(pick.clone(), window, cx))
                .child(agent_circle(spec.size, spec.ring_width, spec.ring_gap, ring, glyph, cx))
                .into_any_element(),
        );
    };
    for agent in &state.agents {
        let glyph = agent_avatar_glyph(
            agent.icon.as_deref().unwrap_or(defaults.icon.as_str()),
            agent.color.as_deref().unwrap_or(defaults.color.as_str()),
            agent.is_assistant(),
            false,
            glyph_size,
            cx,
        );
        option(agent.id.clone(), agent.name.clone().unwrap_or_default(), glyph);
    }
    let dotted = Icon::new(registry::concept_by_name(&spec.temporary_icon).unwrap_or(registry::UI_ICON_PLACEHOLDER))
        .with_size(px(glyph_size))
        .text_color(theme.muted_foreground)
        .into_any_element();
    option(spec.temporary_id.clone(), spec.copy.temporary.clone(), dotted);
    v_flex()
        .id(SharedString::from(spec.test_ids.row.clone()))
        .w_full()
        .min_w_0()
        .px_1()
        .pb(px((spec.row_to_card - 8.).max(0.)))
        .gap(px(spec.label_to_row))
        .child(
            div()
                .id(SharedString::from(spec.test_ids.label.clone()))
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(SharedString::from(state.label.clone())),
        )
        .child(
            h_flex()
                .id("composer-agent-circles")
                .w_full()
                .gap(px(spec.gap))
                .overflow_x_scroll()
                .children(circles),
        )
        .into_any_element()
}

/// THE STORY ROW of the Chats header (#EXP-1461, stories.json `row` +
/// `rowCases`): ONLY rings. The agent bubbles whose agent has a ring
/// (`story_row`, list_agents order), each wearing it, then the rings no
/// bubble wears (painted by `ring_avatar`); nothing = `None` (no row, no
/// reserved height). One line scrolling sideways, every item EXACTLY
/// `row.itemWidth` wide and never shrunk (`flex_none` on the caller's wrapper
/// too), `row.gap` apart, `row.padBottom` under the names. `wrap` hangs each
/// bubble's click + menu (its index, agent and ring), `wrap_ring` each
/// trailing ring's — the styleguide passes plain wrappers.
pub(crate) fn agent_bubbles_row(
    bubbles: &[AgentBubble],
    rings: &[StoryRing],
    ring_avatar: impl Fn(&StoryRing) -> AnyElement,
    wrap: impl Fn(usize, &AgentBubble, &StoryRing, Div) -> AnyElement,
    wrap_ring: impl Fn(usize, &StoryRing, Div) -> AnyElement,
    cx: &App,
) -> Option<AnyElement> {
    let ids: Vec<&str> = bubbles.iter().map(|bubble| bubble.agent.agent_id.as_str()).collect();
    let row = story_row(&ids, rings);
    if row.is_empty() {
        return None;
    }
    let spec = STORIES.row;
    let drawn: Vec<(usize, &AgentBubble, &StoryRing)> = row
        .bubbles
        .iter()
        .filter_map(|id| {
            let bubble = bubbles.iter().find(|bubble| bubble.agent.agent_id == *id)?;
            Some((bubble, row.by_agent.get(id)?))
        })
        .enumerate()
        .map(|(index, (bubble, ring))| (index, bubble, ring))
        .collect();
    Some(
        h_flex()
            .id("chat-agent-bubbles")
            .w_full()
            .min_w_0()
            .flex_none()
            .items_start()
            .gap(px(spec.gap))
            .pb(px(spec.pad_bottom))
            .overflow_x_scroll()
            .children(drawn.into_iter().map(|(index, bubble, ring)| {
                story_row_slot(wrap(index, bubble, ring, agent_bubble(bubble, ring, cx)))
            }))
            .children(row.rest.iter().enumerate().map(|(index, ring)| {
                story_row_slot(wrap_ring(index, ring, crate::story_ring::story_ring_button(ring, ring_avatar(ring), cx)))
            }))
            .into_any_element(),
    )
}

/// One item's slot in THE STORY ROW: exactly `row.itemWidth`, never shrunk
/// (gpui flex children shrink by default, the wrapper too).
fn story_row_slot(item: AnyElement) -> AnyElement {
    div().w(px(STORIES.row.item_width)).flex_none().child(item).into_any_element()
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
