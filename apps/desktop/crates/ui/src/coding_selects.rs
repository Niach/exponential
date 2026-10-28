//! Shared model/effort choice selects for the coding surfaces (the
//! Start-coding dialog + Settings → Coding) — thin wrappers over
//! `gpui_component::select` carrying the CLOSED alias sets the CLI accepts
//! (mirrors `coding::settings::MODEL_ALIASES` / `EFFORT_LEVELS`, which
//! `Settings::load` normalizes against). Free-text model/effort inputs are
//! deleted (rework decision 6): every surface picks from these lists, so the
//! argv can never carry a value the CLI rejects.

use crate::controls::PointerMenuButtonExt as _;
use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, ParentElement as _, SharedString,
    Styled as _, Window,
};
use gpui_component::searchable_list::SearchableListItem;
use gpui_component::select::SelectState;
use gpui_component::{ActiveTheme as _, Icon, IndexPath, Sizable as _};

use crate::icons::registry;

/// One dropdown row: a display label + the argv value it stands for
/// (`""` = omit the flag / inherit).
#[derive(Clone)]
pub struct ChoiceItem {
    pub label: SharedString,
    pub value: SharedString,
}

impl SearchableListItem for ChoiceItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &SharedString {
        &self.value
    }

    /// picker-presentation.json (3): a long picked value ends in `…` on one
    /// line (the select itself only clips).
    fn display_title(&self) -> Option<gpui::AnyElement> {
        use gpui::IntoElement as _;
        Some(div().w_full().min_w_0().truncate().child(self.label.clone()).into_any_element())
    }
}

/// The select state every coding surface holds (delegate = a plain item vec —
/// these lists are tiny and never searched).
pub type ChoiceSelect = Entity<SelectState<Vec<ChoiceItem>>>;

/// THE claude model list — contract `codingModel` (fable, opus, sonnet,
/// haiku), the ONE set every claude model pick offers: the run's Model, the
/// Thread model a dispatcher root runs on, the Subagent model and the
/// Computer use model. Models are explicit ALWAYS: no blank "Default" /
/// "CLI default" row in any of them. These are the CLI's family aliases,
/// which it resolves to the newest model of that family (Claude Code ≥
/// 2.1.257: `fable` → `claude-fable-5-1`); the labels carry no version
/// number on purpose — see `coding::settings::DEFAULT_CLAUDE_MODEL`.
pub const MODEL_CHOICES: [(&str, &str); 4] = [
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
    ("Haiku", "haiku"),
];

/// Claude `--effort` levels; blank = leave the flag off (the CLI's own
/// default).
pub const EFFORT_CHOICES: [(&str, &str); 6] = [
    ("CLI default", ""),
    ("Low", "low"),
    ("Medium", "medium"),
    ("High", "high"),
    ("XHigh", "xhigh"),
    ("Max", "max"),
];

/// Codex `-m` slugs (the GPT-5.6 tiers, contract `codexModel`) — explicit
/// like claude's: no blank "CLI default" row.
pub const CODEX_MODEL_CHOICES: [(&str, &str); 3] = [
    ("GPT-5.6 Sol", "gpt-5.6-sol"),
    ("GPT-5.6 Terra", "gpt-5.6-terra"),
    ("GPT-5.6 Luna", "gpt-5.6-luna"),
];

/// Codex `model_reasoning_effort` levels (no `max`); blank = omit.
pub const CODEX_EFFORT_CHOICES: [(&str, &str); 6] = [
    ("CLI default", ""),
    ("Minimal", "minimal"),
    ("Low", "low"),
    ("Medium", "medium"),
    ("High", "high"),
    ("XHigh", "xhigh"),
];

/// The model choice list for `agent` (EXP-201 — the dialog + settings pane
/// swap their selects from these).
pub fn model_choices_for(agent: coding::CodingAgent) -> &'static [(&'static str, &'static str)] {
    match agent {
        coding::CodingAgent::Claude => &MODEL_CHOICES,
        coding::CodingAgent::Codex => &CODEX_MODEL_CHOICES,
    }
}

