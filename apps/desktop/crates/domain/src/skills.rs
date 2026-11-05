//! #EXP-1655/1660: team SKILLS, the desktop mirror of
//! `packages/domain-contract/src/skills.ts`. The spec is
//! `packages/domain-contract/fixtures/skills.json` (its `$comment`); the
//! tests below replay every case in it.

/// The fixture's `limits` (the server's `SKILL_*`, db-schema `domain.ts`).
pub const NAME_MAX: usize = 64;
pub const DESCRIPTION_MAX: usize = 1024;
pub const BODY_MAX_BYTES: usize = 64 * 1024;
pub const PER_SESSION: usize = 20;

/// `list` copy.
pub const TITLE: &str = "Skills";
pub const NEW_BUTTON: &str = "New skill";
pub const DEFAULT_PILL: &str = "Default";
pub const EMPTY_TITLE: &str = "No skills yet";

/// `page.fields` labels + placeholders, top to bottom.
pub const NAME_LABEL: &str = "Name";
pub const NAME_PLACEHOLDER: &str = "review-checklist";
pub const DESCRIPTION_LABEL: &str = "Description";
pub const DESCRIPTION_PLACEHOLDER: &str = "What the skill does and when to use it";
pub const BODY_LABEL: &str = "Instructions";
pub const BODY_PLACEHOLDER: &str = "The SKILL.md body, in markdown";
pub const DEFAULT_LABEL: &str = "Pre-select in new sessions";
pub const DELETE_LABEL: &str = "Delete skill";
pub const SAVE: &str = "Save";

/// `page.deleteConfirm`.
pub const DELETE_CONFIRM_TITLE: &str = "Delete this skill?";
pub const DELETE_CONFIRM_BODY: &str = "Sessions that picked it run without it from their next start.";
pub const DELETE_CONFIRM: &str = "Delete";
pub const DELETE_CANCEL: &str = "Cancel";

/// The Delete confirm (`page.deleteConfirm`) as THE Prompt: Cancel + a
/// destructive Delete, Return on Cancel.
pub fn delete_confirm() -> crate::prompts::Prompt {
    use crate::prompts::{Action, Role};
    const ACTIONS: &[Action] = &[
        Action { id: "cancel", label: DELETE_CANCEL, role: Role::Cancel },
        Action { id: "delete", label: DELETE_CONFIRM, role: Role::Destructive },
    ];
    crate::prompts::Prompt {
        id: "delete-skill",
        title: DELETE_CONFIRM_TITLE.to_string(),
        body: Some(DELETE_CONFIRM_BODY.to_string()),
        actions: ACTIONS,
        focus: "cancel",
    }
}

/// `page.errors`.
pub const ERR_NAME_REQUIRED: &str = "Name is required";
pub const ERR_NAME_TOO_LONG: &str = "Name is too long";
pub const ERR_NAME_INVALID: &str = "Use lowercase letters, numbers and hyphens";
pub const ERR_NAME_TAKEN: &str = "A skill with this name already exists";
pub const ERR_DESCRIPTION_REQUIRED: &str = "Description is required";
pub const ERR_DESCRIPTION_TOO_LONG: &str = "Description is too long";
pub const ERR_BODY_REQUIRED: &str = "Instructions are required";
pub const ERR_BODY_TOO_LONG: &str = "Instructions are too long";

/// `menuRow.label` / `agentRow.label` / `agentRow.none`.
pub const MENU_LABEL: &str = "Skills";
pub const AGENT_ROW_LABEL: &str = "Skills";
pub const AGENT_ROW_NONE: &str = "None";

/// A field the skill page validates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillField {
    Name,
    Description,
    Body,
}

impl SkillField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Description => "description",
            Self::Body => "body",
        }
    }
}

/// One inline error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillFormError {
    pub field: SkillField,
    pub message: &'static str,
}

