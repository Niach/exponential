//! #EXP-1950: the attacks of install-trust.json `maliciousServer` against the
//! REAL key holder and keeper, with ONE lying server double ([`Liar`]: it
//! lies in `sessionKeys.mine`, `pending`, `publicKeys`, `sessions.sendTarget`
//! and `installs.trust`), in both modes; decision (e); the negatives.

use super::*;
use crate::seal_keys::{KeyApi, KeyHolder};
use api::session_keys::{CreateKey, HeldKey, PairWrap, PendingWrap, RootKey};
use coding::sealing::{Keyring, SealService as _};
use enet_core::install_trust::{ServedApproval, TargetRefusal};
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/install-trust.json")).unwrap()
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn hex32(v: &Value) -> [u8; 32] {
    data_encoding::HEXLOWER.decode(text(v).as_bytes()).unwrap().try_into().unwrap()
}
fn key<'a>(fx: &'a Value, name: &str) -> &'a Value {
    &fx["keys"][name]
}
fn endpoint(fx: &Value, name: &str) -> String {
    text(&key(fx, name)["endpointId"]).to_string()
}
fn identity(fx: &Value, name: &str) -> Identity {
    Identity::from_bytes(hex32(&key(fx, name)["identity"]))
}

/// A served row of key `name` (as install-trust.test.ts `served`).
fn served(fx: &Value, name: &str, registration: Option<&str>, asked: bool) -> ServedInstall {
    let k = key(fx, name);
    ServedInstall {
        install_id: format!("install-{name}"),
        user_id: text(&k["userId"]).into(),
        endpoint_id: text(&k["endpointId"]).into(),
        x25519_public: text(&k["x25519Public"]).into(),
        registration_signature: match registration {
            Some("none") => None,
            Some("forged") => Some(enet_core::install_trust::sign_registration(&identity(fx, "Z"), text(&k["userId"]), text(&k["signedAt"]))),
            _ => Some(text(&k["registrationSignature"]).into()),
        },
        registration_signed_at: (registration != Some("none")).then(|| text(&k["signedAt"]).into()),
        approval_requested: asked,
        label: Some(text(&k["label"]).into()),
    }
}

fn approval(fx: &Value, kind: ApprovalKind, subject: &str, approver: &str) -> ServedApproval {
    let k = key(fx, subject);
    let subject_ref =
        ApprovalSubject { user_id: text(&k["userId"]), endpoint_id: text(&k["endpointId"]), x25519_public: text(&k["x25519Public"]) };
    ServedApproval {
        kind,
        subject_endpoint_id: endpoint(fx, subject),
        approver_endpoint_id: endpoint(fx, approver),
        signature: sign_approval(&identity(fx, approver), kind, &subject_ref).unwrap(),
    }
}

/// THE lying server: an honest base it can be told to lie in, per answer.
#[derive(Default)]
struct Liar {
    installs: Mutex<Vec<ServedInstall>>,
    approvals: Mutex<Vec<ServedApproval>>,
    /// Answered once by `installs.trust` before the live state (a read that
    /// raced another install's claim).
    stale_once: Mutex<Option<TrustAnswer>>,
    /// No `installs.trust` at all (an older server).
    no_trust: bool,
    mine: Mutex<Vec<HeldKey>>,
    pending: Mutex<Vec<PendingWrap>>,
    public_keys: Mutex<Vec<RootKey>>,
    /// `sessions.sendTarget` (the tests hand it to the holder's seal / call paths).
    send_target: Mutex<Option<Value>>,
    wrapped: Mutex<Vec<PairWrap>>,
    claims: Mutex<Vec<(String, Vec<String>)>>,
    approves: Mutex<Vec<(String, String, ApprovalKind)>>,
    requests: Mutex<Vec<String>>,
    revokes: Mutex<Vec<String>>,
}

impl Liar {
    fn endpoint_of(&self, install_id: &str) -> Option<ServedInstall> {
        lock(&self.installs).iter().find(|i| i.install_id == install_id).cloned()
    }
}

impl KeyApi for Liar {
    fn create(&self, _input: &CreateKey) -> Result<(), ApiError> {
        Ok(())
    }
    fn mine(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
        Ok(lock(&self.mine).clone())
    }
    fn pending(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
        Ok(lock(&self.pending).clone())
    }
    fn wrap(&self, _install_id: &str, wraps: &[PairWrap]) -> Result<(), ApiError> {
        lock(&self.wrapped).extend(wraps.iter().cloned());
        Ok(())
    }
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
        Ok(lock(&self.public_keys).iter().filter(|k| root_ids.contains(&k.root_id)).cloned().collect())
    }
}

