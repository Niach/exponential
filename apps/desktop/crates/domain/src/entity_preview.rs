//! EXP-920 — the ONE entity-preview rule (web
//! `@exp/domain-contract/src/entity-preview.ts`'s byte-identical twin).
//!
//! An Exponential MCP tool's settled row carries `preview.refs` (contract
//! `entityRefKind`). Every client renders those refs the SAME way: one CHIP
//! per ref (a glyph + a short label), a hover card resolved from the client's
//! own synced rows, and a click that opens the entity's detail. This module
//! owns what is pure about that — the icon concept per kind, the product
//! nouns, the clamped chip label + detail and the `list` grouping — and is
//! byte-locked ×4 by `packages/domain-contract/fixtures/entity-chip.json`.
//!
//! The wire struct lives in `steer::frames::EntityRef` and `steer` depends on
//! THIS crate, so the rule takes a borrowed [`EntityRefView`] the caller
//! builds from whatever it holds.

/// A chip's label never runs past this many characters (code points); a
/// longer one is cut to `CHIP_LABEL_MAX - 1` and ends in an ellipsis.
pub const CHIP_LABEL_MAX: usize = 48;

/// One entity a tool's answer named, borrowed: `kind` = contract
/// `entityRefKind`; `id` = the row id (a `list` ref's id is its MEMBER kind);
/// `count` only on a `list`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityRefView<'a> {
    pub kind: &'a str,
    pub id: &'a str,
    pub identifier: Option<&'a str>,
    pub title: Option<&'a str>,
    pub count: Option<u32>,
}

impl<'a> EntityRefView<'a> {
    pub fn new(kind: &'a str, id: &'a str) -> Self {
        Self { kind, id, identifier: None, title: None, count: None }
    }
}

/// The icon CONCEPT a kind's chip and card header draw (`packages/icons`
/// `semantic`). A `list` chip draws its MEMBER kind's icon. Exactly the TS
/// table, in the contract's kind order.
pub const ENTITY_REF_ICON: &[(&str, &str)] = &[
    ("issue", "ui-issue"),
    ("board", "nav-boards"),
    ("action", "nav-actions"),
    ("automation", "nav-automations"),
    ("comment", "notification-issue-comment"),
    ("session", "coding-running"),
    ("label", "settings-labels"),
    ("status", "settings-statuses"),
    ("workflow", "nav-workflows"),
    ("device", "ui-device"),
    ("member", "ui-avatar-placeholder"),
    ("repository", "ui-repository"),
    ("team", "ui-team"),
    ("invite", "ui-invite"),
    ("notification", "nav-notifications"),
    ("thread", "nav-support"),
    ("attachment", "ui-attach"),
    ("list", "ui-checklist"),
];

const NOUNS: &[(&str, &str, &str)] = &[
    ("issue", "issue", "issues"),
    ("board", "board", "boards"),
    ("action", "action", "actions"),
    ("automation", "automation", "automations"),
    ("comment", "comment", "comments"),
    ("session", "run", "runs"),
    ("label", "label", "labels"),
    ("status", "status", "statuses"),
    ("workflow", "workflow", "workflows"),
    ("device", "device", "devices"),
    ("member", "member", "members"),
    ("repository", "repository", "repositories"),
    ("team", "team", "teams"),
    ("invite", "invite", "invites"),
    ("notification", "notification", "notifications"),
    ("thread", "thread", "threads"),
    ("attachment", "attachment", "attachments"),
    ("list", "list", "lists"),
];

/// The icon concept for a KIND, `None` for one this build does not know.
pub fn entity_kind_icon(kind: &str) -> Option<&'static str> {
    ENTITY_REF_ICON
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, icon)| *icon)
}

/// The product noun for a kind: singular for `count` 1, plural otherwise.
/// An unknown kind (a NEWER publisher) reads as `item`/`items`.
pub fn entity_kind_noun(kind: &str, count: u32) -> &'static str {
    let (singular, plural) = NOUNS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map(|(_, singular, plural)| (*singular, *plural))
        .unwrap_or(("item", "items"));
    if count == 1 {
        singular
    } else {
        plural
    }
}

