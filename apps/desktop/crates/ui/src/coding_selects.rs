//! Shared model/effort choice selects for the coding surfaces (the
//! Start-coding dialog + Settings → Coding) — thin wrappers over
//! `gpui_component::select` carrying the CLOSED alias sets the CLI accepts
//! (mirrors `coding::settings::MODEL_ALIASES` / `EFFORT_LEVELS`, which
//! `Settings::load` normalizes against). Free-text model/effort inputs are
//! deleted (rework decision 6): every surface picks from these lists, so the
//! argv can never carry a value the CLI rejects.

use gpui::{App, AppContext as _, Entity, SharedString, Window};
use gpui_component::searchable_list::SearchableListItem;
use gpui_component::select::SelectState;
use gpui_component::IndexPath;

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

/// `--model` aliases — default Fable (rework decision 6).
pub const MODEL_CHOICES: [(&str, &str); 3] =
    [("Fable", "fable"), ("Opus", "opus"), ("Sonnet", "sonnet")];

/// `--effort` levels; blank = leave the flag off (the CLI's own default).
pub const EFFORT_CHOICES: [(&str, &str); 6] = [
    ("CLI default", ""),
    ("Low", "low"),
    ("Medium", "medium"),
    ("High", "high"),
    ("XHigh", "xhigh"),
    ("Max", "max"),
];

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
}