/// `limits.namePattern` = `^[a-z0-9-]+$`.
fn name_matches(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The skill page's errors, keyed by field in field order (`[]` = valid).
/// Lengths count UTF-16 units like the TS reference for name/description
/// (names are ASCII anyway) and UTF-8 bytes for the body.
pub fn validate_skill_form(name: &str, description: &str, body: &str) -> Vec<SkillFormError> {
    let mut out = Vec::new();
    let name = name.trim();
    if name.is_empty() {
        out.push(SkillFormError { field: SkillField::Name, message: ERR_NAME_REQUIRED });
    } else if name.encode_utf16().count() > NAME_MAX {
        out.push(SkillFormError { field: SkillField::Name, message: ERR_NAME_TOO_LONG });
    } else if !name_matches(name) {
        out.push(SkillFormError { field: SkillField::Name, message: ERR_NAME_INVALID });
    }
    let description = description.trim();
    if description.is_empty() {
        out.push(SkillFormError { field: SkillField::Description, message: ERR_DESCRIPTION_REQUIRED });
    } else if description.encode_utf16().count() > DESCRIPTION_MAX {
        out.push(SkillFormError { field: SkillField::Description, message: ERR_DESCRIPTION_TOO_LONG });
    }
    if body.trim().is_empty() {
        out.push(SkillFormError { field: SkillField::Body, message: ERR_BODY_REQUIRED });
    } else if body.len() > BODY_MAX_BYTES {
        out.push(SkillFormError { field: SkillField::Body, message: ERR_BODY_TOO_LONG });
    }
    out
}

/// Anything with a skill name (the list rows, the picker rows).
pub trait Named {
    fn skill_name(&self) -> &str;
}

impl Named for crate::rows::SkillRow {
    fn skill_name(&self) -> &str {
        &self.name
    }
}

/// Settings › Skills and the + submenu order: by name, code-point order
/// (stable).
pub fn sort_skills<T: Named + Clone>(skills: &[T]) -> Vec<T> {
    let mut out = skills.to_vec();
    out.sort_by(|a, b| a.skill_name().cmp(b.skill_name()));
    out
}

/// What [`preselect_skill_ids`] reads off a skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreselectSkill {
    pub id: String,
    pub name: String,
    pub enabled_by_default: bool,
}

impl Named for PreselectSkill {
    fn skill_name(&self) -> &str {
        &self.name
    }
}

impl From<&crate::rows::SkillRow> for PreselectSkill {
    fn from(row: &crate::rows::SkillRow) -> Self {
        Self { id: row.id.clone(), name: row.name.clone(), enabled_by_default: row.enabled_by_default }
    }
}

/// The launch pick's seed (the MCP servers' rule): the saved pick clamped to
/// skills that still exist, else the enabled-by-default ones; list order.
pub fn preselect_skill_ids(skills: &[PreselectSkill], saved: Option<&[String]>) -> Vec<String> {
    let sorted = sort_skills(skills);
    match saved {
        Some(saved) => sorted
            .into_iter()
            .filter(|skill| saved.iter().any(|id| id == &skill.id))
            .map(|skill| skill.id)
            .collect(),
        None => sorted
            .into_iter()
            .filter(|skill| skill.enabled_by_default)
            .map(|skill| skill.id)
            .collect(),
    }
}

/// The + menu row's value: the number picked, none at zero.
pub fn skills_menu_value(count: usize) -> Option<String> {
    (count > 0).then(|| count.to_string())
}

/// The agent page row's value: the picked names (list order) through the
/// shared picker summary (`a` / `a, b` / `a, b +N`), `None` when empty.
pub fn agent_skills_value(names: &[&str]) -> String {
    match names.len() {
        0 => AGENT_ROW_NONE.to_string(),
        1 | 2 => names.join(", "),
        n => format!("{} +{}", names[..2].join(", "), n - 2),
    }
}