impl TrustApi for Liar {
    fn trust(&self, _install_id: &str) -> Result<TrustAnswer, ApiError> {
        if self.no_trust {
            return Err(ApiError::Http { status: 404, message: "No procedure found on path \"installs.trust\"".into() });
        }
        if let Some(stale) = lock(&self.stale_once).take() {
            return Ok(stale);
        }
        Ok(TrustAnswer { installs: lock(&self.installs).clone(), approvals: lock(&self.approvals).clone() })
    }
    fn request_approval(&self, install_id: &str) -> Result<(), ApiError> {
        lock(&self.requests).push(install_id.into());
        if let Some(row) = lock(&self.installs).iter_mut().find(|i| i.install_id == install_id) {
            row.approval_requested = true;
        }
        Ok(())
    }
    fn claim_root(&self, install_id: &str, signature: &str, approvals: &[RootClaimApproval]) -> Result<(), ApiError> {
        let me = self.endpoint_of(install_id).unwrap();
        let chained = lock(&self.approvals).iter().any(|a| {
            matches!(a.kind, ApprovalKind::Root | ApprovalKind::Approve)
                && lock(&self.installs).iter().any(|i| i.endpoint_id == a.subject_endpoint_id && i.user_id == me.user_id)
        });
        if chained {
            return Err(ApiError::Http { status: 409, message: "This account already has a trusted install".into() });
        }
        lock(&self.claims).push((install_id.into(), approvals.iter().map(|a| a.subject_install_id.clone()).collect()));
        let mut stored = vec![ServedApproval {
            kind: ApprovalKind::Root,
            subject_endpoint_id: me.endpoint_id.clone(),
            approver_endpoint_id: me.endpoint_id.clone(),
            signature: signature.into(),
        }];
        for a in approvals {
            let subject = self.endpoint_of(&a.subject_install_id).unwrap();
            stored.push(ServedApproval {
                kind: ApprovalKind::Approve,
                subject_endpoint_id: subject.endpoint_id,
                approver_endpoint_id: me.endpoint_id.clone(),
                signature: a.signature.clone(),
            });
        }
        lock(&self.approvals).extend(stored);
        Ok(())
    }
    fn approve(&self, install_id: &str, subject_install_id: &str, kind: ApprovalKind, signature: &str) -> Result<(), ApiError> {
        lock(&self.approves).push((install_id.into(), subject_install_id.into(), kind));
        let (me, subject) = (self.endpoint_of(install_id).unwrap(), self.endpoint_of(subject_install_id).unwrap());
        lock(&self.approvals).push(ServedApproval {
            kind,
            subject_endpoint_id: subject.endpoint_id,
            approver_endpoint_id: me.endpoint_id,
            signature: signature.into(),
        });
        Ok(())
    }
    fn revoke(&self, install_id: &str) -> Result<(), ApiError> {
        lock(&self.revokes).push(install_id.into());
        lock(&self.installs).retain(|i| i.install_id != install_id);
        Ok(())
    }
}

static DIRS: AtomicUsize = AtomicUsize::new(0);

/// A private store path under the system temp dir (never a real data dir).
fn store_file() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exp1950-trust-{}-{}", std::process::id(), DIRS.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("install-pins")
}

/// install-trust.test.ts `buildStore`, for a named store of the fixture.
fn fixture_store(fx: &Value, spec: &Value) -> TrustStore {
    let viewer = key(fx, text(&spec["viewer"]));
    let mut store = TrustStore::empty(text(&viewer["endpointId"]), text(&viewer["userId"]), false);
    store.joined = spec["joined"].as_bool().unwrap();
    for (name, pin) in spec["pins"].as_object().unwrap() {
        let k = key(fx, name);
        store.pins.insert(
            text(&k["endpointId"]).into(),
            enet_core::install_trust::Pin {
                user_id: text(&k["userId"]).into(),
                x25519_public: text(&k["x25519Public"]).into(),
                via: serde_json::from_value(pin[0].clone()).unwrap(),
                by: pin[1].as_str().map(|by| endpoint(fx, by)),
            },
        );
    }
    store.members = spec["members"].as_array().map_or_else(Vec::new, |m| m.iter().map(|u| text(u).to_string()).collect());
    if let Some(roots) = spec["roots"].as_object() {
        for (root, record) in roots {
            store.roots.insert(root.clone(), serde_json::from_value(record.clone()).unwrap());
        }
    }
    store
}

/// The victim of the fixture (`stores.victim`: install A, joined, pinning
/// A, B and T1), its real holder and keeper over the liar, synced once
/// against an honest base (A's chain: root A, approve B; T1's root).
struct Victim {
    liar: Arc<Liar>,
    holder: KeyHolder,
    keeper: Arc<TrustKeeper>,
}

fn victim(mode: TrustMode) -> Victim {
    let fx = fixture();
    let liar = Arc::new(Liar::default());
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "B", None, false), served(&fx, "T1", None, false)];
    *lock(&liar.approvals) = vec![
        approval(&fx, ApprovalKind::Root, "A", "A"),
        approval(&fx, ApprovalKind::Approve, "B", "A"),
        approval(&fx, ApprovalKind::Root, "T1", "T1"),
    ];
    let path = store_file();
    write_store(&path, &fixture_store(&fx, &fx["stores"]["victim"])).unwrap();
    let a = identity(&fx, "A");
    let keeper = Arc::new(TrustKeeper::open(path, &a, "user-me", "install-A".into(), liar.clone(), mode));
    let holder = KeyHolder::signing("install-A".into(), &a, liar.clone(), Keyring::in_memory());
    holder.set_trust(keeper.clone());
    keeper.sync();
    assert_eq!(keeper.snapshot().state, TrustState::Trusted);
    Victim { liar, holder, keeper }
}

const MODES: [(TrustMode, &str); 2] = [(TrustMode::Record, "now"), (TrustMode::Mandatory, "mandatory")];

fn attack_cases(fx: &Value, attack: u64) -> Vec<Value> {
    fx["maliciousServer"].as_array().unwrap().iter().find(|e| e["attack"].as_u64() == Some(attack)).unwrap()["cases"].as_array().unwrap().clone()
}

fn rule_case<'a>(fx: &'a Value, replay: &str, name: &str) -> &'a Value {
    fx["rules"][replay].as_array().unwrap().iter().find(|r| text(&r["name"]) == name).unwrap()
}

/// The holder state a seal / call case starts from: its held secret in the
/// keyring, recorded as the case says (what an earlier `keep` left).
fn hold(v: &Victim, root_id: &str, epoch: u32, case: &Value) {
    if let Some(held) = case["held"].as_object() {
        v.holder.keyring().insert_secret(root_id, epoch, hex32(&held["secret"]));
        v.keeper.record(root_id, epoch, KeyRecord::parse(text(&held["record"])).unwrap());
    }
}

