//! #EXP-1660: THE skills multi picker (skills.json `menuRow` / `agentRow`):
//! the MCP servers picker's twin over the team's skills (sorted by name;
//! row = the `ui-docs` glyph + the name, caption = the description). The
//! composer's "+" › Skills and the agent page's Skills row both open it.

use gpui::{AnyElement, SharedString};

use crate::icons::registry;
use crate::launch_options::SkillOption;

use super::{OnPickerChange, Picker, PickerItem};

/// The search list's empty line.
pub(crate) const NO_SKILLS_FOUND: &str = "No skills found.";

pub(crate) fn skill_picker_items(skills: &[SkillOption]) -> Vec<PickerItem<String>> {
    skills
        .iter()
        .map(|skill| {
            PickerItem::new(skill.id.clone(), skill.name.clone())
                .icon(gpui_component::Icon::from(registry::UI_DOCS))
                .description(skill.description.clone())
                .keywords(vec![SharedString::from(skill.name.clone())])
        })
        .collect()
}

pub(crate) fn skill_picker(
    skills: &[SkillOption],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(skill_picker_items(skills), value, trigger, on_change)
        .search(true)
        .empty_text(NO_SKILLS_FOUND)
}
