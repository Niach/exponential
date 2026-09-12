//! Shared model/effort choice selects for the coding surfaces (the
//! Start-coding dialog + Settings → Coding) — thin wrappers over
//! `gpui_component::select` carrying the CLOSED alias sets the CLI accepts
//! (mirrors `coding::settings::MODEL_ALIASES` / `EFFORT_LEVELS`, which
//! `Settings::load` normalizes against). Free-text model/effort inputs are
//! deleted (rework decision 6): every surface picks from these lists, so the
//! argv can never carry a value the CLI rejects.

use gpui::{App, AppContext as _, Entity, SharedString, Styled as _, Window};
use gpui_component::searchable_list::SearchableListItem;
use gpui_component::select::SelectState;
use gpui_component::{ActiveTheme as _, Icon, IndexPath};

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
}

/// The select state every coding surface holds (delegate = a plain item vec —
/// these lists are tiny and never searched).
pub type ChoiceSelect = Entity<SelectState<Vec<ChoiceItem>>>;

/// Claude `--model` aliases — default Fable (rework decision 6; Claude is
/// explicit-always, so no blank row here). These are the CLI's family
/// aliases, which it resolves to the newest model of that family (Claude
/// Code ≥ 2.1.257: `fable` → `claude-fable-5-1`); the labels carry no
/// version number on purpose — see `coding::settings::DEFAULT_CLAUDE_MODEL`.
pub const MODEL_CHOICES: [(&str, &str); 3] =
    [("Fable", "fable"), ("Opus", "opus"), ("Sonnet", "sonnet")];

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

/// The agent tabs/select rows (EXP-201) — mirrors `CodingAgent::ALL` order.
pub const AGENT_CHOICES: [(&str, &str); 2] = [("Claude Code", "claude"), ("Codex", "codex")];

/// Codex `-m` slugs (the GPT-5.6 tiers); blank = codex's own default model.
pub const CODEX_MODEL_CHOICES: [(&str, &str); 4] = [
    ("CLI default", ""),
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

/// The effort/reasoning/thinking choice list for `agent`.
pub fn effort_choices_for(agent: coding::CodingAgent) -> &'static [(&'static str, &'static str)] {
    match agent {
        coding::CodingAgent::Claude => &EFFORT_CHOICES,
        coding::CodingAgent::Codex => &CODEX_EFFORT_CHOICES,
    }
}

/// The agent's brand mark (EXP-206 — `assets/icons/{claude,codex}.svg`,
/// rendered theme-tinted like every bundled icon) for the agent tab strips.
pub fn agent_icon(agent: coding::CodingAgent) -> crate::icons::ExpIcon {
    match agent {
        coding::CodingAgent::Claude => crate::icons::ExpIcon::Claude,
        coding::CodingAgent::Codex => crate::icons::ExpIcon::Codex,
    }
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
    use gpui_component::menu::DropdownMenu as _;

    let glyph = crate::surface::PillSize::Sm.glyph();
    let agents: Vec<coding::CodingAgent> = agents.to_vec();
    let on_pick = std::rc::Rc::new(on_pick);
    crate::pickers::chip_button(id, cx)
        .tooltip(current.label())
        .child(Icon::from(agent_icon(current)).size(gpui::px(glyph)))
        .child(
            Icon::from(crate::icons::registry::UI_CHEVRON_DOWN)
                .size(gpui::px(glyph))
                .text_color(cx.theme().muted_foreground),
        )
        .dropdown_menu(move |menu, _window, _cx| {
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
    let mut menu = menu;
    for agent in agents {
        let agent = *agent;
        let on_pick = on_pick.clone();
        menu = menu.item(crate::pickers::option_item(
            SharedString::from(agent.label()),
            Icon::from(agent_icon(agent)),
            current == Some(agent),
            move |window, cx| on_pick(agent, window, cx),
        ));
    }
    menu
}

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
        let agents: Vec<&str> = AGENT_CHOICES.iter().map(|(_, value)| *value).collect();
        assert_eq!(agents, domain::contract::CODING_AGENT_VALUES);
        let ids: Vec<&str> = coding::CodingAgent::ALL.iter().map(|a| a.id()).collect();
        assert_eq!(agents, ids);

        // Every codex list leads with the local-only blank "CLI default"
        // row; the contract carries only the real values.
        for (choices, contract_values, agent_values) in [
            (
                &CODEX_MODEL_CHOICES[..],
                domain::contract::CODEX_MODEL_VALUES,
                &coding::agent::CODEX_MODELS[..],
            ),
            (
                &CODEX_EFFORT_CHOICES[..],
                domain::contract::CODEX_EFFORT_VALUES,
                &coding::agent::CODEX_EFFORTS[..],
            ),
        ] {
            assert_eq!(choices[0].1, "");
            let values: Vec<&str> = choices[1..].iter().map(|(_, value)| *value).collect();
            assert_eq!(values, contract_values);
            assert_eq!(values, agent_values);
        }

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