#[test]
fn attack_1_a_planted_epoch_from_mine_against_the_real_holder_both_modes() {
    for (mode, at) in MODES {
        for case in attack_cases(&fixture(), 1).iter().filter(|c| c["answers"]["sessionKeys.mine"].is_array()) {
            let v = victim(mode);
            let name = text(&case["name"]);
            let mine: Vec<HeldKey> = serde_json::from_value(case["answers"]["sessionKeys.mine"].clone()).unwrap();
            *lock(&v.liar.mine) = mine.clone();
            v.holder.pull(None).unwrap();
            let planted = &mine[0];
            let held = v.holder.keyring().secret(&planted.root_id, planted.epoch).is_some();
            let want = &case[at];
            assert_eq!(held, want["keep"].as_bool().unwrap(), "{name} ({at})");
            match want["record"].as_str() {
                Some(record) => assert_eq!(v.keeper.store().record(&planted.root_id, planted.epoch).map(KeyRecord::as_str), Some(record), "{name} ({at})"),
                None => {
                    assert_eq!(v.keeper.store().record(&planted.root_id, planted.epoch), None, "{name} ({at})");
                    let last = v.keeper.decisions().into_iter().rev().find(|d| d.rule == "R1").unwrap();
                    assert_eq!(last.verdict, format!("refused:{}", text(&want["reason"])), "{name} ({at})");
                }
            }
        }
    }
}

#[test]
fn attack_1_and_2_send_target_and_public_keys_against_the_real_holder_both_modes() {
    let fx = fixture();
    let mut ran = 0;
    for attack in [1, 2] {
        for case in attack_cases(&fx, attack).iter().filter(|c| c["replay"] == "sealCases") {
            let name = text(&case["name"]);
            let rule = rule_case(&fx, "sealCases", name);
            for (mode, at) in MODES {
                let v = victim(mode);
                let (root_id, epoch, public_key) = (text(&rule["rootId"]), rule["epoch"].as_u64().unwrap() as u32, text(&rule["publicKey"]));
                hold(&v, root_id, epoch, rule);
                let public = enet_core::sealed::public_key_from_b64(public_key).unwrap();
                let got = if let Some(target) = case["answers"].get("sessions.sendTarget") {
                    // The gateway's / composer's sendTarget path: its vouch rides along.
                    *lock(&v.liar.send_target) = Some(target.clone());
                    v.holder.check_seal(root_id, epoch, &public, target.get("vouch").filter(|x| !x.is_null()))
                } else {
                    // `publicKeys` through the holder's own lookup.
                    *lock(&v.liar.public_keys) = serde_json::from_value(case["answers"]["sessionKeys.publicKeys"].clone()).unwrap();
                    v.holder.public_key(root_id).map(|_| ()).map_err(coding::sealing::SealError::NoKey)
                };
                let want = &case[at];
                assert_eq!(got.is_ok(), want["seal"].as_bool().unwrap(), "{name} ({at}): {got:?}");
                let last = v.keeper.decisions().into_iter().rev().find(|d| d.rule == "R3").unwrap();
                let verdict = want["record"].as_str().map_or_else(|| format!("refused:{}", text(&want["reason"])), str::to_string);
                assert_eq!(last.verdict, verdict, "{name} ({at})");
                ran += 1;
            }
        }
    }
    assert_eq!(ran, 16);
}

#[test]
fn attack_3_a_false_reader_in_pending_and_the_install_list_both_modes() {
    let fx = fixture();
    let mut ran = 0;
    for case in attack_cases(&fx, 3) {
        let name = text(&case["name"]);
        for (mode, at) in MODES {
            let v = victim(mode);
            // The liar serves its reader in `installs.trust` too.
            let listed: Vec<ServedInstall> = serde_json::from_value(case["answers"]["installs.list"].clone()).unwrap();
            for row in &listed {
                let mut installs = lock(&v.liar.installs);
                installs.retain(|i| i.endpoint_id != row.endpoint_id);
                installs.push(row.clone());
            }
            if name.contains("claims a root") {
                lock(&v.liar.approvals).push(approval(&fx, ApprovalKind::Root, "Z", "Z"));
            }
            *lock(&v.liar.pending) = serde_json::from_value(case["answers"]["sessionKeys.pending"].clone()).unwrap();
            // The tree's real key, held and verified.
            let tree = hex32(&rule_case(&fx, "callCases", "honest: the verified key")["held"]["secret"]);
            v.holder.keyring().insert_secret("01234567-89ab-4def-8123-456789abcdef", 1, tree);
            v.holder.pass();
            let target = text(&case["answers"]["sessionKeys.pending"][0]["targetInstallId"]);
            assert!(lock(&v.liar.wrapped).iter().all(|w| w.target_install_id != target), "{name} ({at}): wrapped for the false reader");
            let last = v.keeper.decisions().into_iter().rev().find(|d| d.rule == "R4" && d.subject == target).unwrap();
            let want = format!("refused:{}", text(&case[at]["reason"]));
            // A pinned endpoint served with Z's key: once `installs.trust`
            // serves it so, its registration no longer binds it and the pin
            // is dropped (the server may shrink): refused all the same.
            if name.contains("pinned endpoint (B)") {
                assert!(last.verdict == want || last.verdict == "refused:registration", "{name} ({at}): {}", last.verdict);
            } else {
                assert_eq!(last.verdict, want, "{name} ({at})");
            }
            // Never approved by itself.
            assert!(lock(&v.liar.approves).iter().all(|(_, subject, _)| subject != target), "{name}");
            ran += 1;
        }
    }
    assert_eq!(ran, 12);
}

#[test]
fn attack_4_a_false_call_key_through_the_real_holder_both_modes() {
    let fx = fixture();
    let mut ran = 0;
    for case in attack_cases(&fx, 4) {
        let name = text(&case["name"]);
        let rule = rule_case(&fx, "callCases", name);
        for (mode, at) in MODES {
            let v = victim(mode);
            *lock(&v.liar.send_target) = Some(case["answers"]["sessions.sendTarget"].clone());
            let target = &rule["target"];
            let (root_id, epoch) = (text(&target["rootId"]), target["epoch"].as_u64().unwrap() as u32);
            // What the earlier pull left (record mode kept it; once
            // mandatory, R1 already refused an unsigned or unpinned wrap).
            let kept = rule["held"].is_object()
                && (mode == TrustMode::Record || matches!(rule["held"]["record"].as_str(), Some("verified" | "legacy")));
            if kept {
                hold(&v, root_id, epoch, rule);
            }
            let got = v.holder.call_secret(root_id, epoch);
            let want = &case[at];
            match want["key"].as_bool().unwrap() {
                true => assert!(matches!(got, Ok(Some(_))), "{name} ({at}): {got:?}"),
                false if want["noKey"] == "rollback" => assert_eq!(got, Err("rollback".to_string()), "{name} ({at})"),
                false => assert!(matches!(got, Ok(None) | Err(_)), "{name} ({at}): {got:?}"),
            }
            ran += 1;
        }
    }
    assert_eq!(ran, 8);
}

