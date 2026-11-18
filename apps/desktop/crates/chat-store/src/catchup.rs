//! #EDE-108 S2 BACKGROUND CATCH-UP (`catchUp` in
//! `packages/domain-contract/fixtures/chat-store.json`): the pure rules every
//! client replays. A reader walks, per (session, device the session is on),
//! the events it lacks from the runner or another reader device, over eNet
//! (`steer::enet_catchup` carries it); this module decides what to ask, what
//! a holder answers and how far the asker is complete. No I/O, no clock.

use serde::{Deserialize, Serialize};

/// The fixture's `catchUp.constants`.
pub const PAGE_MAX: usize = 200;
pub const HAVE_RUNS_MAX: usize = 64;
pub const CONCURRENCY: usize = 2;
pub const BYTES_PER_SECOND: i64 = 1_048_576;
pub const BURST_BYTES: i64 = 4_194_304;
pub const ASK_TIMEOUT_MS: u64 = 15_000;
pub const PASS_INTERVAL_MS: u64 = 300_000;
pub const RETRY_MAX_MS: u64 = 21_600_000;
pub const ENDPOINT_BACKOFF_MS: u64 = 300_000;
pub const IDLE_AFTER_MS: u64 = 300_000;

/// A run of consecutive seqs, both ends included.
pub type SeqRun = [u64; 2];

/// `have`: the runs of `seqs` (ascending) above `after_seq` (all when
/// `None`), the lowest `max`.
pub fn have_runs(seqs: impl IntoIterator<Item = u64>, after_seq: Option<u64>, max: usize) -> Vec<SeqRun> {
    let mut runs: Vec<SeqRun> = Vec::new();
    for seq in seqs {
        if after_seq.is_some_and(|after| seq <= after) {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last[1] + 1 == seq => last[1] = seq,
            Some(last) if last[1] >= seq => {}
            _ => {
                if runs.len() == max {
                    break;
                }
                runs.push([seq, seq]);
            }
        }
    }
    runs
}

/// The fields of a device-naming `history_page` that shape its answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    #[serde(default)]
    pub after_seq: Option<u64>,
    pub limit: usize,
    #[serde(default)]
    pub have: Vec<SeqRun>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_at: Option<i64>,
}

impl Ask {
    pub fn limit(&self) -> usize {
        self.limit.clamp(1, PAGE_MAX)
    }

    fn held(&self, seq: u64) -> bool {
        self.have.iter().any(|[from, to]| *from <= seq && seq <= *to)
    }

    /// Whether the answer skips an event of `kind` at `at` (RETENTION on the
    /// asker: prose always comes).
    pub fn too_old(&self, kind: &str, at: i64) -> bool {
        self.min_at.is_some_and(|min| !is_prose(kind) && at < min)
    }

    /// Whether the scan takes the event at `seq`.
    pub fn takes(&self, seq: u64, kind: &str, at: i64) -> bool {
        self.after_seq.is_none_or(|after| seq > after) && !self.held(seq) && !self.too_old(kind, at)
    }
}

fn is_prose(kind: &str) -> bool {
    matches!(kind, "user_message" | "narration")
}

/// A holder's page (its events travel beside it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub seqs: Vec<u64>,
    pub through_seq: Option<u64>,
    pub done: bool,
    pub last_seq: Option<u64>,
    pub full_to: Option<u64>,
}

/// One held event as the answer reads it: (seq, kind, at).
pub type Held<'a> = (u64, &'a str, i64);

/// THE HOLDER's page over its events in seq order: the first `limit` above
/// `after_seq` the asker lacks; `full_to` = how far the holder is complete
/// (a journal: its last seq; a store: its own watermark).
pub fn answer<'a>(events: impl IntoIterator<Item = Held<'a>>, full_to: Option<u64>, ask: &Ask) -> Answer {
    let limit = ask.limit();
    let mut seqs = Vec::new();
    let mut last_seq = None;
    for (seq, kind, at) in events {
        last_seq = Some(seq);
        if seqs.len() < limit && ask.takes(seq, kind, at) {
            seqs.push(seq);
        }
    }
    finish(seqs, limit, last_seq, full_to, ask)
}

/// The answer's ends once its page (`seqs`, at most `limit`) and the holder's
/// last seq are known (the SQLite adapter reads them by query).
pub fn finish(seqs: Vec<u64>, limit: usize, last_seq: Option<u64>, full_to: Option<u64>, ask: &Ask) -> Answer {
    let through_seq = if seqs.len() == limit { seqs.last().copied() } else { ask.after_seq.max(last_seq) };
    let done = match (last_seq, through_seq) {
        (None, _) => true,
        (Some(last), Some(through)) => through >= last,
        (Some(_), None) => false,
    };
    Answer { seqs, through_seq, done, last_seq, full_to }
}

/// `a < b`, `None` below every seq.
fn below(a: Option<u64>, b: Option<u64>) -> bool {
    a.map_or(-1, |a| a as i128) < b.map_or(-1, |b| b as i128)
}