/// The icon concept a ref draws: a `list` ref its member kind's, anything
/// else its own; an unknown kind falls back to the `list` glyph.
pub fn entity_ref_icon(kind: &str, id: &str) -> &'static str {
    let kind = if kind == "list" { id } else { kind };
    entity_kind_icon(kind).unwrap_or("ui-checklist")
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Cut to [`CHIP_LABEL_MAX`] code points, ellipsis included in the budget;
/// the kept prefix loses its trailing whitespace so no label ends in ` …`.
pub fn clamp_chip_label(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= CHIP_LABEL_MAX {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(CHIP_LABEL_MAX - 1).collect();
    format!("{}…", kept.trim_end())
}

fn present(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}

/// The chip's text:
/// - a `list`: `<count> <member noun>` ("3 issues", "1 run", "0 labels");
/// - an issue: its identifier, else its title, else "Issue";
/// - anything else: its title, else the capitalized noun ("Board", "Run").
/// Whitespace-only titles count as absent.
pub fn entity_chip_label(r#ref: &EntityRefView<'_>) -> String {
    if r#ref.kind == "list" {
        let count = r#ref.count.unwrap_or(0);
        return clamp_chip_label(&format!("{count} {}", entity_kind_noun(r#ref.id, count)));
    }
    let identifier = present(r#ref.identifier);
    let title = present(r#ref.title);
    if r#ref.kind == "issue" {
        if let Some(identifier) = identifier {
            return clamp_chip_label(identifier);
        }
    }
    if let Some(title) = title {
        return clamp_chip_label(title);
    }
    capitalize(entity_kind_noun(r#ref.kind, 1))
}

/// The secondary text an issue chip shows beside its identifier (the
/// title), `None` for every other kind or when the identifier already IS the
/// label.
pub fn entity_chip_detail(r#ref: &EntityRefView<'_>) -> Option<String> {
    if r#ref.kind != "issue" {
        return None;
    }
    present(r#ref.identifier)?;
    present(r#ref.title).map(clamp_chip_label)
}

/// One chip: the ref it draws and, for a `list`, the member refs its card
/// lists (indexes into the slice `group_preview_refs` was given).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRefGroup {
    pub r#ref: usize,
    pub members: Vec<usize>,
}

/// One chip per group: a `list` ref opens a group and absorbs every DIRECTLY
/// following ref of its member kind; the first ref of another kind (or
/// another list) closes it. Any other ref is a group of its own. Returns
/// INDEXES into `refs` so the caller can hand out whatever owns the rows.
pub fn group_preview_refs(refs: &[EntityRefView<'_>]) -> Vec<PreviewRefGroup> {
    let mut groups: Vec<PreviewRefGroup> = Vec::new();
    let mut open: Option<usize> = None;
    for (index, r#ref) in refs.iter().enumerate() {
        if let Some(open_index) = open {
            let list_id = refs[groups[open_index].r#ref].id;
            if r#ref.kind != "list" && r#ref.kind == list_id {
                groups[open_index].members.push(index);
                continue;
            }
        }
        groups.push(PreviewRefGroup { r#ref: index, members: Vec::new() });
        open = (r#ref.kind == "list").then_some(groups.len() - 1);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `entity-preview.test.ts`, iOS `EntityPreviewTests`, Android
    /// `EntityPreviewTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/entity-chip.json");

    #[derive(Deserialize)]
    struct FixtureRef {
        kind: String,
        id: String,
        #[serde(default)]
        identifier: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        count: Option<u32>,
    }

    impl FixtureRef {
        fn view(&self) -> EntityRefView<'_> {
            EntityRefView {
                kind: &self.kind,
                id: &self.id,
                identifier: self.identifier.as_deref(),
                title: self.title.as_deref(),
                count: self.count,
            }
        }
    }

    #[derive(Deserialize)]
    struct ChipCase {
        name: String,
        r#ref: FixtureRef,
        label: String,
        detail: Option<String>,
        icon: String,
    }

    #[derive(Deserialize)]
    struct GroupExpectation {
        r#ref: usize,
        members: Vec<usize>,
    }

    #[derive(Deserialize)]
    struct GroupCase {
        name: String,
        refs: Vec<FixtureRef>,
        groups: Vec<GroupExpectation>,
    }

    #[derive(Deserialize)]
    struct Fixture {
        chips: Vec<ChipCase>,
        groups: Vec<GroupCase>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("entity-chip.json parses")
    }

    #[test]
    fn every_chip_case_renders_byte_exact() {
        let fixture = fixture();
        assert!(!fixture.chips.is_empty());
        for case in &fixture.chips {
            let view = case.r#ref.view();
            assert_eq!(entity_chip_label(&view), case.label, "label: {}", case.name);
            assert_eq!(entity_chip_detail(&view), case.detail, "detail: {}", case.name);
            assert_eq!(entity_ref_icon(view.kind, view.id), case.icon, "icon: {}", case.name);
        }
    }

    #[test]
    fn every_group_case_groups_the_same() {
        let fixture = fixture();
        assert!(!fixture.groups.is_empty());
        for case in &fixture.groups {
            let views: Vec<EntityRefView<'_>> = case.refs.iter().map(FixtureRef::view).collect();
            let expected: Vec<PreviewRefGroup> = case
                .groups
                .iter()
                .map(|group| PreviewRefGroup { r#ref: group.r#ref, members: group.members.clone() })
                .collect();
            assert_eq!(group_preview_refs(&views), expected, "{}", case.name);
        }
    }

    /// Every contract kind has an icon concept and a noun pair; the icon
    /// table names no kind outside the contract. (The ui crate locks each
    /// concept to a registry glyph.)
    #[test]
    fn every_contract_kind_has_an_icon_and_a_noun() {
        for kind in crate::contract::ENTITY_REF_KIND_VALUES {
            assert!(entity_kind_icon(kind).is_some(), "{kind} has no icon concept");
            assert_ne!(entity_kind_noun(kind, 1), "item", "{kind} has no noun");
        }
        for (kind, _) in ENTITY_REF_ICON {
            assert!(
                crate::contract::ENTITY_REF_KIND_VALUES.contains(kind),
                "{kind} is not a contract kind"
            );
        }
        assert_eq!(ENTITY_REF_ICON.len(), crate::contract::ENTITY_REF_KIND_VALUES.len());
    }

    #[test]
    fn clamp_keeps_the_budget_and_drops_a_dangling_space() {
        let long = "a".repeat(60);
        let clamped = clamp_chip_label(&long);
        assert_eq!(clamped.chars().count(), CHIP_LABEL_MAX);
        assert!(clamped.ends_with('…'));
        let spaced = format!("{} {}", "b".repeat(46), "c".repeat(10));
        assert_eq!(clamp_chip_label(&spaced), format!("{}…", "b".repeat(46)));
        assert_eq!(clamp_chip_label("  short  "), "short");
    }
}