#[test]
fn attack_1_then_4_end_to_end_the_planted_wrap_is_what_the_call_gets() {
    let fx = fixture();
    let planted = attack_cases(&fx, 1)[0]["answers"]["sessionKeys.mine"].clone(); // unsigned
    for (mode, at) in MODES {
        let v = victim(mode);
        *lock(&v.liar.mine) = serde_json::from_value(planted.clone()).unwrap();
        v.holder.pass();
        let got = v.holder.call_secret("01234567-89ab-4def-8123-456789abcdef", 2);
        match at {
            // Record-only never fixes it: kept unsigned, the call gets it (recorded).
            "now" => assert!(matches!(got, Ok(Some(_))), "{got:?}"),
            // Mandatory: the wrap was refused, nothing held: no_key.
            _ => assert_eq!(got, Ok(None)),
        }
    }
}

// ── decision (e) ──────────────────────────────────────────────────────────

/// A keeper of install `name` on `liar`, its store at a fresh path:
/// `existing` = no store file at all (an install an earlier build registered).
fn keeper_of(fx: &Value, liar: &Arc<Liar>, name: &str, existing: bool) -> Arc<TrustKeeper> {
    let path = store_file();
    let id = identity(fx, name);
    if !existing {
        mark_new(&path, &id, text(&key(fx, name)["userId"])).unwrap();
    }
    Arc::new(TrustKeeper::open(path, &id, text(&key(fx, name)["userId"]), format!("install-{name}"), liar.clone(), TRUST_MODE))
}

#[test]
fn decision_e_day_one_no_row_and_no_waiting() {
    let fx = fixture();
    let liar = Arc::new(Liar::default());
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "B", None, false), served(&fx, "L", Some("none"), false)];
    let a = keeper_of(&fx, &liar, "A", true);
    a.sync();
    let snapshot = a.snapshot();
    assert_eq!(snapshot.state, TrustState::Trusted);
    assert!(snapshot.rows.is_empty());
    // It claimed the root with the approvals of every install that existed.
    assert_eq!(*lock(&liar.claims), vec![("install-A".to_string(), vec!["install-L".to_string(), "install-B".to_string()])]);
    assert!(lock(&liar.requests).is_empty(), "nobody asked");
    // The second install runs this release later: on the chain, never waits.
    let b = keeper_of(&fx, &liar, "B", true);
    b.sync();
    assert_eq!(b.snapshot().state, TrustState::Trusted);
    assert!(b.snapshot().rows.is_empty());
    assert_eq!(lock(&liar.claims).len(), 1);
}

#[test]
fn decision_e_two_existing_installs_at_once() {
    let fx = fixture();
    let liar = Arc::new(Liar::default());
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "B", None, false)];
    let before = TrustAnswer { installs: lock(&liar.installs).clone(), approvals: Vec::new() };
    let a = keeper_of(&fx, &liar, "A", true);
    let b = keeper_of(&fx, &liar, "B", true);
    a.sync();
    // B read the account before A's claim landed: it adopts and joins too,
    // its claim CONFLICTs, the next round finds it on A's chain.
    *lock(&liar.stale_once) = Some(before);
    b.sync();
    for keeper in [&a, &b] {
        assert_eq!(keeper.snapshot().state, TrustState::Trusted);
        assert!(keeper.snapshot().rows.is_empty());
    }
    assert_eq!(lock(&liar.claims).len(), 1, "one root");
    assert!(lock(&liar.requests).is_empty());
}

#[test]
fn decision_e_a_planted_install_after_the_first_reconcile_a_row_no_wrap_never_approved() {
    let fx = fixture();
    let v = victim(TRUST_MODE);
    // The server plants Z as an install that "existed" (silent, valid registration).
    lock(&v.liar.installs).push(served(&fx, "Z", None, false));
    *lock(&v.liar.pending) = serde_json::from_value(attack_cases(&fx, 3)[0]["answers"]["sessionKeys.pending"].clone()).unwrap();
    let tree = hex32(&rule_case(&fx, "callCases", "honest: the verified key")["held"]["secret"]);
    v.holder.keyring().insert_secret("01234567-89ab-4def-8123-456789abcdef", 1, tree);
    for _ in 0..3 {
        v.holder.pass();
    }
    let snapshot = v.keeper.snapshot();
    let z = endpoint(&fx, "Z");
    assert_eq!(snapshot.rows.iter().map(|r| (r.kind, r.endpoint_id.clone())).collect::<Vec<_>>(), vec![(RowKind::NewInstall, z.clone())]);
    assert!(!v.keeper.store().pins.contains_key(&z));
    assert!(lock(&v.liar.wrapped).iter().all(|w| w.target_install_id != "install-Z"));
    assert!(lock(&v.liar.approves).is_empty(), "never approved by itself");
    // Its stripped twin: no row (no key bound to its code), no pin, no wrap.
    let v = victim(TRUST_MODE);
    lock(&v.liar.installs).push(served(&fx, "Z", Some("none"), false));
    v.keeper.sync();
    assert!(v.keeper.snapshot().rows.is_empty());
    assert!(!v.keeper.store().pins.contains_key(&z));
    assert_eq!(v.keeper.target("install-Z", text(&key(&fx, "Z")["x25519Public"]), Some(&z), None), TargetVerdict::Refuse(TargetRefusal::Unapproved));
}

