//! VAPP-234 — the template gallery's list and copy, read from
//! `fixtures/app-templates.json` (TS `app-templates.ts`, iOS
//! `AppTemplates.swift`, Android `AppTemplates.kt`). Picking one is the
//! SERVER's (`apps.importTemplate`: the pinned signature, then the import).

use std::sync::LazyLock;

use serde::Deserialize;

const TEMPLATES_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/app-templates.json");

/// The fixture's `copy`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTemplatesCopy {
    pub title: String,
    pub from_template: String,
    pub include_records: String,
    pub note: String,
}

/// The fixture's `testIds`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTemplatesTestIds {
    pub row: String,
    pub records: String,
    pub from_template: String,
}

/// One gallery entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppTemplate {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplatesSpec {
    copy: AppTemplatesCopy,
    test_ids: AppTemplatesTestIds,
    templates: Vec<AppTemplate>,
}

static SPEC: LazyLock<TemplatesSpec> =
    LazyLock::new(|| serde_json::from_str(TEMPLATES_JSON).expect("app-templates.json parses"));

pub static APP_TEMPLATES_COPY: LazyLock<&'static AppTemplatesCopy> = LazyLock::new(|| &SPEC.copy);
pub static APP_TEMPLATES_TEST_IDS: LazyLock<&'static AppTemplatesTestIds> = LazyLock::new(|| &SPEC.test_ids);
/// The gallery, in order.
pub static APP_TEMPLATES: LazyLock<&'static [AppTemplate]> = LazyLock::new(|| SPEC.templates.as_slice());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_parses() {
        assert_eq!(APP_TEMPLATES.first().map(|t| t.id.as_str()), Some("time-tracker"));
        assert_eq!(APP_TEMPLATES_COPY.from_template, "Start from a template");
    }
}