/// #VAPP-213: the built-in skills (`builtin`): names, the app editor's
/// delivery order, the list pill.
pub const BUILTIN_NAMES: [&str; 3] = ["exponential-apps", "exponential-genui", "exponential-motion"];
pub const EDITOR_DELIVERY: [&str; 3] = ["exponential-apps", "exponential-genui", "exponential-motion"];
pub const BUILTIN_PILL: &str = "Built-in";

/// The list row's pill (`pillCases`): Built-in for a built-in (never
/// Default), Default while a team skill is pre-selected, else none.
pub fn skill_pill(builtin: bool, enabled_by_default: bool) -> Option<&'static str> {
    if builtin {
        Some(BUILTIN_PILL)
    } else if enabled_by_default {
        Some(DEFAULT_PILL)
    } else {
        None
    }
}

/// Whether the skill page is editable (`editCases`): owners, never a built-in.
pub fn can_edit_skill(builtin: bool, owner: bool) -> bool {
    owner && !builtin
}

/// A built-in's file path (`filePathCases`): relative, `/`-separated, no
/// empty, `.` or `..` segment, no backslash or NUL, never SKILL.md itself.
pub fn is_skill_file_path(path: &str) -> bool {
    if path.is_empty() || path == "SKILL.md" || path.contains('\\') || path.contains('\0') {
        return false;
    }
    path.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/skills.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    /// `{repeat, times}` = that string repeated `times` times.
    fn text(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Object(o) => o["repeat"].as_str().unwrap().repeat(o["times"].as_u64().unwrap() as usize),
            other => panic!("not a text: {other}"),
        }
    }

    #[test]
    fn the_limits_and_copy_are_the_fixtures() {
        let f = fixture();
        assert_eq!(f["limits"]["nameMax"], NAME_MAX);
        assert_eq!(f["limits"]["namePattern"], "^[a-z0-9-]+$");
        assert_eq!(f["limits"]["descriptionMax"], DESCRIPTION_MAX);
        assert_eq!(f["limits"]["bodyMaxBytes"], BODY_MAX_BYTES);
        assert_eq!(f["limits"]["perSession"], PER_SESSION);
        assert_eq!(f["title"], TITLE);
        assert_eq!(f["list"]["newButton"], NEW_BUTTON);
        assert_eq!(f["list"]["defaultPill"], DEFAULT_PILL);
        assert_eq!(f["list"]["empty"]["title"], EMPTY_TITLE);
        let fields: Vec<(&str, &str, Option<&str>)> = f["page"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| {
                (field["id"].as_str().unwrap(), field["label"].as_str().unwrap(), field["placeholder"].as_str())
            })
            .collect();
        assert_eq!(
            fields,
            vec![
                ("name", NAME_LABEL, Some(NAME_PLACEHOLDER)),
                ("description", DESCRIPTION_LABEL, Some(DESCRIPTION_PLACEHOLDER)),
                ("body", BODY_LABEL, Some(BODY_PLACEHOLDER)),
                ("enabledByDefault", DEFAULT_LABEL, None),
                ("delete", DELETE_LABEL, None),
            ]
        );
        assert_eq!(f["page"]["save"], SAVE);
        let confirm = &f["page"]["deleteConfirm"];
        assert_eq!(confirm["title"], DELETE_CONFIRM_TITLE);
        assert_eq!(confirm["body"], DELETE_CONFIRM_BODY);
        assert_eq!(confirm["confirm"], DELETE_CONFIRM);
        assert_eq!(confirm["cancel"], DELETE_CANCEL);
        let errors = &f["page"]["errors"];
        assert_eq!(errors["nameRequired"], ERR_NAME_REQUIRED);
        assert_eq!(errors["nameTooLong"], ERR_NAME_TOO_LONG);
        assert_eq!(errors["nameInvalid"], ERR_NAME_INVALID);
        assert_eq!(errors["nameTaken"], ERR_NAME_TAKEN);
        assert_eq!(errors["descriptionRequired"], ERR_DESCRIPTION_REQUIRED);
        assert_eq!(errors["descriptionTooLong"], ERR_DESCRIPTION_TOO_LONG);
        assert_eq!(errors["bodyRequired"], ERR_BODY_REQUIRED);
        assert_eq!(errors["bodyTooLong"], ERR_BODY_TOO_LONG);
        assert_eq!(f["menuRow"]["label"], MENU_LABEL);
        assert_eq!(f["agentRow"]["label"], AGENT_ROW_LABEL);
        assert_eq!(f["agentRow"]["none"], AGENT_ROW_NONE);
        assert_eq!(f["menuValue"]["one"], "1");
        assert_eq!(f["menuValue"]["other"], "{n}");
    }

    #[test]
    fn validation_replays_the_fixture() {
        for case in fixture()["validationCases"].as_array().unwrap() {
            let form = &case["form"];
            let got: Vec<(String, String)> =
                validate_skill_form(&text(&form["name"]), &text(&form["description"]), &text(&form["body"]))
                    .into_iter()
                    .map(|e| (e.field.key().to_string(), e.message.to_string()))
                    .collect();
            let expected: Vec<(String, String)> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (e["field"].as_str().unwrap().to_string(), e["message"].as_str().unwrap().to_string()))
                .collect();
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn sort_replays_the_fixture() {
        for case in fixture()["sortCases"].as_array().unwrap() {
            let skills: Vec<PreselectSkill> = case["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| PreselectSkill { id: String::new(), name: n.as_str().unwrap().into(), enabled_by_default: false })
                .collect();
            let got: Vec<String> = sort_skills(&skills).into_iter().map(|s| s.name).collect();
            let expected: Vec<String> =
                case["expected"].as_array().unwrap().iter().map(|n| n.as_str().unwrap().to_string()).collect();
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn preselect_replays_the_fixture() {
        for case in fixture()["preselectCases"].as_array().unwrap() {
            let skills: Vec<PreselectSkill> = case["skills"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| PreselectSkill {
                    id: s["id"].as_str().unwrap().into(),
                    name: s["name"].as_str().unwrap().into(),
                    enabled_by_default: s["enabledByDefault"].as_bool().unwrap(),
                })
                .collect();
            let saved: Option<Vec<String>> = case["saved"]
                .as_array()
                .map(|ids| ids.iter().map(|id| id.as_str().unwrap().to_string()).collect());
            let expected: Vec<String> =
                case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap().to_string()).collect();
            assert_eq!(preselect_skill_ids(&skills, saved.as_deref()), expected, "{}", case["name"]);
        }
    }

    #[test]
    fn values_replay_the_fixture() {
        let f = fixture();
        for case in f["menuValueCases"].as_array().unwrap() {
            let count = case["count"].as_u64().unwrap() as usize;
            assert_eq!(skills_menu_value(count).as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
        for case in f["agentValueCases"].as_array().unwrap() {
            let names: Vec<&str> = case["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
            assert_eq!(agent_skills_value(&names), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_builtins_replay_the_fixture() {
        let f = fixture();
        let names: Vec<&str> = f["builtin"]["names"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, BUILTIN_NAMES);
        let delivery: Vec<&str> = f["builtin"]["editorDelivery"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(delivery, EDITOR_DELIVERY);
        assert_eq!(f["builtin"]["pill"], BUILTIN_PILL);
        for case in f["pillCases"].as_array().unwrap() {
            let skill = &case["skill"];
            let got = skill_pill(skill["builtin"].as_bool().unwrap(), skill["enabledByDefault"].as_bool().unwrap());
            assert_eq!(got, case["expected"].as_str(), "{}", case["name"]);
        }
        for case in f["editCases"].as_array().unwrap() {
            let got = can_edit_skill(case["skill"]["builtin"].as_bool().unwrap(), case["owner"].as_bool().unwrap());
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
        for case in f["filePathCases"].as_array().unwrap() {
            let got = is_skill_file_path(case["path"].as_str().unwrap());
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }
}
