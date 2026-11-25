//! #EXP-1950: replays EVERY case of
//! `packages/domain-contract/fixtures/install-trust.json` by name (the TS
//! reference's `install-trust.test.ts`, case for case).

use super::*;
use serde_json::Value;

pub(crate) fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/install-trust.json")).unwrap()
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap()
}
fn hex32(v: &Value) -> [u8; 32] {
    HEXLOWER.decode(text(v).as_bytes()).unwrap().try_into().unwrap()
}
fn key<'a>(fx: &'a Value, name: &str) -> &'a Value {
    &fx["keys"][name]
}
fn name_of(fx: &Value, endpoint_id: &str) -> String {
    fx["keys"].as_object().unwrap().iter().find(|(_, k)| text(&k["endpointId"]) == endpoint_id).map(|(n, _)| n.clone()).unwrap()
}
fn identity(fx: &Value, name: &str) -> Identity {
    Identity::from_bytes(hex32(&key(fx, name)["identity"]))
}

/// install-trust.test.ts `served`: `registration` none | forged (Z signed it).
fn served(fx: &Value, spec: &Value) -> ServedInstall {
    let name = text(&spec["key"]);
    let k = key(fx, name);
    let registration = spec["registration"].as_str();
    ServedInstall {
        install_id: format!("install-{name}"),
        user_id: text(&k["userId"]).into(),
        endpoint_id: text(&k["endpointId"]).into(),
        x25519_public: spec["x25519Public"].as_str().unwrap_or(text(&k["x25519Public"])).into(),
        registration_signature: match registration {
            Some("none") => None,
            Some("forged") => Some(sign_registration(&identity(fx, "Z"), text(&k["userId"]), text(&k["signedAt"]))),
            _ => Some(text(&k["registrationSignature"]).into()),
        },
        registration_signed_at: (registration != Some("none")).then(|| text(&k["signedAt"]).into()),
        approval_requested: spec["approvalRequested"].as_bool().unwrap_or(false),
        label: Some(text(&k["label"]).into()),
    }
}

/// install-trust.test.ts `buildStore`.
fn build_store(fx: &Value, spec: &Value) -> TrustStore {
    let viewer = key(fx, text(&spec["viewer"]));
    let mut store = TrustStore::empty(text(&viewer["endpointId"]), text(&viewer["userId"]), spec["existing"].as_bool().unwrap_or(false));
    store.joined = spec["joined"].as_bool().unwrap();
    for (name, pin) in spec["pins"].as_object().unwrap() {
        let k = key(fx, name);
        let via: PinVia = serde_json::from_value(pin[0].clone()).unwrap();
        let by = pin[1].as_str().map(|by| text(&key(fx, by)["endpointId"]).to_string());
        store.pins.insert(
            text(&k["endpointId"]).into(),
            Pin { user_id: text(&k["userId"]).into(), x25519_public: text(&k["x25519Public"]).into(), via, by },
        );
    }
    store.members = spec["members"].as_array().map_or_else(Vec::new, |m| m.iter().map(|u| text(u).to_string()).collect());
    store.declined =
        spec["declined"].as_array().map_or_else(Vec::new, |d| d.iter().map(|n| text(&key(fx, text(n))["endpointId"]).to_string()).collect());
    if let Some(roots) = spec["roots"].as_object() {
        for (root, record) in roots {
            store.roots.insert(root.clone(), serde_json::from_value(record.clone()).unwrap());
        }
    }
    store
}

fn store_named(fx: &Value, name: &str) -> TrustStore {
    build_store(fx, &fx["stores"][name])
}

fn modes() -> [(TrustMode, &'static str); 2] {
    [(TrustMode::Record, "now"), (TrustMode::Mandatory, "mandatory")]
}

fn held(case: &Value) -> Option<([u8; 32], KeyRecord)> {
    case["held"].as_object().map(|h| (hex32(&h["secret"]), KeyRecord::parse(text(&h["record"])).unwrap()))
}

pub(crate) fn answer_of(fx: &Value, case: &Value) -> TrustAnswer {
    TrustAnswer {
        installs: case["installs"].as_array().unwrap().iter().map(|spec| served(fx, spec)).collect(),
        approvals: case["approvals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| ServedApproval {
                kind: serde_json::from_value(a["kind"].clone()).unwrap(),
                subject_endpoint_id: text(&key(fx, text(&a["subject"]))["endpointId"]).into(),
                approver_endpoint_id: text(&key(fx, text(&a["approver"]))["endpointId"]).into(),
                signature: text(&a["signature"]).into(),
            })
            .collect(),
    }
}