#[test]
fn decision_e_a_hidden_chain_waits_and_asks_once() {
    let fx = fixture();
    let liar = Arc::new(Liar::default());
    // N's first reconcile is past (its store exists); the server hides A's chain.
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "N", None, false)];
    let n = keeper_of(&fx, &liar, "N", false);
    n.sync();
    n.sync();
    let snapshot = n.snapshot();
    assert_eq!(snapshot.state, TrustState::Waiting);
    assert_eq!(snapshot.code.as_deref(), Some(text(&key(&fx, "N")["code"])));
    assert!(snapshot.rows.is_empty());
    assert!(lock(&liar.claims).is_empty(), "adopts nothing, claims nothing");
    assert_eq!(*lock(&liar.requests), vec!["install-N".to_string()], "asks once");
    // A waiting install wraps for nobody but itself.
    assert_eq!(n.target("install-A", text(&key(&fx, "A")["x25519Public"]), Some(&endpoint(&fx, "A")), None), TargetVerdict::Refuse(TargetRefusal::NotTrusted));
    assert_eq!(n.target("install-N", "x", None, None), TargetVerdict::Wrap(KeyRecord::Verified));
}

#[test]
fn decision_e_a_new_identity_writes_its_store_first_and_waits() {
    let fx = fixture();
    let path = store_file();
    let n = identity(&fx, "N");
    mark_new(&path, &n, "user-me").unwrap();
    let kept: TrustStore = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(!kept.existing);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // A chain of its account exists (A's root): it waits, never adopts.
    let liar = Arc::new(Liar::default());
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "B", None, false), served(&fx, "N", None, false)];
    *lock(&liar.approvals) = vec![approval(&fx, ApprovalKind::Root, "A", "A")];
    let keeper = TrustKeeper::open(path.clone(), &n, "user-me", "install-N".into(), liar.clone(), TRUST_MODE);
    keeper.sync();
    assert_eq!(keeper.snapshot().state, TrustState::Waiting);
    assert!(keeper.store().pins.keys().all(|id| *id == endpoint(&fx, "N")));
    // mark_new again (a restart) leaves the kept store alone.
    mark_new(&path, &n, "user-me").unwrap();
    assert_eq!(serde_json::from_str::<TrustStore>(&std::fs::read_to_string(&path).unwrap()).unwrap(), keeper.store());
}

#[test]
fn a_store_that_cannot_be_read_is_never_taken_for_missing() {
    let fx = fixture();
    let path = store_file();
    std::fs::create_dir_all(&path).unwrap(); // a directory: reading it fails
    let liar = Arc::new(Liar::default());
    *lock(&liar.installs) = vec![served(&fx, "A", None, false), served(&fx, "B", None, false)];
    let keeper = TrustKeeper::open(path.clone(), &identity(&fx, "A"), "user-me", "install-A".into(), liar.clone(), TRUST_MODE);
    assert!(keeper.unreadable());
    assert!(!keeper.store().existing, "never adopts");
    keeper.sync();
    assert!(lock(&liar.claims).is_empty() && lock(&liar.requests).is_empty());
    assert!(path.is_dir(), "never written over");
}

// ── the negatives ─────────────────────────────────────────────────────────

#[test]
fn never_approves_by_itself_only_on_a_press_of_its_own_row() {
    let fx = fixture();
    let v = victim(TRUST_MODE);
    lock(&v.liar.installs).push(served(&fx, "N", None, true));
    for _ in 0..3 {
        v.holder.pass();
    }
    assert!(lock(&v.liar.approves).is_empty());
    let row = v.keeper.snapshot().rows[0].clone();
    assert_eq!(row.kind, RowKind::NewInstall);
    // A row the keeper did not yield (another install id) is refused.
    let forged = TrustRow { install_id: "install-B".into(), ..row.clone() };
    assert!(v.keeper.approve(&forged).is_err());
    assert!(lock(&v.liar.approves).is_empty());
    // The press: approved, pinned, the row is gone.
    v.keeper.approve(&row).unwrap();
    assert_eq!(*lock(&v.liar.approves), vec![("install-A".to_string(), "install-N".to_string(), ApprovalKind::Approve)]);
    assert!(v.keeper.snapshot().rows.is_empty());
    assert!(v.keeper.store().pins.contains_key(&endpoint(&fx, "N")));
}

#[test]
fn remove_revokes_a_new_install_and_declines_a_reset() {
    let fx = fixture();
    let v = victim(TRUST_MODE);
    lock(&v.liar.installs).push(served(&fx, "N", None, true));
    // T1's account lost every install; T3 claims a new root.
    lock(&v.liar.installs).retain(|i| i.install_id != "install-T1");
    lock(&v.liar.installs).push(served(&fx, "T3", None, false));
    lock(&v.liar.approvals).push(approval(&fx, ApprovalKind::Root, "T3", "T3"));
    v.keeper.sync();
    let rows = v.keeper.snapshot().rows;
    assert_eq!(rows.iter().map(|r| r.kind).collect::<Vec<_>>(), vec![RowKind::NewInstall, RowKind::Reset]);
    v.keeper.remove(&rows[0]).unwrap();
    v.keeper.remove(&rows[1]).unwrap();
    assert_eq!(*lock(&v.liar.revokes), vec!["install-N".to_string()]);
    assert!(lock(&v.liar.approves).iter().any(|(_, s, k)| s == "install-T3" && *k == ApprovalKind::Decline));
    assert!(v.keeper.snapshot().rows.is_empty());
}

#[test]
fn never_shows_a_row_whose_code_the_server_supplied() {
    let fx = fixture();
    let v = victim(TRUST_MODE);
    // A served row carrying its own "code" and a lying label: the code is
    // derived from the endpoint id here, the extra field is never read.
    let mut z = serde_json::to_value(served(&fx, "Z", None, true)).unwrap();
    z["code"] = json!("AAAAA AAAAA");
    let z: ServedInstall = serde_json::from_value(z).unwrap();
    lock(&v.liar.installs).push(z);
    v.keeper.sync();
    let row = &v.keeper.snapshot().rows[0];
    assert_eq!(row.code, install_code(&endpoint(&fx, "Z")).unwrap());
    assert_eq!(row.code, text(&key(&fx, "Z")["code"]));
}