/// THE model select — every model pick of a settings surface (Model,
/// Thread model, Subagent model, Computer use model) is this one component
/// over `agent`'s whole list ([`model_choices_for`]), never a variant with a
/// default row.
pub fn model_select(
    agent: coding::CodingAgent,
    initial: &str,
    window: &mut Window,
    cx: &mut App,
) -> ChoiceSelect {
    choice_select(model_choices_for(agent), initial, window, cx)
}

/// The effort/reasoning/thinking choice list for `agent`.
pub fn effort_choices_for(agent: coding::CodingAgent) -> &'static [(&'static str, &'static str)] {
    match agent {
        coding::CodingAgent::Claude => &EFFORT_CHOICES,
        coding::CodingAgent::Codex => &CODEX_EFFORT_CHOICES,
    }
}

/// The agent's brand mark (EXP-206 — `assets/icons/{claude,codex}.svg`) for
/// the agent tab strips. Draw it through [`mark_icon`] / [`agent_mark`], never
/// a bare `Icon::from`: claude's mark is orange (EXP-877).
pub fn agent_icon(agent: coding::CodingAgent) -> crate::icons::ExpIcon {
    match agent {
        coding::CodingAgent::Claude => crate::icons::ExpIcon::Claude,
        coding::CodingAgent::Codex => crate::icons::ExpIcon::Codex,
    }
}

/// EXP-877: a brand mark as an [`Icon`] — Claude's in Anthropic's brand orange
/// ([`theme::CLAUDE_BRAND`], the fill the web, iOS and Android draw), every
/// other mark in the current text colour (Codex's has no brand colour; the
/// neutral agents concept is a registry glyph). Size it at the call site.
pub(crate) fn mark_icon(icon: crate::icons::ExpIcon) -> Icon {
    let claude = matches!(icon, crate::icons::ExpIcon::Claude);
    let mark = Icon::from(icon);
    if claude {
        mark.text_color(theme::CLAUDE_BRAND)
    } else {
        mark
    }
}

/// [`mark_icon`] for a shipped agent.
pub(crate) fn agent_mark(agent: coding::CodingAgent) -> Icon {
    mark_icon(agent_icon(agent))
}

/// EXP-1184 — Claude's own "writing" spark: 8 frames
/// (`assets/icons/claude-writing-{0..7}.svg`, generated by
/// `packages/icons` from the ONE sprite every client steps), hard cuts.
const CLAUDE_WRITING_FRAMES: [crate::icons::ExpIcon; 8] = [
    crate::icons::ExpIcon::ClaudeWriting0,
    crate::icons::ExpIcon::ClaudeWriting1,
    crate::icons::ExpIcon::ClaudeWriting2,
    crate::icons::ExpIcon::ClaudeWriting3,
    crate::icons::ExpIcon::ClaudeWriting4,
    crate::icons::ExpIcon::ClaudeWriting5,
    crate::icons::ExpIcon::ClaudeWriting6,
    crate::icons::ExpIcon::ClaudeWriting7,
];

/// One frame of the spark lasts 90 ms; the loop is 8 of them.
pub(crate) const CLAUDE_WRITING_FRAME: std::time::Duration = std::time::Duration::from_millis(90);
pub(crate) const CLAUDE_WRITING_LOOP: std::time::Duration =
    std::time::Duration::from_millis(90 * CLAUDE_WRITING_FRAMES.len() as u64);

/// The frame the spark shows `delta` (0..=1) of the way through its loop:
/// `floor(delta * 8)`, clamped (the loop's own end lands on the last frame,
/// never past it). Pure.
pub(crate) fn claude_writing_frame(delta: f32) -> usize {
    ((delta.max(0.) * CLAUDE_WRITING_FRAMES.len() as f32).floor() as usize)
        .min(CLAUDE_WRITING_FRAMES.len() - 1)
}