#[test]
fn the_constants_the_keys_and_their_registrations() {
    let fx = fixture();
    assert_eq!(text(&fx["mode"]["release1"]), TRUST_MODE.as_str());
    assert_eq!(text(&fx["mode"]["release2"]), TrustMode::Mandatory.as_str());
    let c = &fx["copy"];
    assert_eq!(
        [text(&c["newInstall"]), text(&c["reset"]), text(&c["approve"]), text(&c["remove"]), text(&c["waiting"])],
        [copy::NEW_INSTALL, copy::RESET, copy::APPROVE, copy::REMOVE, copy::WAITING]
    );
    assert_eq!(text(&fx["icon"]), WAITING_ICON);
    assert_eq!(text(&fx["code"]["label"]), INSTALL_CODE_LABEL);
    assert_eq!(text(&fx["code"]["alphabet"]).as_bytes(), CROCKFORD);
    assert_eq!(text(&fx["approval"]["label"]), APPROVAL_LABEL);
    assert_eq!(fx["approval"]["messageBytes"].as_u64().unwrap() as usize, APPROVAL_MESSAGE_BYTES);
    let layout: u64 = fx["approval"]["layout"].as_object().unwrap().values().map(|n| n.as_u64().unwrap()).sum();
    assert_eq!(layout as usize, APPROVAL_MESSAGE_BYTES);
    for (kind, byte) in fx["approval"]["kinds"].as_object().unwrap() {
        let kind: ApprovalKind = serde_json::from_value(Value::String(kind.clone())).unwrap();
        assert_eq!(u64::from(kind.byte()), byte.as_u64().unwrap());
    }
    for (name, k) in fx["keys"].as_object().unwrap() {
        let id = identity(&fx, name);
        assert_eq!(HEXLOWER.encode(id.endpoint_id().as_bytes()), text(&k["endpointId"]), "{name}");
        assert_eq!(install_x25519_of(&id.to_bytes()), text(&k["x25519Public"]), "{name}");
        assert_eq!(sign_registration(&id, text(&k["userId"]), text(&k["signedAt"])), text(&k["registrationSignature"]), "{name}");
        assert_eq!(install_code(text(&k["endpointId"])).as_deref(), Some(text(&k["code"])), "{name}");
        assert!(registration_valid(&served(&fx, &serde_json::json!({ "key": name }))), "{name}");
        assert_eq!(registration_valid(&served(&fx, &serde_json::json!({ "key": name, "registration": "forged" }))), name == "Z");
        assert!(!registration_valid(&served(&fx, &serde_json::json!({ "key": name, "registration": "none" }))), "{name}");
    }
}

#[test]
fn the_code_vectors() {
    let fx = fixture();
    for v in fx["code"]["vectors"].as_array().unwrap() {
        let code = install_code(text(&v["endpointId"])).unwrap();
        assert_eq!(code, text(&v["code"]));
        assert_eq!(code.len(), 11);
        assert!(code.chars().enumerate().all(|(i, c)| if i == 5 { c == ' ' } else { CROCKFORD.contains(&(c as u8)) }));
    }
    assert_eq!(install_code("not hex"), None);
    assert_eq!(install_code(&"A".repeat(64)), None, "uppercase is not an endpoint id");
}