#[test]
fn a_reader_installs_trust_does_not_list_is_judged_by_r4_alone_both_modes() {
    // One rule ×4: the keeper adds nothing to `wrapTarget` for a reader the
    // server names but `installs.trust` does not list (no registration, never
    // asked, no root, not the own account = the fixture's older team-mate).
    let fx = fixture();
    let case = rule_case(&fx, "targetCases", "a team-mate's install nobody pinned that never asked and claims no root (an older build): legacy");
    for (mode, at) in MODES {
        let v = victim(mode);
        let t2 = endpoint(&fx, "T2");
        assert!(lock(&v.liar.installs).iter().all(|i| i.endpoint_id != t2));
        let got = v.keeper.target("install-T2", text(&key(&fx, "T2")["x25519Public"]), Some(&t2), None);
        let got = match got {
            TargetVerdict::Wrap(record) => record.as_str().to_string(),
            TargetVerdict::Refuse(reason) => format!("refused:{}", reason.as_str()),
        };
        let want = if case[at]["wrap"].as_bool().unwrap() { text(&case[at]["record"]).to_string() } else { format!("refused:{}", text(&case[at]["reason"])) };
        assert_eq!(got, want, "{at}");
    }
}

#[test]
fn a_server_without_install_trust_changes_nothing() {
    let fx = fixture();
    let liar = Arc::new(Liar { no_trust: true, ..Default::default() });
    let a = identity(&fx, "A");
    let keeper = Arc::new(TrustKeeper::open(store_file(), &a, "user-me", "install-A".into(), liar.clone(), TRUST_MODE));
    let holder = KeyHolder::signing("install-A".into(), &a, liar.clone(), Keyring::in_memory());
    holder.set_trust(keeper.clone());
    let tree = hex32(&rule_case(&fx, "callCases", "honest: the verified key")["held"]["secret"]);
    holder.keyring().insert_secret("01234567-89ab-4def-8123-456789abcdef", 1, tree);
    // Any reader the server lists is wrapped for, as before (recorded legacy).
    *lock(&liar.pending) = serde_json::from_value(attack_cases(&fx, 3)[0]["answers"]["sessionKeys.pending"].clone()).unwrap();
    // An older build's unsigned wrap is kept.
    *lock(&liar.mine) = serde_json::from_value(attack_cases(&fx, 1)[0]["answers"]["sessionKeys.mine"].clone()).unwrap();
    holder.pass();
    assert_eq!(keeper.snapshot().state, TrustState::Unavailable);
    assert!(keeper.snapshot().rows.is_empty());
    assert_eq!(lock(&liar.wrapped).len(), 1);
    assert!(holder.keyring().secret("01234567-89ab-4def-8123-456789abcdef", 2).is_some());
    assert!(lock(&liar.claims).is_empty() && lock(&liar.requests).is_empty());
}

#[test]
fn the_build_constant_is_record_only() {
    assert_eq!(TRUST_MODE, TrustMode::Record);
}

// ── #EXP-2123: who makes a chat key (install-trust.json `keyMaker`) ──────

/// One server double for the key writes, HONEST or MALICIOUS, over the
/// liar's install chain (`installs.trust`). Honest: a waiting install's
/// `create` is refused 403 `KEY_MAKER_REFUSAL` (and, `refuse_all`, a joined
/// one's too), `due` names it nothing. Malicious: every key write is
/// accepted and every root it knows is named due AND stranded.
struct KeyServer {
    honest: bool,
    /// The server lists the install as waiting.
    waits: std::sync::atomic::AtomicBool,
    /// Honest, but it refuses a joined install too (its chain lags).
    refuse_all: std::sync::atomic::AtomicBool,
    chain: Arc<Liar>,
    /// Stored keys `(epoch, public)` per root.
    roots: Mutex<HashMap<String, (u32, String)>>,
    creates: Mutex<Vec<CreateKey>>,
    /// The roots `due` names honestly: `(root, epoch, stranded)`.
    due: Mutex<Vec<(String, u32, bool)>>,
    requested: Mutex<Vec<String>>,
    /// The key the tree's device makes when asked (`requestKey`).
    device_key: Mutex<Option<enet_core::sealed::TreeKey>>,
}

impl KeyServer {
    fn new(honest: bool, waits: bool, chain: Arc<Liar>) -> Arc<KeyServer> {
        Arc::new(KeyServer {
            honest,
            waits: waits.into(),
            refuse_all: false.into(),
            chain,
            roots: Mutex::default(),
            creates: Mutex::default(),
            due: Mutex::default(),
            requested: Mutex::default(),
            device_key: Mutex::default(),
        })
    }
    fn refuses(&self) -> bool {
        self.honest && (self.waits.load(Ordering::SeqCst) || self.refuse_all.load(Ordering::SeqCst))
    }
    fn creates(&self) -> usize {
        lock(&self.creates).len()
    }
}

