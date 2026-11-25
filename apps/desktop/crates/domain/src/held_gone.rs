//! #EXP-2138: deleting a chat, its agent or its team removes what this device
//! holds of it. The pure half: what is asked for, what an answer does, what
//! goes. Locked by `fixtures/held-gone.json` (TS `src/held-gone.ts`); the host
//! half (asking, files) is `steer::held_gone`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

/// `limits` of the fixture.
pub const BATCH: usize = 500;
pub const ASK_EVERY_HOURS: u64 = 6;
pub const CONFIRM_HOURS: i64 = 24;
pub const CAP_COUNT: usize = 20;
/// `reason` of the kill a delete sends.
pub const KILL_REASON_DELETED: &str = "deleted";

/// One run record, as the rules read it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct HeldRecord {
    pub id: String,
    pub account: String,
    pub aside: bool,
    #[serde(default)]
    pub cwd: String,
    /// `cwd` is a repo-less run's scratch dir.
    #[serde(default)]
    pub scratch: bool,
}

/// What the server says of an id that is not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoneKind {
    /// The row does not exist: deleted.
    Gone,
    /// It exists and this account may not have it.
    Unreadable,
}

/// `sessions.heldGone`'s answer; both fields are required.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct HeldAnswer {
    pub gone: Vec<String>,
    pub unreadable: Vec<String>,
}

/// The same answers of an id so far (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoneAnswers {
    pub kind: GoneKind,
    pub first: i64,
    pub last: i64,
}

/// What a pass decided ([`pass`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct HeldPass {
    /// Deleted: everything goes.
    pub remove: Vec<String>,
    /// Not readable any more: only the cached rows go.
    pub uncache: Vec<String>,
    pub state: BTreeMap<String, GoneAnswers>,
    pub notices: BTreeSet<String>,
    pub capped: bool,
}

/// What of one id goes ([`removal`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct HeldRemoval {
    pub record: bool,
    pub journal: bool,
    pub scratch: bool,
    pub store: bool,
}