#[test]
fn the_approval_vector_byte_exact() {
    let fx = fixture();
    let v = &fx["approval"]["vector"];
    let kind: ApprovalKind = serde_json::from_value(v["kind"].clone()).unwrap();
    let s = &v["subject"];
    let subject = ApprovalSubject { user_id: text(&s["userId"]), endpoint_id: text(&s["endpointId"]), x25519_public: text(&s["x25519Public"]) };
    assert_eq!(HEXLOWER.encode(&approval_message(kind, &subject).unwrap()), text(&v["message"]));
    assert_eq!(sign_approval(&identity(&fx, "A"), kind, &subject).unwrap(), text(&v["signature"]));
    let approver = text(&v["approverEndpointId"]);
    let sig = text(&v["signature"]);
    assert!(verify_approval(kind, &subject, sig, approver));
    // Every field is covered.
    assert!(!verify_approval(ApprovalKind::Reset, &subject, sig, approver));
    assert!(!verify_approval(kind, &ApprovalSubject { user_id: "user-other", ..subject }, sig, approver));
    let b = key(&fx, "B");
    assert!(!verify_approval(kind, &ApprovalSubject { endpoint_id: text(&b["endpointId"]), ..subject }, sig, approver));
    assert!(!verify_approval(kind, &ApprovalSubject { x25519_public: text(&b["x25519Public"]), ..subject }, sig, approver));
    assert!(!verify_approval(kind, &subject, sig, text(&b["endpointId"])));
}

#[test]
fn trust_cases_every_one_by_name() {
    let fx = fixture();
    let cases = fx["trustCases"].as_array().unwrap();
    assert!(cases.len() >= 34);
    for c in cases {
        let name = text(&c["name"]);
        let view = reconcile_trust(&build_store(&fx, &c["before"]), &answer_of(&fx, c));
        assert_eq!(view.status.as_str(), text(&c["status"]), "{name}: status");
        assert_eq!(view.store.joined, c["after"]["joined"].as_bool().unwrap(), "{name}: joined");
        let pins: BTreeMap<String, (String, Option<String>)> = view
            .store
            .pins
            .iter()
            .map(|(id, pin)| (name_of(&fx, id), (pin.via.as_str().to_string(), pin.by.as_deref().map(|by| name_of(&fx, by)))))
            .collect();
        let want: BTreeMap<String, (String, Option<String>)> = c["after"]["pins"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(n, p)| (n.clone(), (text(&p[0]).to_string(), p[1].as_str().map(str::to_string))))
            .collect();
        assert_eq!(pins, want, "{name}: pins");
        let members: Vec<&str> = c["after"]["members"].as_array().unwrap().iter().map(text).collect();
        assert_eq!(view.store.members, members, "{name}: members");
        let declined: Vec<String> = view.store.declined.iter().map(|id| name_of(&fx, id)).collect();
        let want_declined: Vec<&str> = c["after"]["declined"].as_array().unwrap().iter().map(text).collect();
        assert_eq!(declined, want_declined, "{name}: declined");
        let rows: Vec<Value> = view
            .rows
            .iter()
            .map(|row| {
                serde_json::json!({ "kind": row.kind.as_str(), "key": name_of(&fx, &row.endpoint_id), "installId": row.install_id, "code": row.code, "label": row.label })
            })
            .collect();
        assert_eq!(Value::Array(rows), c["rows"], "{name}: rows");
        let approve: Vec<String> = view.approve_existing.iter().map(|id| name_of(&fx, id)).collect();
        let want_approve: Vec<&str> = c["approveExisting"].as_array().map_or_else(Vec::new, |a| a.iter().map(text).collect());
        assert_eq!(approve, want_approve, "{name}: approveExisting");
        if let Some(existing) = c["after"]["existing"].as_bool() {
            assert_eq!(view.store.existing, existing, "{name}: existing");
        }
    }
}

#[test]
fn decision_e_day_one_no_row_no_waiting() {
    let fx = fixture();
    let day_one: Vec<&Value> = fx["trustCases"].as_array().unwrap().iter().filter(|c| c["dayOne"].as_bool() == Some(true)).collect();
    assert!(day_one.len() >= 4);
    for c in day_one {
        let view = reconcile_trust(&build_store(&fx, &c["before"]), &answer_of(&fx, c));
        assert!(view.rows.is_empty(), "{}", text(&c["name"]));
        assert_ne!(view.status, TrustStatus::Waiting, "{}", text(&c["name"]));
        assert!(view.store.joined, "{}", text(&c["name"]));
    }
}