/// THE ASKER's PROGRESS after one answer to an ask from `after_seq`: the new
/// `completeTo` and the next ask's `afterSeq` (`None` = this holder's walk ends).
pub fn progress(
    complete_to: Option<u64>,
    after_seq: Option<u64>,
    through_seq: Option<u64>,
    done: bool,
    full_to: Option<u64>,
) -> (Option<u64>, Option<Option<u64>>) {
    let reach = through_seq.zip(full_to).map(|(through, full)| through.min(full));
    let mut complete = complete_to;
    if !below(complete_to, after_seq) && reach.is_some() && below(complete_to, reach) {
        complete = reach;
    }
    (complete, (!done).then_some(through_seq))
}

/// Whether a RUNNER's walk that ended at `complete_to` caught the session up.
pub fn caught_up(complete_to: Option<u64>, last_seq: Option<u64>) -> bool {
    last_seq.is_none() || !below(complete_to, last_seq)
}

/// One install the registry lists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub endpoint_id: String,
    pub user_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub last_seen_at: Option<String>,
}

/// One holder to ask, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
    pub endpoint_id: String,
    pub runner: bool,
}

/// HOLDERS: the installs of the session's device (newest seen first), then
/// the asker's own other installs; never itself, never a teammate's device,
/// never an endpoint in its backoff (`skip`).
pub fn holders(installs: &[Install], device_id: &str, self_endpoint: &str, user_id: &str, skip: &[String]) -> Vec<Holder> {
    let askable: Vec<&Install> =
        installs.iter().filter(|i| i.endpoint_id != self_endpoint && !skip.contains(&i.endpoint_id)).collect();
    let newest = |a: &&Install, b: &&Install| {
        b.last_seen_at.as_deref().unwrap_or_default().cmp(a.last_seen_at.as_deref().unwrap_or_default())
    };
    let mut runner: Vec<&Install> = askable.iter().copied().filter(|i| i.device_id.as_deref() == Some(device_id)).collect();
    let mut own: Vec<&Install> = askable
        .iter()
        .copied()
        .filter(|i| i.device_id.as_deref() != Some(device_id) && i.user_id == user_id)
        .collect();
    runner.sort_by(newest);
    own.sort_by(newest);
    runner
        .into_iter()
        .map(|i| Holder { endpoint_id: i.endpoint_id.clone(), runner: true })
        .chain(own.into_iter().map(|i| Holder { endpoint_id: i.endpoint_id.clone(), runner: false }))
        .collect()
}

/// What the platform says about its power.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    Ac,
    Battery,
    Unknown,
}

/// POWER: on battery a pass runs only once the person is idle.
pub fn may_run(power: Power, idle_ms: u64) -> bool {
    power != Power::Battery || idle_ms >= IDLE_AFTER_MS
}

/// A session served by readers alone is tried again after this long.
pub fn retry_ms(attempts: u32) -> u64 {
    PASS_INTERVAL_MS.saturating_mul(1u64.checked_shl(attempts).unwrap_or(u64::MAX)).min(RETRY_MAX_MS)
}

/// The TOKEN BUCKET (`bytesPerSecond`, capacity `burstBytes`, starting full).
#[derive(Clone, Debug)]
pub struct Bucket {
    tokens: i64,
    at_ms: i64,
}

impl Bucket {
    pub fn new(now_ms: i64) -> Bucket {
        Bucket { tokens: BURST_BYTES, at_ms: now_ms }
    }

    fn refill(&mut self, now_ms: i64) {
        let elapsed = (now_ms - self.at_ms).max(0);
        self.tokens = BURST_BYTES.min(self.tokens.saturating_add(elapsed.saturating_mul(BYTES_PER_SECOND) / 1000));
        self.at_ms = now_ms.max(self.at_ms);
    }

    /// An answer's bytes arrived.
    pub fn spend(&mut self, now_ms: i64, bytes: usize) {
        self.refill(now_ms);
        self.tokens -= bytes as i64;
    }