/// EXP-1184 — THE working mark (web `AgentWorkingMark`): Claude's
/// frame-stepped spark in its brand orange; an agent with no working art of
/// its own (codex, an unknown one = `None`'s generic glyph) keeps its mark
/// with the EXP-850 opacity pulse. `size` square. A live element — gpui
/// animates by repainting — so draw it only while the agent works.
///
/// EXP-1325: this mark lives in the sidebar, the tabs and the thread rows
/// for as long as a session works, and gpui repaints the WHOLE window for
/// every animation frame, at the monitor's rate while focused — so both
/// faces are STEPPED (`with_stepped_animation`): the spark repaints only
/// when its 90 ms frame changes, the pulse at the smooth step.
pub(crate) fn agent_working_mark(agent: Option<coding::CodingAgent>, size: f32) -> gpui::Div {
    use exponential_ui_gpui::controls::{SteppedAnimationExt as _, SMOOTH_STEP};
    let slot = div().flex_shrink_0().size(px(size));
    match agent {
        Some(coding::CodingAgent::Claude) => slot.child(
            div().size(px(size)).with_stepped_animation(
                "agent-working-claude",
                gpui::Animation::new(CLAUDE_WRITING_LOOP).repeat(),
                CLAUDE_WRITING_FRAME,
                move |frame, delta| {
                    frame.child(
                        Icon::from(CLAUDE_WRITING_FRAMES[claude_writing_frame(delta)].clone())
                            .text_color(theme::CLAUDE_BRAND)
                            .with_size(px(size)),
                    )
                },
            ),
        ),
        _ => {
            let mark = match agent {
                Some(agent) => agent_mark(agent),
                None => Icon::new(registry::SETTINGS_AGENTS),
            };
            slot.child(
                div().child(mark.with_size(px(size))).with_stepped_animation(
                    "agent-working-pulse",
                    gpui::Animation::new(crate::steer_viewer::WORKING_PULSE)
                        .repeat()
                        .with_easing(gpui::bounce(gpui::ease_in_out)),
                    SMOOTH_STEP,
                    |mark, delta| mark.opacity(0.4 + 0.6 * delta),
                ),
            )
        }
    }
}

/// EXP-1184 — a session state's corner badge (web `RUN_MARK_BADGE_CLASS`),
/// in the ONE session-dot palette: amber needs you, blue done; working and
/// idle wear none (S-3: the four session-card states).
pub(crate) fn run_badge_color(state: domain::session_tree::SessionState) -> Option<gpui::Hsla> {
    use domain::session_tree::SessionState;
    match state {
        SessionState::Working | SessionState::Idle => None,
        SessionState::NeedsYou => Some(theme::tokens::YELLOW.to_hsla()),
        SessionState::Done => Some(theme::tokens::BLUE.to_hsla()),
    }
}

/// EXP-923/EXP-1162/EXP-1184: a live run's LEAD — wherever a run is named
/// (the rail's Running rows, the Threads rows) it reads the same:
/// the agent's working mark ([`agent_working_mark`]) while it works, else its
/// brand mark (the generic agents concept for an unknown agent, `None`),
/// `size` square, with a `badge`-wide state dot ([`run_badge_color`]) on its
/// top trailing corner. No `state` = a paused run: the bare mark.
pub(crate) fn run_lead(
    agent: Option<coding::CodingAgent>,
    size: f32,
    badge: f32,
    state: Option<domain::session_tree::SessionState>,
) -> gpui::Div {
    if state == Some(domain::session_tree::SessionState::Working) {
        return div().relative().flex_shrink_0().child(agent_working_mark(agent, size));
    }
    let mark = match agent {
        Some(agent) => agent_mark(agent),
        None => Icon::new(registry::SETTINGS_AGENTS),
    };
    div()
        .relative()
        .flex_shrink_0()
        .child(mark.with_size(px(size)))
        .when_some(state.and_then(run_badge_color), |this, color| {
            this.child(
                div()
                    .absolute()
                    .top(px(-2.))
                    .right(px(-2.))
                    .size(px(badge))
                    .rounded_full()
                    .bg(color)
                    // The hairline keeps the dot readable over the glyph it
                    // overlaps.
                    .border_1()
                    .border_color(theme::tokens::BACKGROUND.to_hsla()),
            )
        })
}