impl KeyApi for KeyServer {
    fn create(&self, input: &CreateKey) -> Result<(), ApiError> {
        if self.refuses() {
            return Err(ApiError::Http { status: 403, message: enet_core::install_trust::KEY_MAKER_REFUSAL.into() });
        }
        let mut roots = lock(&self.roots);
        match roots.get(&input.root_id) {
            None => {}
            Some((epoch, _)) if input.epoch == epoch + 1 => {}
            _ => return Err(ApiError::Http { status: 409, message: "keyed".into() }),
        }
        roots.insert(input.root_id.clone(), (input.epoch, input.public_key.clone()));
        lock(&self.creates).push(input.clone());
        Ok(())
    }
    fn mine(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<HeldKey>, ApiError> {
        Ok(Vec::new())
    }
    fn pending(&self, _install_id: &str, _root_ids: Option<&[String]>) -> Result<Vec<PendingWrap>, ApiError> {
        Ok(Vec::new())
    }
    fn wrap(&self, _install_id: &str, _wraps: &[PairWrap]) -> Result<(), ApiError> {
        Ok(())
    }
    fn public_keys(&self, root_ids: &[String]) -> Result<Vec<RootKey>, ApiError> {
        Ok(lock(&self.roots)
            .iter()
            .filter(|(root, _)| root_ids.contains(root))
            .map(|(root, (epoch, public))| RootKey { root_id: root.clone(), epoch: *epoch, public_key: public.clone(), vouch: None })
            .collect())
    }
    fn request_key(&self, root_id: &str) -> Result<api::session_keys::RequestedKey, ApiError> {
        lock(&self.requested).push(root_id.to_string());
        // The tree's (approved) device keys it: the key lands for the next read.
        if let Some(key) = lock(&self.device_key).as_ref() {
            lock(&self.roots).insert(root_id.to_string(), (1, key.public_key()));
            return Ok(api::session_keys::RequestedKey::Requested);
        }
        Ok(api::session_keys::RequestedKey::NoHost)
    }
    fn due(&self, _install_id: &str) -> Result<Vec<api::session_keys::DueRoot>, ApiError> {
        let rows = lock(&self.due).clone();
        if !self.honest {
            let mut every: Vec<api::session_keys::DueRoot> =
                lock(&self.roots).iter().map(|(root, (epoch, _))| api::session_keys::DueRoot { root_id: root.clone(), epoch: *epoch, stranded: true }).collect();
            every.extend(rows.into_iter().map(|(root_id, epoch, _)| api::session_keys::DueRoot { root_id, epoch, stranded: true }));
            return Ok(every);
        }
        if self.waits.load(Ordering::SeqCst) {
            return Ok(Vec::new());
        }
        Ok(rows.into_iter().map(|(root_id, epoch, stranded)| api::session_keys::DueRoot { root_id, epoch, stranded }).collect())
    }
}

impl TrustApi for KeyServer {
    fn trust(&self, install_id: &str) -> Result<TrustAnswer, ApiError> {
        self.chain.trust(install_id)
    }
    fn request_approval(&self, install_id: &str) -> Result<(), ApiError> {
        self.chain.request_approval(install_id)
    }
    fn claim_root(&self, install_id: &str, signature: &str, approvals: &[RootClaimApproval]) -> Result<(), ApiError> {
        self.chain.claim_root(install_id, signature, approvals)
    }
    fn approve(&self, install_id: &str, subject_install_id: &str, kind: ApprovalKind, signature: &str) -> Result<(), ApiError> {
        self.chain.approve(install_id, subject_install_id, kind, signature)
    }
    fn revoke(&self, install_id: &str) -> Result<(), ApiError> {
        self.chain.revoke(install_id)
    }
}

/// Install N of `user-me`: A holds the account's chain; N asked and is
/// waiting, or (`approved`) A approved it.
fn install_n(honest: bool, approved: bool) -> (Arc<KeyServer>, KeyHolder, Arc<TrustKeeper>) {
    let fx = fixture();
    let chain = Arc::new(Liar::default());
    *lock(&chain.installs) = vec![served(&fx, "A", None, false), served(&fx, "N", None, !approved)];
    let mut approvals = vec![approval(&fx, ApprovalKind::Root, "A", "A")];
    if approved {
        approvals.push(approval(&fx, ApprovalKind::Approve, "N", "A"));
    }
    *lock(&chain.approvals) = approvals;
    let server = KeyServer::new(honest, !approved, chain.clone());
    let path = store_file();
    let n = identity(&fx, "N");
    mark_new(&path, &n, "user-me").unwrap();
    let keeper = Arc::new(TrustKeeper::open(path, &n, "user-me", "install-N".into(), server.clone(), TRUST_MODE));
    let holder = KeyHolder::signing("install-N".into(), &n, server.clone(), Keyring::in_memory());
    holder.set_trust(keeper.clone());
    (server, holder, keeper)
}

/// The free functions seal through the process-global service.
static GLOBAL: Mutex<()> = Mutex::new(());

/// A synced root row flagged for rotation at `epoch`.
fn flagged_row(root: &str, epoch: u32, public: &[u8; 32]) -> domain::rows::SessionRow {
    domain::rows::SessionRow {
        id: root.into(),
        seal_public_key: Some(enet_core::sealed::b64url_encode(public)),
        seal_epoch: Some(i64::from(epoch)),
        seal_rotate_epoch: Some(i64::from(epoch)),
        ..Default::default()
    }
}

#[test]
fn a_waiting_install_makes_no_chat_key_against_an_honest_and_a_malicious_server() {
    let _global = lock(&GLOBAL);
    for honest in [true, false] {
        let kind = if honest { "honest" } else { "malicious" };
        let (server, holder, keeper) = install_n(honest, false);
        // `Unknown` = reconciled first, then judged.
        assert!(!holder.joined(), "{kind}");
        assert_eq!(keeper.snapshot().state, TrustState::Waiting, "{kind}");

        // (a) The first message of a NEW chat: no seal, the send fails.
        assert_eq!(holder.new_root_seal(Vec::new()).err(), Some(coding::sealing::SealError::KeyMakerWaits), "{kind}");
        assert_eq!(coding::sealing::SealError::KeyMakerWaits.to_string(), coding::sealing::NO_KEY_REFUSAL);

        // (b) The first message into a KEYLESS chat: nothing created, its
        // devices asked, sealed to the key the device made.
        assert_eq!(holder.ensure_key("keyless"), Err(coding::sealing::SealError::KeyMakerWaits), "{kind}");
        let device = enet_core::sealed::generate_tree_key();
        *lock(&server.device_key) = Some(device.clone());
        coding::sealing::install(Arc::new(holder.clone()));
        let body = coding::sealing::seal_body("keyless", "the first words");
        let opened = body.as_ref().ok().map(|body| enet_core::sealed::open_body(body, &|e| (e == 1).then_some(device.secret)).text);
        // (b') a tree no device keys: the no-key refusal, never plaintext.
        *lock(&server.device_key) = None;
        let none = coding::sealing::seal_body("keyless-2", "never in clear");
        // (d) A daemon asked to key its tree (`tree_key`): skipped, nothing asked.
        let asked = lock(&server.requested).len();
        coding::sealing::ensure_tree_key("claimed");
        let asked_at_claim = lock(&server.requested).len() - asked;
        coding::sealing::uninstall();
        assert_eq!(opened, Some(Some("the first words".to_string())), "{kind}: {body:?}");
        assert!(lock(&server.requested).contains(&"keyless".to_string()), "{kind}");
        assert_eq!(none, Err(coding::sealing::SealError::NoKey(coding::sealing::NO_KEY_REFUSAL.into())), "{kind}");
        assert_eq!(asked_at_claim, 0, "{kind}: a claim asks nobody");
        assert_eq!(server.creates(), 0, "{kind}: no key written by a waiting install");

        // (c) It races a due rotation holding the current epoch: never
        // creates, pass after pass, and remembers nothing.
        let held = enet_core::sealed::generate_tree_key();
        holder.keyring().insert_secret("due", 1, held.secret);
        holder.keyring().note_sessions(&[flagged_row("due", 1, &held.public)]);
        lock(&server.roots).insert("due".into(), (1, held.public_key()));
        lock(&server.due).push(("due".into(), 1, false));
        for _ in 0..3 {
            holder.pass();
            assert_eq!(holder.rotate_due(), 0, "{kind}");
            assert_eq!(holder.rotate_root("due", 1), Ok(false), "{kind}");
        }
        assert_eq!(server.creates(), 0, "{kind}: never rotated");
        assert_eq!(holder.refused_count(), 0, "{kind}: nothing remembered");
        assert_eq!(server.roots.lock().unwrap()["due"].0, 1, "{kind}");
    }
}

#[test]
fn an_approved_install_keys_and_rotates() {
    let _global = lock(&GLOBAL);
    let (server, holder, keeper) = install_n(true, true);
    assert!(holder.joined());
    assert_eq!(keeper.snapshot().state, TrustState::Trusted);
    assert!(holder.new_root_seal(Vec::new()).is_ok());
    assert_eq!(holder.ensure_key("keyless").unwrap().0, 1);
    assert_eq!(server.creates(), 1);
    // A due rotation of a root it holds.
    holder.keyring().note_sessions(&[flagged_row("keyless", 1, &holder.keyring().current("keyless").unwrap().1)]);
    lock(&server.due).push(("keyless".into(), 1, false));
    assert_eq!(holder.rotate_due(), 1);
    assert_eq!(server.roots.lock().unwrap()["keyless"].0, 2);
    // The daemon's claim keys its tree.
    coding::sealing::install(Arc::new(holder.clone()));
    coding::sealing::ensure_tree_key("claimed");
    coding::sealing::uninstall();
    assert_eq!(server.roots.lock().unwrap().get("claimed").map(|r| r.0), Some(1));
}

#[test]
fn a_joined_install_the_honest_server_refuses_does_not_remember_it() {
    let _global = lock(&GLOBAL);
    let (server, holder, _keeper) = install_n(true, true);
    assert!(holder.joined());
    server.refuse_all.store(true, Ordering::SeqCst);
    // A first key: the same step as waiting (its devices are asked).
    assert_eq!(holder.ensure_key("keyless"), Err(coding::sealing::SealError::KeyMakerWaits));
    // A due rotation: skipped, never remembered.
    let held = enet_core::sealed::generate_tree_key();
    holder.keyring().insert_secret("due", 1, held.secret);
    holder.keyring().note_sessions(&[flagged_row("due", 1, &held.public)]);
    lock(&server.roots).insert("due".into(), (1, held.public_key()));
    assert_eq!(holder.rotate_due(), 0);
    assert_eq!(holder.rotate_root("due", 1), Ok(false));
    assert_eq!(holder.refused_count(), 0);
    // The chain caught up: it rotates now.
    server.refuse_all.store(false, Ordering::SeqCst);
    assert_eq!(holder.rotate_due(), 1);
    assert_eq!(server.roots.lock().unwrap()["due"].0, 2);
}

#[test]
fn a_joined_install_rotates_a_stranded_root_it_holds_no_secret_of() {
    let _global = lock(&GLOBAL);
    let (server, holder, _keeper) = install_n(true, true);
    // Keyed by a waiting install before the rule: no approved reader holds epoch 1.
    let stranded = enet_core::sealed::generate_tree_key();
    lock(&server.roots).insert("stranded".into(), (1, stranded.public_key()));
    // Not stranded and not held: not this install's (as before).
    lock(&server.due).push(("stranded".into(), 1, false));
    assert_eq!(holder.rotate_due(), 0);
    assert!(server.creates() == 0);
    // Named stranded: rotated WITHOUT the old secret, its own wrap.
    lock(&server.due).clear();
    lock(&server.due).push(("stranded".into(), 1, true));
    assert_eq!(holder.rotate_due(), 1);
    let create = lock(&server.creates)[0].clone();
    assert_eq!((create.root_id.as_str(), create.epoch), ("stranded", 2));
    assert_eq!(create.wraps.iter().map(|w| w.install_id.as_str()).collect::<Vec<_>>(), ["install-N"]);
    assert!(create.wraps[0].signature.is_some(), "its own signed wrap");
    assert!(holder.keyring().secret("stranded", 2).is_some());
    assert!(holder.keyring().secret("stranded", 1).is_none());
    // Already rotated: nothing again.
    assert_eq!(holder.rotate_due(), 0);
    assert_eq!(server.creates(), 1);
}
