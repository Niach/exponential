//! EXP-736 issue relations — the picker vocabulary, the per-side labels and
//! the timeline phrases, mirrored from `apps/web/src/lib/issue-relations.ts`.
//!
//! Canonical direction (the row as stored): `blocks` = `issue_id` blocks
//! `related_issue_id`; `parent` = `issue_id` is the parent; `duplicate` =
//! `issue_id` is the duplicate and `related_issue_id` the canonical issue;
//! `related` is symmetric (the server normalizes `issue_id < related_issue_id`).
//! Reading a row from the OTHER issue's side flips it — that is the `inverse`
//! flag every function here takes, and the labels come from the generated
//! contract's two label slices, never from a hand-written table.
//!
//! `domain` is gpui-free: [`RelationPick`] carries the icon CONCEPT NAME
//! (`packages/icons/icons.json`), and the `ui` crate maps it to its glyph.

use crate::contract;

/// One entry of the "Add relation" menu: the phrasing the user picks, and the
/// `(type, inverse)` pair it posts to `relations.create`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationPick {
    /// Menu label (sentence case, web parity).
    pub label: &'static str,
    /// Contract `issue_relation_type` value.
    pub kind: &'static str,
    /// `true` when the picked phrasing reads the relation from the OTHER
    /// side — the server flips the pair before storing the canonical row.
    pub inverse: bool,
    /// Icon concept name (`packages/icons/icons.json`).
    pub icon: &'static str,
}

/// The six picks, in menu order (web `IssueRelationsCard` dropdown).
pub const RELATION_PICKS: [RelationPick; 6] = [
    RelationPick {
        label: "Parent of",
        kind: contract::ISSUE_RELATION_TYPE_PARENT,
        inverse: false,
        icon: "relation-parent",
    },
    RelationPick {
        label: "Sub-issue of",
        kind: contract::ISSUE_RELATION_TYPE_PARENT,
        inverse: true,
        icon: "relation-sub-issue",
    },
    RelationPick {
        label: "Blocking",
        kind: contract::ISSUE_RELATION_TYPE_BLOCKS,
        inverse: false,
        icon: "relation-blocks",
    },
    RelationPick {
        label: "Blocked by",
        kind: contract::ISSUE_RELATION_TYPE_BLOCKS,
        inverse: true,
        icon: "relation-blocked-by",
    },
    RelationPick {
        label: "Duplicate of",
        kind: contract::ISSUE_RELATION_TYPE_DUPLICATE,
        inverse: false,
        icon: "relation-duplicate",
    },
    RelationPick {
        label: "Related to",
        kind: contract::ISSUE_RELATION_TYPE_RELATED,
        inverse: false,
        icon: "relation-related",
    },
];

/// The label a relation wears when read from one side: the contract's
/// `forwardLabels`/`inverseLabels` entry for `kind`. An unknown type (a newer
/// server) degrades to a generic phrase instead of vanishing — the row still
/// names the issue it points at.
pub fn label(kind: &str, inverse: bool) -> &'static str {
    let labels = if inverse {
        contract::ISSUE_RELATION_TYPE_INVERSE_LABELS
    } else {
        contract::ISSUE_RELATION_TYPE_FORWARD_LABELS
    };
    contract::ISSUE_RELATION_TYPE_VALUES
        .iter()
        .position(|value| *value == kind)
        .and_then(|index| labels.get(index).copied())
        .unwrap_or("linked to")
}

/// The icon concept name for one side of a relation — the pick's icon,
/// resolved through the same `(kind, inverse)` pair. Unknown types fall back
/// to the section glyph.
pub fn icon_name(kind: &str, inverse: bool) -> &'static str {
    RELATION_PICKS
        .iter()
        .find(|pick| pick.kind == kind && pick.inverse == inverse)
        .map(|pick| pick.icon)
        // `related` is symmetric: there is no inverse pick for it.
        .or_else(|| {
            RELATION_PICKS
                .iter()
                .find(|pick| pick.kind == kind)
                .map(|pick| pick.icon)
        })
        .unwrap_or("relation-section")
}