    /// Ms until the next ask may go.
    pub fn wait_ms(&mut self, now_ms: i64) -> u64 {
        self.refill(now_ms);
        if self.tokens >= 0 {
            0
        } else {
            ((-self.tokens) as u64 * 1000).div_ceil(BYTES_PER_SECOND as u64)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fixture::{from, spec};
    use serde_json::{json, Value};

    fn catch_up() -> Value {
        spec()["catchUp"].clone()
    }

    #[test]
    fn constants_match_the_contract() {
        let c = &catch_up()["constants"];
        let got = json!({
            "pageMax": PAGE_MAX, "haveRunsMax": HAVE_RUNS_MAX, "concurrency": CONCURRENCY,
            "bytesPerSecond": BYTES_PER_SECOND, "burstBytes": BURST_BYTES, "askTimeoutMs": ASK_TIMEOUT_MS,
            "passIntervalMs": PASS_INTERVAL_MS, "retryMaxMs": RETRY_MAX_MS, "endpointBackoffMs": ENDPOINT_BACKOFF_MS,
            "idleAfterMs": IDLE_AFTER_MS,
        });
        assert_eq!(&got, c);
        assert_eq!(PAGE_MAX as u32, 200, "pageMax = steerFeed.historyPageMax");
    }

    #[test]
    fn have_runs_cases() {
        for case in catch_up()["haveRunsCases"].as_array().unwrap() {
            let mut seqs: Vec<u64> = from(&case["seqs"]);
            seqs.sort_unstable();
            let got = have_runs(seqs, from(&case["afterSeq"]), from(&case["max"]));
            assert_eq!(json!(got), case["expect"], "{}", case["name"]);
        }
    }

    /// The case's held events: `events` or `eventRange` (narration at 0).
    pub(crate) fn case_events(case: &Value) -> Vec<(u64, String, i64)> {
        match case.get("events") {
            Some(events) => events
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (e["seq"].as_u64().unwrap(), e["kind"].as_str().unwrap().to_string(), e["at"].as_i64().unwrap()))
                .collect(),
            None => {
                let [first, last]: [u64; 2] = from(&case["eventRange"]);
                (first..=last).map(|seq| (seq, "narration".to_string(), 0)).collect()
            }
        }
    }

    #[test]
    fn answer_cases() {
        for case in catch_up()["answerCases"].as_array().unwrap() {
            let mut events = case_events(case);
            events.sort_by_key(|e| e.0);
            let ask: Ask = from(&case["ask"]);
            let got = answer(events.iter().map(|(s, k, a)| (*s, k.as_str(), *a)), from(&case["fullTo"]), &ask);
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn progress_cases() {
        for case in catch_up()["progressCases"].as_array().unwrap() {
            let a = &case["answer"];
            let (complete, next) = progress(
                from(&case["completeTo"]),
                from(&case["afterSeq"]),
                from(&a["throughSeq"]),
                a["done"].as_bool().unwrap(),
                from(&a["fullTo"]),
            );
            let got = json!({ "completeTo": complete, "nextAfter": next.flatten() });
            assert_eq!(got, case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn walk_cases() {
        for case in catch_up()["walkCases"].as_array().unwrap() {
            let mut seqs: std::collections::BTreeSet<u64> = from(&case["asker"]["seqs"]);
            let mut complete: Option<u64> = from(&case["asker"]["completeTo"]);
            let limit: usize = from(&case["limit"]);
            let mut asks = serde_json::Map::new();
            let mut caught = false;
            for holder in case["holders"].as_array().unwrap() {
                if holder.get("reachable") == Some(&json!(false)) {
                    continue;
                }
                let held: Vec<u64> = from(&holder["seqs"]);
                let full_to: Option<u64> = from(&holder["fullTo"]);
                let name = holder["name"].as_str().unwrap().to_string();
                let mut cursor = complete;
                let last = loop {
                    let ask = Ask { after_seq: cursor, limit, have: have_runs(seqs.iter().copied(), cursor, HAVE_RUNS_MAX), min_at: None };
                    let got = answer(held.iter().map(|seq| (*seq, "narration", 0)), full_to, &ask);
                    *asks.entry(name.clone()).or_insert(json!(0)) = json!(asks.get(&name).and_then(Value::as_u64).unwrap_or(0) + 1);
                    seqs.extend(got.seqs.iter().copied());
                    let (next_complete, next) = progress(complete, cursor, got.through_seq, got.done, got.full_to);
                    complete = next_complete;
                    match next {
                        Some(after) => cursor = after,
                        None => break got.last_seq,
                    }
                };
                if holder.get("runner") == Some(&json!(true)) && caught_up(complete, last) {
                    caught = true;
                    break;
                }
            }
            let got = json!({ "seqs": seqs, "completeTo": complete, "asks": asks, "caughtUp": caught });
            assert_eq!(got, case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn holder_cases() {
        for case in catch_up()["holderCases"].as_array().unwrap() {
            let installs: Vec<Install> = from(&case["installs"]);
            let skip: Vec<String> = from(&case["skip"]);
            let got = holders(
                &installs,
                case["deviceId"].as_str().unwrap(),
                case["selfEndpoint"].as_str().unwrap(),
                case["userId"].as_str().unwrap(),
                &skip,
            );
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn gate_retry_and_bucket_cases() {
        let c = catch_up();
        for case in c["gateCases"].as_array().unwrap() {
            assert_eq!(json!(may_run(from(&case["power"]), from(&case["idleMs"]))), case["run"], "{}", case["name"]);
        }
        for case in c["retryCases"].as_array().unwrap() {
            assert_eq!(json!(retry_ms(from(&case["attempts"]))), case["ms"]);
        }
        for case in c["bucketCases"].as_array().unwrap() {
            let mut bucket = Bucket::new(0);
            let mut waits = Vec::new();
            for op in case["ops"].as_array().unwrap() {
                let at = op["at"].as_i64().unwrap();
                match op.get("spend") {
                    Some(bytes) => bucket.spend(at, bytes.as_u64().unwrap() as usize),
                    None => waits.push(bucket.wait_ms(at)),
                }
            }
            assert_eq!(json!(waits), case["waits"], "{}", case["name"]);
        }
    }
}