#[test]
fn decision_e_a_planted_install_after_the_first_reconcile_is_never_pinned() {
    let fx = fixture();
    let z = text(&key(&fx, "Z")["endpointId"]).to_string();
    let attacks: Vec<&Value> = fx["trustCases"].as_array().unwrap().iter().filter(|c| c["attack"].as_str() == Some("existing")).collect();
    assert!(attacks.len() >= 3);
    let mut z_row = false;
    for c in attacks {
        let before = build_store(&fx, &c["before"]);
        let view = reconcile_trust(&before, &answer_of(&fx, c));
        z_row |= view.rows.iter().any(|row| row.endpoint_id == z && row.kind == RowKind::NewInstall);
        if before.existing {
            continue; // the accepted moment: the first reconcile
        }
        assert!(!view.store.pins.contains_key(&z), "{}", text(&c["name"]));
        assert_eq!(view.store.pins.keys().collect::<Vec<_>>(), before.pins.keys().collect::<Vec<_>>(), "{}", text(&c["name"]));
    }
    assert!(z_row);
}

#[test]
fn r1_keep_cases_now_and_once_mandatory() {
    let fx = fixture();
    for c in fx["rules"]["keepCases"].as_array().unwrap() {
        let store = store_named(&fx, text(&c["store"]));
        let viewer = text(&fx["stores"][text(&c["store"])]["viewer"]);
        let w = &c["wrap"];
        let wrap = ServedWrap {
            root_id: text(&w["rootId"]),
            epoch: w["epoch"].as_u64().unwrap() as u32,
            wrapped: text(&w["wrapped"]),
            signature: w["signature"].as_str(),
            signer_endpoint_id: w["signerEndpointId"].as_str(),
        };
        for (mode, at) in modes() {
            let got = keep_wrap(&wrap, &identity(&fx, viewer), &hex32(&key(&fx, viewer)["installSecret"]), &store, mode);
            let got = match got {
                KeepVerdict::Keep { record, .. } => serde_json::json!({ "keep": true, "record": record.as_str() }),
                KeepVerdict::Refuse(reason) => serde_json::json!({ "keep": false, "reason": reason.as_str() }),
            };
            assert_eq!(got, c[at], "{} ({at})", text(&c["name"]));
        }
    }
}

#[test]
fn r2_r3_seal_cases_now_and_once_mandatory() {
    let fx = fixture();
    for c in fx["rules"]["sealCases"].as_array().unwrap() {
        let store = store_named(&fx, text(&c["store"]));
        let vouch: Option<Vouch> = c["vouch"].is_object().then(|| serde_json::from_value(c["vouch"].clone()).unwrap());
        for (mode, at) in modes() {
            let got = seal_to(text(&c["rootId"]), c["epoch"].as_u64().unwrap() as u32, text(&c["publicKey"]), held(c), vouch.as_ref(), &store, mode);
            let got = match got {
                SealVerdict::Seal(record) => serde_json::json!({ "seal": true, "record": record.as_str() }),
                SealVerdict::Refuse(reason) => serde_json::json!({ "seal": false, "reason": reason.as_str() }),
            };
            assert_eq!(got, c[at], "{} ({at})", text(&c["name"]));
        }
    }
}

#[test]
fn r4_target_cases_now_and_once_mandatory() {
    let fx = fixture();
    for c in fx["rules"]["targetCases"].as_array().unwrap() {
        let store = store_named(&fx, text(&c["store"]));
        let roots: HashSet<String> = c["roots"].as_array().unwrap().iter().map(|n| text(&key(&fx, text(n))["endpointId"]).to_string()).collect();
        for (mode, at) in modes() {
            let got = match wrap_target(&served(&fx, &c["target"]), &store, mode, &roots) {
                TargetVerdict::Wrap(record) => serde_json::json!({ "wrap": true, "record": record.as_str() }),
                TargetVerdict::Refuse(reason) => serde_json::json!({ "wrap": false, "reason": reason.as_str() }),
            };
            assert_eq!(got, c[at], "{} ({at})", text(&c["name"]));
        }
    }
}

