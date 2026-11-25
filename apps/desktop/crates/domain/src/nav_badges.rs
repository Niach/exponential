//! #FEED-232: ONE count badge on every nav destination, never a dot.
//! Byte-locked ×4 by `packages/domain-contract/fixtures/nav-badges.json`
//! (its `$comment` is the spec; `src/nav-badges.ts` the TS reference).

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::rows::SessionReadRow;
use crate::session_tree::ChatsListGroup;

const NAV_BADGES_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/nav-badges.json");

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

/// A parent's badge over the rows it hides (`sumCases`): a folded folder.
/// Attention while a counted row needs the viewer.
pub fn sum_badges(rows: impl IntoIterator<Item = CountBadgeValue>) -> CountBadgeValue {
    let mut out = CountBadgeValue::default();
    for row in rows.into_iter().filter(|row| row.count > 0) {
        out.count += row.count;
        if row.tone == CountBadgeTone::Attention {
            out.tone = CountBadgeTone::Attention;
        }
    }
    out
}

/// A team row (`teamCases`): its share of Needs you, and while folded its
/// hidden rows' muted counts (the attention ones are in the share).
pub fn team_badge(needs_you: usize, open: bool, rows: impl IntoIterator<Item = CountBadgeValue>) -> CountBadgeValue {
    let share = CountBadgeValue { count: needs_you, tone: CountBadgeTone::Attention };
    let hidden = rows.into_iter().filter(|row| !open && row.tone == CountBadgeTone::Muted);
    sum_badges(std::iter::once(share).chain(hidden))
}

/// [`needs_you_counts`]' answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeedsYouCounts {
    pub total: usize,
    pub teams: BTreeMap<String, usize>,
}

/// Needs you (`needsYouCases`): `rows` = THE list over every member team and
/// the Assistant's home, each its session's team; a row outside `teams` (the
/// Assistant's) and the waiting installs belong to no team.
pub fn needs_you_counts<'a>(rows: impl IntoIterator<Item = Option<&'a str>>, teams: impl IntoIterator<Item = &'a str>, installs: usize) -> NeedsYouCounts {
    let mut out = NeedsYouCounts { total: installs, teams: teams.into_iter().map(|team| (team.to_string(), 0)).collect() };
    for team in rows {
        out.total += 1;
        if let Some(count) = team.and_then(|team| out.teams.get_mut(team)) {
            *count += 1;
        }
    }
    out
}

/// The OS app icon (`appIconCases`): every signed-in account's Needs you
/// total (a negative never subtracts).
pub fn app_icon_badge(needs_you_per_account: impl IntoIterator<Item = i64>) -> usize {
    needs_you_per_account.into_iter().map(|n| n.max(0) as usize).sum()
}

/// A row's team mark on its lead (`teamMark.sizes`): the mark's side, the
/// band of ground around it, how far it hangs past the lead's corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct TeamMarkGeometry {
    pub mark: u32,
    pub ring: u32,
    pub offset: u32,
}

#[derive(Deserialize)]
struct TeamMarkSize {
    lead: u32,
    #[serde(flatten)]
    geometry: TeamMarkGeometry,
}

static TEAM_MARK_SIZES: LazyLock<Vec<TeamMarkSize>> = LazyLock::new(|| {
    let fixture: serde_json::Value = serde_json::from_str(NAV_BADGES_JSON).expect("nav-badges.json parses");
    serde_json::from_value(fixture["teamMark"]["sizes"].clone()).expect("nav-badges.json teamMark.sizes")
});

/// `teamMark.leadCases`: the first row whose `lead` is at most the lead's side.
pub fn team_mark_of(lead: f32) -> TeamMarkGeometry {
    let sizes = &*TEAM_MARK_SIZES;
    sizes.iter().find(|row| row.lead as f32 <= lead).or(sizes.last()).expect("teamMark.sizes").geometry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_tree::{ChatsListKey, ChatsListRow};
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(NAV_BADGES_JSON).unwrap()
    }

    fn expected(value: &Value) -> CountBadgeValue {
        serde_json::from_value(value.clone()).unwrap()
    }

    /// The fixture's badges (a negative count is none: `count` is unsigned).
    fn badges(value: &Value) -> Vec<CountBadgeValue> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| CountBadgeValue { count: row["count"].as_i64().unwrap().max(0) as usize, tone: serde_json::from_value(row["tone"].clone()).unwrap() })
            .collect()
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
    fn sum_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["sumCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(sum_badges(badges(&case["input"])), expected(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn team_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["teamCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let badge = team_badge(input["needsYou"].as_u64().unwrap() as usize, input["open"].as_bool().unwrap(), badges(&input["rows"]));
            assert_eq!(badge, expected(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn needs_you_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["needsYouCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let counts = needs_you_counts(
                input["rows"].as_array().unwrap().iter().map(|row| row["teamId"].as_str()),
                input["teams"].as_array().unwrap().iter().map(|team| team.as_str().unwrap()),
                input["installs"].as_u64().unwrap() as usize,
            );
            assert_eq!(serde_json::to_value(&counts).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn app_icon_cases_match_the_fixture() {
        let f = fixture();
        let cases = f["appIconCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let accounts = case["accounts"].as_array().unwrap().iter().map(|account| account["needsYou"].as_i64().unwrap());
            assert_eq!(app_icon_badge(accounts) as u64, case["expected"].as_u64().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn team_mark_lead_cases_match_the_fixture() {
        let f = fixture();
        assert_eq!(f["teamMark"]["corner"], "bottomRight");
        let cases = f["teamMark"]["leadCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let want: TeamMarkGeometry = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(team_mark_of(case["lead"].as_f64().unwrap() as f32), want, "{}", case["lead"]);
        }
    }

    /// Every destination of the table, `badge: false` ones included: a new
    /// one must be wired here first.
    #[test]
    fn the_destinations_are_the_known_ones() {
        let f = fixture();
        let ids = |badge: bool| -> Vec<&str> {
            f["destinations"].as_array().unwrap().iter().filter(|d| d["badge"].as_bool() == Some(badge)).map(|d| d["id"].as_str().unwrap()).collect()
        };
        assert_eq!(ids(true), ["assistant", "needs-you", "team", "folder", "agent", "session", "teams-tab", "chats", "chat-row", "inbox", "drafts", "app-icon"]);
        assert_eq!(ids(false), ["home", "devices", "device", "board", "app", "new", "settings", "search", "work-tab", "active-row"]);
    }
}
