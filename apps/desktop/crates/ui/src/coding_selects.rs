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

/// EXP-981: the claude SUBAGENT model picks — the same aliases as
/// [`MODEL_CHOICES`], with the blank "CLI default" in front (a subagent pin
/// is optional, unlike claude's explicit-always `--model`). The labels are
/// the Model picker's own, so the two rows read alike.
pub const SUBAGENT_MODEL_CHOICES: [(&str, &str); 4] = [
    (crate::launch_options::CLI_DEFAULT_LABEL, ""),
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
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

/// The WORKFLOW pair's choice list for `agent`: [`model_choices_for`] minus
/// the blank "CLI default" row. The pair always names a model
/// (`coding::settings::normalize_workflow_pair`), and the server drops the pair WHOLE
/// when one half is invalid, so a blank pick from the plain codex list wiped
/// the machine's stored pair.
pub fn workflow_model_choices_for(
    agent: coding::CodingAgent,
) -> Vec<(&'static str, &'static str)> {
    model_choices_for(agent)
        .iter()
        .copied()
        .filter(|(_, value)| !value.is_empty())
        .collect()
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
        .child(agent_mark(current).size(gpui::px(glyph)))
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

/// Where an [`account_picker`] is drawn, which is the only thing that varies
/// between its two homes. Both are BARE ghost triggers: an account pick is
/// already inside a card (the composer's options line, a `glass_picker_row`),
/// and a bordered pill inside a row would be a second edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountTrigger {
    /// The Agent page composer's muted options line (`text_xs`, muted).
    Inline,
    /// A settings `glass_picker_row`'s trailing control (`text_sm`, the
    /// normal foreground — the value of the row, not a caption).
    Row,
}

/// EXP-872 — THE account picker, one component per client (web
/// `@exp/ui` `AccountPicker`, iOS `AccountPickerMenu`, Android
/// `AccountPickerPill`). It REPLACES the agent picker + the account picker on
/// every launch surface: the list is every signed-in login the target machine
/// reports ACROSS agents ([`coding::flatten_accounts`]), and picking one
/// IMPLIES its agent.
///
/// The trigger and every row read the same way: the agent's brand mark + the
/// login's EMAIL, never the profile name and never the word "default" — the
/// device default is simply the first row. A dead credential rides as a muted
/// health badge beside the email.
///
/// With exactly ONE option there is nothing to pick, so the trigger collapses
/// to plain text without a chevron (the mark and the email still say which
/// login the run spends). An EMPTY list renders nothing at all — a caller
/// with no login to offer builds its own fallback rows.
///
/// EXP-1030: the SURFACE is THE picker primitive's
/// ([`crate::picker::account_picker`]) — this function is the trigger and the
/// row body, nothing else. Every launch surface (the composer's account pin,
/// the automation editor's, device settings, Settings → Agents) reaches the
/// primitive through here, so none of them holds a menu of its own any more.
pub(crate) fn account_picker(
    id: impl Into<gpui::ElementId>,
    options: &[coding::AccountOption],
    current_key: Option<&str>,
    variant: AccountTrigger,
    on_pick: impl Fn(&coding::AccountOption, &mut Window, &mut App) + 'static,
    cx: &App,
) -> gpui::AnyElement {
    use gpui::{IntoElement as _, ParentElement as _, Styled as _};
    use gpui_component::button::ButtonVariants as _;

    let Some(current) = options
        .iter()
        .find(|option| Some(option.account_option_key().as_str()) == current_key)
        .or_else(|| coding::default_account_option(options))
    else {
        return gpui::div().into_any_element();
    };
    let muted = cx.theme().muted_foreground;
    // Web parity (`AccountOptionLabel`'s `title`): the brand name the mark
    // stands for, and the login it is.
    let tooltip: SharedString =
        format!("{} \u{b7} {}", current.agent.label(), current.email).into();
    let label = account_label_row(current, variant, cx);
    if options.len() < 2 {
        // Nothing to pick: the same line, without the affordance.
        return gpui::div()
            .flex()
            .items_center()
            .px_1()
            .child(label)
            .into_any_element();
    }
    let id: gpui::ElementId = id.into();
    let current_key = current.account_option_key();
    let options: Vec<coding::AccountOption> = options.to_vec();
    let trigger = gpui_component::button::Button::new(id.clone())
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_1()
        .py_0()
        .tooltip(tooltip)
        .child(label)
        .child(
            Icon::from(crate::icons::registry::UI_CHEVRON_DOWN)
                .size(gpui::px(12.))
                .text_color(muted),
        )
        .into_any_element();
    let rows = options.clone();
    crate::picker::deferred(move |window, cx| {
        let on_change: crate::picker::OnPickerChange<String> = {
            let options = options.clone();
            std::rc::Rc::new(move |values: Vec<String>, window: &mut Window, cx: &mut App| {
                let Some(key) = values.into_iter().next() else {
                    return;
                };
                if let Some(option) = options
                    .iter()
                    .find(|option| option.account_option_key() == key)
                {
                    on_pick(option, window, cx);
                }
            })
        };
        crate::picker::account_picker::account_picker(
            &options,
            Some(current_key.to_string()),
            trigger,
            on_change,
        )
        // Several account pins can paint from one source line (the `⋯`
        // overlay beside the composer's), so each one names itself.
        .id(id.clone())
        // The primitive's default body is mark + email; an account row also
        // carries the health badge and the EXP-992 usage preview.
        .render_item(move |item, cx| account_row_body(&rows, &item.value, cx))
        .render(window, cx)
    })
    .into_any_element()
}

/// One account ROW inside the picker: [`account_label_row`] plus, for a login
/// that reports usage, the EXP-992 rate-limit preview. It rides a TOOLTIP
/// rather than an anchored card of its own — a picker row is already inside a
/// deferred overlay (an absolute child would clip against the surface's
/// bounds), and a tooltip is display-only, so hovering one can never swallow
/// the click that picks it.
fn account_row_body(
    options: &[coding::AccountOption],
    key: &str,
    cx: &App,
) -> gpui::AnyElement {
    use gpui::prelude::FluentBuilder as _;
    use gpui::{
        InteractiveElement as _, IntoElement as _, ParentElement as _,
        StatefulInteractiveElement as _, Styled as _,
    };

    let Some(option) = options
        .iter()
        .find(|option| option.account_option_key() == key)
    else {
        return gpui::div().into_any_element();
    };
    let limits = option.limits.clone();
    gpui::div()
        .id(SharedString::from(format!("account-option-{key}")))
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .child(account_label_row(option, AccountTrigger::Row, cx))
        .when_some(limits, |this, limits| {
            this.tooltip(move |window, cx| {
                let limits = limits.clone();
                gpui_component::tooltip::Tooltip::element(move |_, cx| {
                    crate::usage_bar::render_account_limits(&limits, cx)
                })
                .build(window, cx)
            })
        })
        .into_any_element()
}

/// How wide an email may grow before it ellipsises.
const ACCOUNT_EMAIL_MAX_W: f32 = 200.;

/// The trigger's (and a row's) one line: brand mark + email + the muted
/// health badge, `Row`-sized or `Inline`-sized.
fn account_label_row(
    option: &coding::AccountOption,
    variant: AccountTrigger,
    cx: &App,
) -> impl gpui::IntoElement {
    use gpui::prelude::FluentBuilder as _;
    use gpui::{ParentElement as _, Styled as _};
    let muted = cx.theme().muted_foreground;
    let inline = variant == AccountTrigger::Inline;
    gpui_component::h_flex()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .when(inline, |row| row.text_xs().text_color(muted))
        .when(!inline, |row| row.text_sm())
        .child(agent_mark(option.agent).size(gpui::px(12.)))
        .child(
            gpui::div()
                .min_w_0()
                // A login is an ADDRESS — capped so one long one can never
                // push the composer's options line past the card.
                .max_w(gpui::px(ACCOUNT_EMAIL_MAX_W))
                .truncate()
                .child(SharedString::from(option.email.clone())),
        )
        .children(option.health.badge_label().map(|badge| {
            gpui::div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(badge)
        }))
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
    /// EXP-981: the subagent picks are the MODEL picks with the blank
    /// "CLI default" in front — same aliases, same labels, so the two rows
    /// can never name the same model differently.
    #[test]
    fn subagent_model_choices_mirror_the_model_choices() {
        use super::{MODEL_CHOICES, SUBAGENT_MODEL_CHOICES};
        assert_eq!(SUBAGENT_MODEL_CHOICES[0].1, "");
        assert_eq!(
            SUBAGENT_MODEL_CHOICES[0].0,
            crate::launch_options::CLI_DEFAULT_LABEL
        );
        assert_eq!(&SUBAGENT_MODEL_CHOICES[1..], &MODEL_CHOICES[..]);
    }

    /// The workflow pair never holds a blank: its selects drop the
    /// "CLI default" row and keep every real model, in the model list's
    /// own order, so a workflow pick can never wipe the stored pair.
    #[test]
    fn workflow_model_choices_carry_no_blank_row() {
        use super::{model_choices_for, workflow_model_choices_for};
        for agent in coding::CodingAgent::ALL {
            let choices = workflow_model_choices_for(agent);
            assert!(!choices.is_empty(), "{agent:?} offers at least one model");
            assert!(
                choices.iter().all(|(_, value)| !value.is_empty()),
                "{agent:?} has no blank row"
            );
            let named: Vec<_> = model_choices_for(agent)
                .iter()
                .copied()
                .filter(|(_, value)| !value.is_empty())
                .collect();
            assert_eq!(choices, named, "{agent:?} keeps every real model in order");
            assert_eq!(
                choices.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
                agent.model_values(),
                "{agent:?} mirrors the agent's closed model set"
            );
        }
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