/// EXP-862 — THE agent picker, one component per client (web
/// `components/agent-picker.tsx`, iOS `AgentPickerMenu`, Android
/// `AgentPickerPill`): an ICON-ONLY trigger (the selected agent's brand mark
/// plus a chevron) over a menu whose rows are the same brand mark and the
/// agent's label. The words "Claude Code" / "Codex" appear ONLY in the menu
/// and in the trigger's tooltip — a picker in a composer row says which agent
/// is selected with the mark, not with a name that changes the row's width.
///
/// Consumers: the composer's options row, the device-settings "Default
/// agent" row, Settings → Agents and the launch-options pane.
///
/// Returns an `AnyElement` because the trigger has to be a `Button` (upstream
/// implements `DropdownMenu` for nothing else) and its popover wrapper is not
/// a `Button` any more.
pub(crate) fn agent_picker(
    id: impl Into<gpui::ElementId>,
    agents: &[coding::CodingAgent],
    current: coding::CodingAgent,
    on_pick: impl Fn(coding::CodingAgent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> gpui::AnyElement {
    use gpui::{IntoElement as _, ParentElement as _};

    let glyph = crate::surface::PillSize::Sm.glyph();
    let agents: Vec<coding::CodingAgent> = agents.to_vec();
    let on_pick = std::rc::Rc::new(on_pick);
    crate::pickers::chip_button(id, cx)
        .tooltip(current.label())
        .child(agent_mark(current).size(gpui::px(glyph)))
        .child(
            Icon::from(crate::icons::registry::UI_CHEVRON_DOWN)
                .size(gpui::px(glyph))
                .text_color(cx.theme().muted_foreground),
        )
        .pointer_menu(move |menu, _window, _cx| {
            let on_pick = on_pick.clone();
            agent_menu_items(menu, &agents, Some(current), move |agent, window, cx| {
                on_pick(agent, window, cx)
            })
        })
        .into_any_element()
}

/// The rows of [`agent_picker`]'s menu, on their own so any menu that offers
/// agents (a device row's "..." menu, the add-account dialog) shows the SAME
/// rows: brand mark plus label, the selected one checked.
pub(crate) fn agent_menu_items(
    menu: gpui_component::menu::PopupMenu,
    agents: &[coding::CodingAgent],
    current: Option<coding::CodingAgent>,
    on_pick: impl Fn(coding::CodingAgent, &mut Window, &mut App) + 'static,
) -> gpui_component::menu::PopupMenu {
    let on_pick = std::rc::Rc::new(on_pick);
    let mut menu = crate::pickers::single_select_menu(menu);
    for agent in agents {
        let agent = *agent;
        let on_pick = on_pick.clone();
        menu = menu.item(crate::pickers::option_item(
            SharedString::from(agent.label()),
            agent_mark(agent),
            current == Some(agent),
            move |window, cx| on_pick(agent, window, cx),
        ));
    }
    menu
}

// ---------------------------------------------------------------------------
// EXP-872 — THE account picker
// ---------------------------------------------------------------------------

/// Build a select over `choices`, preselecting `initial` by VALUE (falling
/// back to the first row — every choice set puts its default first, and the
/// persisted settings values are load-normalized into these sets anyway).
pub fn choice_select(
    choices: &[(&str, &str)],
    initial: &str,
    window: &mut Window,
    cx: &mut App,
) -> ChoiceSelect {
    let items: Vec<ChoiceItem> = choices
        .iter()
        .map(|(label, value)| ChoiceItem {
            label: SharedString::from(*label),
            value: SharedString::from(*value),
        })
        .collect();
    let ix = choices
        .iter()
        .position(|(_, value)| *value == initial)
        .unwrap_or(0);
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default().row(ix)), window, cx))
}