/// The timeline phrase of a `relation_added`/`relation_removed` event, byte-
/// identical to the web `relationEventPhrase`. Payload:
/// `{type, relatedIssueId, relatedIdentifier, direction: forward|inverse,
/// source}`. Returns `None` ONLY for another event type.
///
/// Both degrade paths mirror the web (and iOS `EventPhrases`), because an old
/// row or a hard-deleted counterpart must still read as something: an
/// unknown/missing `type` reads as the symmetric `related` (never the
/// row-caption "linked to" fallback [`label`] keeps), and a missing identifier
/// is named "an issue".
pub fn event_phrase(event_type: &str, payload: &serde_json::Value) -> Option<String> {
    let added = match event_type {
        contract::ISSUE_EVENT_TYPE_RELATION_ADDED => true,
        contract::ISSUE_EVENT_TYPE_RELATION_REMOVED => false,
        _ => return None,
    };
    let identifier = payload
        .get("relatedIdentifier")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|identifier| !identifier.is_empty())
        .unwrap_or("an issue");
    let kind = payload
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let inverse = payload.get("direction").and_then(|value| value.as_str()) == Some("inverse");

    let known = contract::ISSUE_RELATION_TYPE_VALUES.contains(&kind);
    if !known || kind == contract::ISSUE_RELATION_TYPE_RELATED {
        let verb = if added { "added" } else { "removed" };
        return Some(format!("{verb} related issue {identifier}"));
    }
    let label = label(kind, inverse);
    Some(if added {
        format!("marked as {label} {identifier}")
    } else {
        format!("no longer {label} {identifier}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_cover_every_contract_type() {
        for value in contract::ISSUE_RELATION_TYPE_VALUES {
            assert!(
                RELATION_PICKS.iter().any(|pick| pick.kind == *value),
                "no pick for relation type {value}"
            );
        }
        for pick in RELATION_PICKS {
            assert!(
                contract::ISSUE_RELATION_TYPE_VALUES.contains(&pick.kind),
                "pick {} names a non-contract type {}",
                pick.label,
                pick.kind
            );
        }
        // `related` is symmetric — it must NOT offer an inverse pick.
        assert!(!RELATION_PICKS
            .iter()
            .any(|pick| pick.kind == contract::ISSUE_RELATION_TYPE_RELATED && pick.inverse));
    }

    #[test]
    fn labels_come_from_the_generated_contract() {
        assert_eq!(label("blocks", false), "blocks");
        assert_eq!(label("blocks", true), "blocked by");
        assert_eq!(label("parent", false), "parent of");
        assert_eq!(label("parent", true), "sub-issue of");
        assert_eq!(label("duplicate", false), "duplicate of");
        assert_eq!(label("duplicate", true), "duplicated by");
        assert_eq!(label("related", false), "related to");
        assert_eq!(label("related", true), "related to");
        // Every contract value resolves through the slices, never the
        // fallback (a new type without labels is a codegen bug).
        for (index, value) in contract::ISSUE_RELATION_TYPE_VALUES.iter().enumerate() {
            assert_eq!(
                label(value, false),
                contract::ISSUE_RELATION_TYPE_FORWARD_LABELS[index]
            );
            assert_eq!(
                label(value, true),
                contract::ISSUE_RELATION_TYPE_INVERSE_LABELS[index]
            );
        }
        assert_eq!(label("something_new", false), "linked to");
    }

    #[test]
    fn icons_resolve_per_side() {
        assert_eq!(icon_name("parent", false), "relation-parent");
        assert_eq!(icon_name("parent", true), "relation-sub-issue");
        assert_eq!(icon_name("blocks", false), "relation-blocks");
        assert_eq!(icon_name("blocks", true), "relation-blocked-by");
        // Symmetric + inverse-less types fall back to their forward icon.
        assert_eq!(icon_name("related", true), "relation-related");
        assert_eq!(icon_name("duplicate", true), "relation-duplicate");
        assert_eq!(icon_name("something_new", false), "relation-section");
    }

    #[test]
    fn event_phrases_mirror_the_web_strings() {
        let payload = json!({
            "type": "related",
            "relatedIssueId": "i-2",
            "relatedIdentifier": "EXP-12",
            "direction": "forward",
            "source": "reference"
        });
        assert_eq!(
            event_phrase("relation_added", &payload).unwrap(),
            "added related issue EXP-12"
        );
        assert_eq!(
            event_phrase("relation_removed", &payload).unwrap(),
            "removed related issue EXP-12"
        );

        let blocks = json!({
            "type": "blocks",
            "relatedIdentifier": "EXP-3",
            "direction": "forward",
            "source": "user"
        });
        assert_eq!(
            event_phrase("relation_added", &blocks).unwrap(),
            "marked as blocks EXP-3"
        );
        assert_eq!(
            event_phrase("relation_removed", &blocks).unwrap(),
            "no longer blocks EXP-3"
        );

        let blocked_by = json!({
            "type": "blocks",
            "relatedIdentifier": "EXP-3",
            "direction": "inverse"
        });
        assert_eq!(
            event_phrase("relation_added", &blocked_by).unwrap(),
            "marked as blocked by EXP-3"
        );

        // Only another event type renders nothing.
        assert!(event_phrase("status_changed", &payload).is_none());

        // Degrade paths, byte-identical to the web:
        // a missing/blank identifier is named "an issue"...
        assert_eq!(
            event_phrase("relation_added", &json!({ "type": "blocks" })).unwrap(),
            "marked as blocks an issue"
        );
        assert_eq!(
            event_phrase(
                "relation_added",
                &json!({ "type": "blocks", "relatedIdentifier": "" })
            )
            .unwrap(),
            "marked as blocks an issue"
        );
        assert_eq!(
            event_phrase(
                "relation_added",
                &json!({ "type": "blocks", "direction": "inverse" })
            )
            .unwrap(),
            "marked as blocked by an issue"
        );
        // ...and an unknown or missing type reads as the symmetric `related`,
        // never the "linked to" row-caption fallback.
        assert_eq!(
            event_phrase(
                "relation_added",
                &json!({ "type": "something_new", "relatedIdentifier": "EXP-3" })
            )
            .unwrap(),
            "added related issue EXP-3"
        );
        assert_eq!(
            event_phrase(
                "relation_removed",
                &json!({ "type": "something_new", "relatedIdentifier": "EXP-3" })
            )
            .unwrap(),
            "removed related issue EXP-3"
        );
        assert_eq!(
            event_phrase("relation_added", &json!({ "relatedIdentifier": "EXP-3" })).unwrap(),
            "added related issue EXP-3"
        );
        // The web's `relationEventPhrase('relation_added', {})`.
        assert_eq!(
            event_phrase("relation_added", &json!({})).unwrap(),
            "added related issue an issue"
        );
    }
}
