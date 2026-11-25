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

/// The gone answers of an id so far (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoneAnswers {
    pub first: i64,
    pub last: i64,
}

/// What a pass decided ([`pass`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct HeldPass {
    pub remove: Vec<String>,
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

/// What one ask decides. `gone` = `None`: the ask failed, was refused or did
/// not parse: nothing changes. `notices` = the deletes announced BEFORE it.
pub fn pass(
    now: i64,
    holds: &[String],
    asked: &[String],
    gone: Option<&[String]>,
    state: &BTreeMap<String, GoneAnswers>,
    notices: &BTreeSet<String>,
) -> HeldPass {
    let Some(gone) = gone else {
        return HeldPass { state: state.clone(), notices: notices.clone(), ..Default::default() };
    };
    let held: HashSet<&String> = holds.iter().collect();
    let gone: HashSet<&String> = gone.iter().collect();
    let mut state: BTreeMap<String, GoneAnswers> =
        state.iter().filter(|(id, _)| held.contains(id)).map(|(id, answers)| (id.clone(), *answers)).collect();
    let mut left: BTreeSet<String> = notices.iter().filter(|id| held.contains(id)).cloned().collect();
    let mut explicit = Vec::new();
    let mut due = Vec::new();
    for id in asked.iter().filter(|id| held.contains(id)) {
        if !gone.contains(id) {
            state.remove(id);
            left.remove(id);
            continue;
        }
        if notices.contains(id) {
            explicit.push(id.clone());
            continue;
        }
        let answers = match state.get(id) {
            Some(answers) if now >= answers.last => GoneAnswers { first: answers.first, last: now },
            _ => GoneAnswers { first: now, last: now },
        };
        state.insert(id.clone(), answers);
        if now - answers.first >= CONFIRM_HOURS * 3_600_000 {
            due.push(id.clone());
        }
    }
    let capped = due.len() > CAP_COUNT && due.len() * 2 > holds.len();
    let mut remove = if capped { Vec::new() } else { due };
    remove.extend(explicit);
    HeldPass { remove, state, notices: left, capped }
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
            let gone: Option<Vec<String>> = from(&input["gone"]);
            let got = pass(
                input["now"].as_i64().unwrap(),
                &from::<Vec<String>>(&input["holds"]),
                &from::<Vec<String>>(&input["asked"]),
                gone.as_deref(),
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