fn is_uuid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12]) && groups.iter().all(|group| group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The ids `account` asks for: its run records and its chat store.
pub fn candidates(account: &str, records: &[HeldRecord], store: &[String]) -> Vec<String> {
    let own = records.iter().filter(|record| record.account == account && !record.aside).map(|record| &record.id);
    let ids: BTreeSet<&String> = own.chain(store).filter(|id| is_uuid(id)).collect();
    ids.into_iter().cloned().collect()
}

/// What one ask decides. `answer` = `None`: the ask failed, was refused or
/// did not parse: nothing changes. `cached` = the ids the chat store names;
/// `notices` = the deletes announced BEFORE the ask.
pub fn pass(
    now: i64,
    holds: &[String],
    cached: &[String],
    asked: &[String],
    answer: Option<&HeldAnswer>,
    state: &BTreeMap<String, GoneAnswers>,
    notices: &BTreeSet<String>,
) -> HeldPass {
    let Some(answer) = answer else {
        return HeldPass { state: state.clone(), notices: notices.clone(), ..Default::default() };
    };
    let held: HashSet<&String> = holds.iter().collect();
    let cached: HashSet<&String> = cached.iter().collect();
    let gone: HashSet<&String> = answer.gone.iter().collect();
    let unreadable: HashSet<&String> = answer.unreadable.iter().collect();
    let mut state: BTreeMap<String, GoneAnswers> =
        state.iter().filter(|(id, _)| held.contains(id)).map(|(id, answers)| (id.clone(), *answers)).collect();
    let mut left: BTreeSet<String> = notices.iter().filter(|id| held.contains(id)).cloned().collect();
    let mut explicit = Vec::new();
    let mut remove = Vec::new();
    let mut uncache = Vec::new();
    for id in asked.iter().filter(|id| held.contains(id)) {
        let kind = if unreadable.contains(id) {
            Some(GoneKind::Unreadable)
        } else {
            gone.contains(id).then_some(GoneKind::Gone)
        };
        if kind != Some(GoneKind::Gone) {
            left.remove(id);
        }
        // Nothing of an unreadable chat is cached: nothing to wait for.
        let Some(kind) = kind.filter(|kind| *kind == GoneKind::Gone || cached.contains(id)) else {
            state.remove(id);
            continue;
        };
        if kind == GoneKind::Gone && notices.contains(id) {
            explicit.push(id.clone());
            continue;
        }
        let first = match state.get(id) {
            Some(answers) if answers.kind == kind && now >= answers.last => answers.first,
            _ => now,
        };
        state.insert(id.clone(), GoneAnswers { kind, first, last: now });
        if now - first >= CONFIRM_HOURS * 3_600_000 {
            match kind {
                GoneKind::Gone => remove.push(id.clone()),
                GoneKind::Unreadable => uncache.push(id.clone()),
            }
        }
    }
    let due = remove.len() + uncache.len();
    let capped = due > CAP_COUNT && due * 2 > holds.len();
    if capped {
        remove.clear();
        uncache.clear();
    }
    remove.extend(explicit);
    HeldPass { remove, uncache, state, notices: left, capped }
}

/// What of `id` goes: `store` / `other_store` = this / another account's chat
/// store on the machine names it.
pub fn removal(account: &str, id: &str, records: &[HeldRecord], store: bool, other_store: bool) -> HeldRemoval {
    let record = records.iter().find(|record| record.id == id);
    let own = record.filter(|record| record.account == account && !record.aside);
    HeldRemoval {
        record: own.is_some(),
        journal: own.is_some() || (record.is_none() && store && !other_store),
        scratch: own.is_some_and(|own| own.scratch && !records.iter().any(|other| other.id != id && other.cwd == own.cwd)),
        store,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/held-gone.json");

    fn from<T: serde::de::DeserializeOwned>(value: &Value) -> T {
        serde_json::from_value(value.clone()).unwrap()
    }

    #[test]
    fn replays_held_gone_json() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let limits = &fixture["limits"];
        assert_eq!(
            (limits["batch"].as_u64(), limits["askEveryHours"].as_u64(), limits["confirmHours"].as_i64(), limits["capCount"].as_u64()),
            (Some(BATCH as u64), Some(ASK_EVERY_HOURS), Some(CONFIRM_HOURS), Some(CAP_COUNT as u64))
        );
        assert_eq!(limits["killReason"], KILL_REASON_DELETED);
        for case in fixture["candidateCases"].as_array().unwrap() {
            let records: Vec<HeldRecord> = from(&case["records"]);
            let store: Vec<String> = from(&case["store"]);
            let got = candidates(case["account"].as_str().unwrap(), &records, &store);
            assert_eq!(got, from::<Vec<String>>(&case["expected"]), "{}", case["name"]);
        }
        for case in fixture["passCases"].as_array().unwrap() {
            let input = &case["input"];
            let answer: Option<HeldAnswer> = from(&input["answer"]);
            let got = pass(
                input["now"].as_i64().unwrap(),
                &from::<Vec<String>>(&input["holds"]),
                &from::<Vec<String>>(&input["cached"]),
                &from::<Vec<String>>(&input["asked"]),
                answer.as_ref(),
                &from(&input["state"]),
                &from(&input["notices"]),
            );
            assert_eq!(got, from::<HeldPass>(&case["expected"]), "{}", case["name"]);
        }
        for case in fixture["removeCases"].as_array().unwrap() {
            let records: Vec<HeldRecord> = from(&case["records"]);
            let got = removal(
                case["account"].as_str().unwrap(),
                case["id"].as_str().unwrap(),
                &records,
                case["store"].as_bool().unwrap(),
                case["otherStores"].as_bool().unwrap(),
            );
            assert_eq!(got, from::<HeldRemoval>(&case["expected"]), "{}", case["name"]);
        }
    }
}
