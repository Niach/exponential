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

/// Release-run subagent model: [`MODEL_CHOICES`] with a leading blank
/// "Inherit" (= the orchestrator session's model).
pub const SUBAGENT_MODEL_CHOICES: [(&str, &str); 4] = [
    ("Inherit", ""),
    ("Fable", "fable"),
    ("Opus", "opus"),
    ("Sonnet", "sonnet"),
];

/// Release-run subagent effort: [`EFFORT_CHOICES`] with blank relabeled
/// "Inherit" (= the orchestrator session's effort).
pub const SUBAGENT_EFFORT_CHOICES: [(&str, &str); 6] = [
    ("Inherit", ""),
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
