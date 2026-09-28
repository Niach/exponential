//! In-process: two PeerLinks (offerer/answerer, policy host, loopback candidates) connect, run the
//! bench protocol, and a tampered offer is rejected before it is applied.
use std::sync::Arc;
use std::time::Duration;

use peer_core::{IcePolicy, PeerConfig, PeerError, PeerLink};

fn cfg(id: &str) -> PeerConfig {
    // SAFETY-ish: every test sets the same value.
    unsafe { std::env::set_var("PEER_CORE_HOST_IPS", "127.0.0.1") };
    let mut c = PeerConfig::new(id, "vapp3-test");
    c.policy = IcePolicy::Host;
    c
}

#[test]
fn loopback_bench_round_trip() {
    let a = PeerLink::new(cfg("client")).unwrap();
    let b = PeerLink::new(cfg("daemon")).unwrap();
    let offer = a.create_offer().unwrap();
    let answer = b.accept_offer(&offer).unwrap();
    a.accept_answer(&answer).unwrap();
    let server = {
        let b: Arc<PeerLink> = b.clone();
        std::thread::spawn(move || b.serve_bench())
    };
    let r = a.run_bench(200_000, 10).expect("bench");
    assert!(r.connect_ms > 0.0);
    assert!(r.up_mbps > 0.0 && r.down_mbps > 0.0, "{r:?}");
    assert_eq!(r.local_path, "host");
    assert_eq!(r.remote_path, "host");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    // ICE restart keeps the channel usable.
    a.restart_ice().unwrap();
    let re = a.poll_signals(Duration::from_secs(2));
    assert_eq!(re.len(), 1);
    let ans = b.accept_offer(&re[0]).unwrap();
    a.accept_answer(&ans).unwrap();
    let ms = peer_core::bench::ping_once(&a, 999, Duration::from_secs(5)).unwrap();
    assert!(ms > 0.0);
    a.send(&[peer_core::bench::CLOSE]).unwrap();
    server.join().unwrap().unwrap();
    let d: serde_json::Value = serde_json::from_str(&a.diagnostics()).unwrap();
    assert_eq!(d["iceRestarts"], 1);
    a.close();
    b.close();
}

#[test]
fn tampered_offer_is_rejected() {
    for (sdp, sig) in [(true, false), (false, true)] {
        let mut c = cfg("evil");
        c.tamper_sdp = sdp;
        c.tamper_sig = sig;
        let a = PeerLink::new(c).unwrap();
        let b = PeerLink::new(cfg("daemon")).unwrap();
        let offer = a.create_offer().unwrap();
        match b.accept_offer(&offer) {
            Err(PeerError::SignatureRejected(r)) => assert_eq!(r, "fingerprint_signature_invalid"),
            other => panic!("expected rejection, got {other:?}"),
        }
        let sig = b.poll_signals(Duration::from_millis(100));
        assert!(sig[0].contains("\"type\":\"reject\""), "{sig:?}");
    }
    // Unexpected key.
    let a = PeerLink::new(cfg("client")).unwrap();
    let mut bc = cfg("daemon");
    bc.expected_remote_pubkey = Some("AAAA".into());
    let b = PeerLink::new(bc).unwrap();
    match b.accept_offer(&a.create_offer().unwrap()) {
        Err(PeerError::SignatureRejected(r)) => assert_eq!(r, "unexpected_peer_key"),
        other => panic!("expected rejection, got {other:?}"),
    }
}
