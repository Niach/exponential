//! #FEED-232: ONE count badge on every nav destination, never a dot.
//! Byte-locked ×4 by `packages/domain-contract/fixtures/nav-badges.json`
//! (its `$comment` is the spec; `src/nav-badges.ts` the TS reference).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::rows::SessionReadRow;
use crate::session_tree::ChatsListGroup;

/// Past this the badge reads `99+` (the fixture's `max`).
pub const COUNT_BADGE_MAX: i64 = 99;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CountBadgeTone {
    /// Something waits on the viewer: the needs-you yellow.
    Attention,
    /// Plain unread: muted fill.
    #[default]
    Muted,
}

/// A destination's badge: `count` 0 = no badge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountBadgeValue {
    pub count: usize,
    pub tone: CountBadgeTone,
}

/// What the badge prints (`labelCases`): `""` (nothing renders) at zero or
/// less, `99+` past the cap.
pub fn count_badge_label(count: i64) -> String {
    if count <= 0 {
        String::new()
    } else if count > COUNT_BADGE_MAX {
        format!("{COUNT_BADGE_MAX}+")
    } else {
        count.to_string()
    }
}

/// The Chats destination (`chatsCases`): distinct depth-0 chats that need
/// the viewer or hold an unread reply; an archived root counts only while
/// it needs the viewer. Attention when one of them needs the viewer.
pub fn chats_badge(groups: &[ChatsListGroup], reads: &[&SessionReadRow]) -> CountBadgeValue {
    let archived: HashSet<&str> = reads
        .iter()
        .filter(|read| read.archived_at.as_deref().is_some_and(|at| !at.is_empty()))
        .map(|read| read.session_id.as_str())
        .collect();
    let mut counted: HashSet<&str> = HashSet::new();
    let mut attention = false;
    for row in groups.iter().flat_map(|group| &group.rows) {
        if row.depth != 0 || !(row.needs_you || (row.unread && !archived.contains(row.session_id.as_str()))) {
            continue;
        }
        counted.insert(row.session_id.as_str());
        attention |= row.needs_you;
    }
    CountBadgeValue { count: counted.len(), tone: if attention { CountBadgeTone::Attention } else { CountBadgeTone::Muted } }
}

/// An agent row, an app row or a chat-list row (`rowCases`): its unread
/// agent messages, at least 1 while it waits on the viewer.
pub fn row_badge(unread_count: usize, needs_you: bool) -> CountBadgeValue {
    if needs_you {
        CountBadgeValue { count: unread_count.max(1), tone: CountBadgeTone::Attention }
    } else {
        CountBadgeValue { count: unread_count, tone: CountBadgeTone::Muted }
    }
}

/// The OS app icon (`appIconCases`): every team's needs-you rows summed (a
/// negative never subtracts).
pub fn app_icon_badge(needs_you_per_team: impl IntoIterator<Item = i64>) -> usize {
    needs_you_per_team.into_iter().map(|n| n.max(0) as usize).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_tree::{ChatsListKey, ChatsListRow};
    use serde_json::Value;

    const NAV_BADGES_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/nav-badges.json");

    fn fixture() -> Value {
        serde_json::from_str(NAV_BADGES_JSON).unwrap()
    }

    fn expected(value: &Value) -> CountBadgeValue {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn the_cap_and_geometry_match_the_fixture() {
        let f = fixture();
        assert_eq!(f["max"].as_i64(), Some(COUNT_BADGE_MAX));
        assert_eq!(f["tones"]["attention"]["fill"], "#FACC15");
        assert_eq!(f["geometry"]["height"], 16);
    }

    #[test]
    fn label_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["labelCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(count_badge_label(case["count"].as_i64().unwrap()), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn chats_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["chatsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            // The fixture's band keys are free-form; the count reads only rows.
            let groups: Vec<ChatsListGroup> = input["groups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|group| ChatsListGroup {
                    key: match group["key"].as_str() {
                        Some("needs_you") => ChatsListKey::NeedsYou,
                        Some("recent") => ChatsListKey::Recent,
                        _ => ChatsListKey::Running,
                    },
                    title: group["title"].as_str().unwrap_or_default().into(),
                    rows: serde_json::from_value::<Vec<ChatsListRow>>(group["rows"].clone()).unwrap(),
                })
                .collect();
            let reads: Vec<SessionReadRow> = input["reads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|read| SessionReadRow {
                    session_id: read["sessionId"].as_str().unwrap().into(),
                    read_at: read["readAt"].as_str().map(str::to_string),
                    archived_at: read["archivedAt"].as_str().map(str::to_string),
                    ..SessionReadRow::default()
                })
                .collect();
            let refs: Vec<&SessionReadRow> = reads.iter().collect();
            assert_eq!(chats_badge(&groups, &refs), expected(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn row_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["rowCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let badge = row_badge(input["unreadCount"].as_u64().unwrap() as usize, input["needsYou"].as_bool().unwrap());
            assert_eq!(badge, expected(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn app_icon_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["appIconCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let teams = case["teams"].as_array().unwrap().iter().map(|team| team["needsYou"].as_i64().unwrap());
            assert_eq!(app_icon_badge(teams) as u64, case["expected"].as_u64().unwrap(), "{}", case["name"]);
        }
    }
}