/// NEW-59: the Work screen choices (`domain::work_screen::work_screen_options`
/// over the device's reported displays + the stored pick) as select items:
/// value `""` = Auto, else the display's stable id (#FEED-161).
pub fn work_screen_items(
    displays: &[domain::work_screen::WorkScreenDisplay],
    stored: Option<&str>,
) -> Vec<ChoiceItem> {
    domain::work_screen::work_screen_options(displays, stored)
        .into_iter()
        .map(|option| ChoiceItem {
            label: SharedString::from(option.label),
            value: work_screen_value(option.value.as_deref()),
        })
        .collect()
}

/// A Work screen pick as the select's value (`""` = Auto).
pub fn work_screen_value(id: Option<&str>) -> SharedString {
    id.map(|id| SharedString::from(id.to_string())).unwrap_or_default()
}

/// THE Work screen select: the device-settings picker row under the
/// Computer use model, preselecting `stored`.
pub fn work_screen_select(
    displays: &[domain::work_screen::WorkScreenDisplay],
    stored: Option<&str>,
    window: &mut Window,
    cx: &mut App,
) -> ChoiceSelect {
    let items = work_screen_items(displays, stored);
    let value = work_screen_value(stored);
    let ix = items.iter().position(|item| item.value == value).unwrap_or(0);
    cx.new(|cx| SelectState::new(items, Some(IndexPath::default().row(ix)), window, cx))
}

/// The Work screen select's pick, a display id (`None` = Auto).
pub fn selected_work_screen(state: &ChoiceSelect, cx: &App) -> Option<String> {
    Some(selected(state, cx)).filter(|id| !id.is_empty())
}