#[test]
fn call_cases_now_and_once_mandatory() {
    let fx = fixture();
    for c in fx["rules"]["callCases"].as_array().unwrap() {
        let store = store_named(&fx, text(&c["store"]));
        for (mode, at) in modes() {
            let t = &c["target"];
            let got = match call_key(text(&t["rootId"]), t["epoch"].as_u64().unwrap() as u32, held(c), &store, mode) {
                CallKeyVerdict::Key { record, .. } => serde_json::json!({ "key": true, "record": record.as_str() }),
                CallKeyVerdict::NoKey(reason) => serde_json::json!({ "key": false, "noKey": reason.as_str() }),
            };
            assert_eq!(got, c[at], "{} ({at})", text(&c["name"]));
        }
    }
}

#[test]
fn malicious_server_every_attack_replays_its_rule_by_name() {
    let fx = fixture();
    let entries = fx["maliciousServer"].as_array().unwrap();
    assert_eq!(entries.iter().map(|e| e["attack"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 2, 3, 4]);
    for entry in entries {
        let cases = entry["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for c in cases {
            let name = text(&c["name"]);
            let rule = fx["rules"][text(&c["replay"])].as_array().unwrap();
            let source = rule.iter().find(|r| text(&r["name"]) == name).unwrap_or_else(|| panic!("{name}: no rule case"));
            assert_eq!(source["attack"], entry["attack"], "{name}");
            assert_eq!((&c["now"], &c["mandatory"]), (&source["now"], &source["mandatory"]), "{name}");
            assert!(!c["answers"].as_object().unwrap().is_empty(), "{name}");
            let m = &c["mandatory"];
            let verdict = [&m["keep"], &m["seal"], &m["wrap"], &m["key"]].into_iter().find(|v| !v.is_null()).unwrap();
            assert_eq!(verdict, &Value::Bool(false), "{name}: once mandatory nothing gets through");
        }
    }
}

#[test]
fn the_new_root_rule() {
    let fx = fixture();
    assert_eq!(text(&fx["newRoot"]["makerRecord"]), KeyRecord::Verified.as_str());
    assert_eq!(text(&fx["newRoot"]["readerRecordBeforeResign"]["now"]), KeyRecord::Unsigned.as_str());
    assert_eq!(text(&fx["newRoot"]["readerRecordAfterResign"]), KeyRecord::Verified.as_str());
}

#[test]
fn the_store_round_trips_as_json() {
    let fx = fixture();
    let store = store_named(&fx, "victim");
    let text = serde_json::to_string(&store).unwrap();
    assert!(text.contains("\"self\":") && text.contains("\"verifiedEpoch\"") && text.contains("\"via\":\"self\""));
    assert_eq!(serde_json::from_str::<TrustStore>(&text).unwrap(), store);
}

/// #EXP-2123 `keyMaker`: the refusal, the server's waiting rule, the
/// client's step and the stranded rotation, every case by name.
#[test]
fn key_maker_every_case_by_name() {
    let fx = fixture();
    let km = &fx["keyMaker"];
    assert_eq!(text(&km["refusal"]), KEY_MAKER_REFUSAL);
    assert_eq!(text(&km["code"]), KEY_MAKER_REFUSAL_CODE);
    assert!(is_key_maker_refusal(KEY_MAKER_REFUSAL));
    assert!(!is_key_maker_refusal("Not your install"));
    assert!(!is_key_maker_refusal(""));
    for case in km["serverCases"].as_array().unwrap() {
        let b = |k: &str| case[k].as_bool().unwrap();
        assert_eq!(key_maker_waits(b("chained"), b("asked"), b("accountChain"), b("registration")), b("waits"), "{}", text(&case["name"]));
    }
    for case in km["makerCases"].as_array().unwrap() {
        let write = KeyWrite::parse(text(&case["write"])).unwrap();
        let step = key_maker_step(case["joined"].as_bool().unwrap(), write, case["refused"].as_bool().unwrap_or(false));
        assert_eq!(step.as_str(), text(&case["step"]), "{}", text(&case["name"]));
    }
    for case in km["strandedCases"].as_array().unwrap() {
        let n = |k: &str| case[k].as_u64().map(|v| v as u32);
        let b = |k: &str| case[k].as_bool().unwrap_or(false);
        let epoch = stranded_rotation_epoch(n("flagged"), n("current"), b("held"), b("stranded"), b("joined"), b("left"));
        assert_eq!(epoch, n("epoch"), "{}", text(&case["name"]));
    }
}