/// The currently selected VALUE (`""` when nothing is selected — only
/// possible transiently; every select is seeded with a selection).
pub fn selected(state: &ChoiceSelect, cx: &App) -> String {
    state
        .read(cx)
        .selected_value()
        .map(|value| value.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    /// Every model pick offers the contract's models, explicit: no blank
    /// row, and the computer-use / thread defaults are in the list.
    #[test]
    fn model_choices_are_the_contract_models_without_a_default_row() {
        let values: Vec<&str> = super::MODEL_CHOICES.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, domain::contract::CODING_MODEL_VALUES.to_vec());
        assert!(values.contains(&domain::contract::DEVICE_COMPUTER_USE_DEFAULTS_MODEL));
        assert!(values.contains(&domain::contract::DEVICE_AGENT_DEFAULTS_THREAD_MODEL));
        assert!(values.contains(&domain::contract::DEVICE_AGENT_DEFAULTS_SUBAGENT_MODEL));
        for (label, value) in super::MODEL_CHOICES {
            assert_eq!(label.to_lowercase(), value, "title-cased alias");
        }
        let codex: Vec<&str> = super::CODEX_MODEL_CHOICES.iter().map(|(_, value)| *value).collect();
        assert!(codex.contains(&domain::contract::DEVICE_AGENT_DEFAULTS_CODEX_THREAD_MODEL));
        assert!(codex.contains(&domain::contract::DEVICE_AGENT_DEFAULTS_CODEX_MODEL));
        for choices in [&super::MODEL_CHOICES[..], &super::CODEX_MODEL_CHOICES[..]] {
            assert!(choices.iter().all(|(_, value)| !value.is_empty()));
        }
    }

    /// EXP-1184: the spark steps 8 frames over its 720 ms loop, 90 ms each,
    /// never past the last; every state but working wears its badge.
    #[test]
    fn the_claude_spark_steps_eight_frames() {
        assert_eq!(super::CLAUDE_WRITING_LOOP.as_millis(), 720);
        assert_eq!(super::claude_writing_frame(0.), 0);
        assert_eq!(super::claude_writing_frame(0.124), 0);
        assert_eq!(super::claude_writing_frame(0.125), 1);
        assert_eq!(super::claude_writing_frame(0.5), 4);
        assert_eq!(super::claude_writing_frame(0.99), 7);
        assert_eq!(super::claude_writing_frame(1.), 7);
        assert_eq!(super::claude_writing_frame(-0.1), 0);

        use domain::session_tree::SessionState;
        assert_eq!(super::run_badge_color(SessionState::Working), None);
        assert_eq!(super::run_badge_color(SessionState::Idle), None);
        assert_eq!(
            super::run_badge_color(SessionState::NeedsYou),
            Some(theme::tokens::YELLOW.to_hsla())
        );
        assert_eq!(
            super::run_badge_color(SessionState::Done),
            Some(theme::tokens::BLUE.to_hsla())
        );
    }

    use super::*;

    // The `coding` crate deliberately does not depend on `domain` — this
    // crate depends on both, so the contract parity check lives here
    // (EXP-149: web/iOS/Android build their Start-coding dialogs from the
    // same contract lists, and the remote-start options they send must be
    // values these desktop sets accept).
    #[test]
    fn choice_sets_match_the_domain_contract_and_the_settings_alias_sets() {
        let models: Vec<&str> = MODEL_CHOICES.iter().map(|(_, value)| *value).collect();
        assert_eq!(models, domain::contract::CODING_MODEL_VALUES);
        assert_eq!(models, coding::settings::MODEL_ALIASES);

        // EFFORT_CHOICES[0] is the local-only "CLI default" blank row; the
        // contract carries only the real levels.
        assert_eq!(EFFORT_CHOICES[0].1, "");
        let efforts: Vec<&str> = EFFORT_CHOICES[1..].iter().map(|(_, value)| *value).collect();
        assert_eq!(efforts, domain::contract::CODING_EFFORT_VALUES);
        assert_eq!(efforts, coding::settings::EFFORT_LEVELS);
    }

    /// EXP-201: the agent list and the codex choice sets stay in lockstep
    /// with the domain contract AND the coding crate's closed sets (remote
    /// clients build their pickers from the same contract lists, and the
    /// options they send must be values these desktop sets accept).
    #[test]
    fn agent_choice_sets_match_the_contract_and_the_agent_consts() {
        let ids: Vec<&str> = coding::CodingAgent::ALL.iter().map(|a| a.id()).collect();
        assert_eq!(ids, domain::contract::CODING_AGENT_VALUES);

        // The codex models are explicit; the effort list leads with the
        // local-only blank "CLI default" row. The contract carries only the
        // real values.
        let models: Vec<&str> = CODEX_MODEL_CHOICES.iter().map(|(_, value)| *value).collect();
        assert_eq!(models, domain::contract::CODEX_MODEL_VALUES);
        assert_eq!(models, coding::agent::CODEX_MODELS);
        assert_eq!(CODEX_EFFORT_CHOICES[0].1, "");
        let efforts: Vec<&str> = CODEX_EFFORT_CHOICES[1..].iter().map(|(_, value)| *value).collect();
        assert_eq!(efforts, domain::contract::CODEX_EFFORT_VALUES);
        assert_eq!(efforts, coding::agent::CODEX_EFFORTS);

        // The per-agent accessors route to the right lists.
        assert_eq!(
            model_choices_for(coding::CodingAgent::Codex),
            &CODEX_MODEL_CHOICES
        );
        assert_eq!(
            effort_choices_for(coding::CodingAgent::Codex),
            &CODEX_EFFORT_CHOICES
        );
        assert_eq!(
            model_choices_for(coding::CodingAgent::Claude),
            &MODEL_CHOICES
        );
    }
}
