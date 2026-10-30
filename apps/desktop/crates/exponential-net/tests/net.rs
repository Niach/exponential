//! End-to-end tests: real nodes in one process over localhost UDP, relays
//! disabled, peers introduced by ticket.

use std::time::Duration;

use exponential_net::{CallEndReason, Clock, Event, Identity, NetConfig, NetError, Node, Record, Ticket};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(20);

async fn node() -> (Node, UnboundedReceiver<Event>) {
    let n = Node::start(NetConfig::local_only(), Identity::generate()).await.expect("node starts");
    let ev = n.events().expect("events once");
    assert!(n.events().is_none());
    (n, ev)
}

/// Introduces both ways and lets both in.
fn pair(a: &Node, b: &Node) {
    a.add_ticket(&b.ticket());
    b.add_ticket(&a.ticket());
    a.access().allow(b.endpoint_id());
    b.access().allow(a.endpoint_id());
}

async fn next_matching<T>(ev: &mut UnboundedReceiver<Event>, mut f: impl FnMut(Event) -> Option<T>) -> T {
    tokio::time::timeout(WAIT, async {
        loop {
            let e = ev.recv().await.expect("event stream open");
            if let Some(t) = f(e) {
                return t;
            }
        }
    })
    .await
    .expect("event in time")
}

#[tokio::test(flavor = "multi_thread")]
async fn ticket_round_trip() {
    let (a, _ev) = node().await;
    let t = a.ticket();
    assert!(!t.addrs.is_empty(), "localhost node has a direct address");
    let s = t.to_string();
    assert!(s.starts_with("enet1"));
    assert_eq!(s.parse::<Ticket>().unwrap(), t);
    assert_eq!(s.to_ascii_uppercase().parse::<Ticket>().unwrap(), t, "QR alphanumeric form");
    let mut with_relay = t.clone();
    with_relay.relays = vec!["https://relay.exponential.at".parse().unwrap()];
    assert_eq!(with_relay.to_string().parse::<Ticket>().unwrap(), with_relay);
    assert!("enet1!!".parse::<Ticket>().is_err());
    assert!("blob1abc".parse::<Ticket>().is_err());
    a.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_round_trip_in_order() {
    let (a, _ea) = node().await;
    let (b, mut eb) = node().await;
    pair(&a, &b);
    let mut sent = Vec::new();
    for i in 0..20u8 {
        sent.push(a.send_chat(b.endpoint_id(), vec![i; i as usize + 1]).await.expect("delivered"));
    }
    for want in &sent {
        let (from, got) = next_matching(&mut eb, |e| match e {
            Event::Chat { from, message } => Some((from, message)),
            _ => None,
        })
        .await;
        assert_eq!(from, a.endpoint_id());
        assert_eq!(&got, want);
    }
    // and back
    b.send_chat(a.endpoint_id(), b"pong".to_vec()).await.unwrap();
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_peer_is_refused() {
    let (a, _ea) = node().await;
    let (b, mut eb) = node().await;
    // a knows how to reach b, but b never allowed a
    a.add_ticket(&b.ticket());
    let err = a.send_chat(b.endpoint_id(), b"hi".to_vec()).await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    let peer = next_matching(&mut eb, |e| match e {
        Event::PeerRefused { peer, .. } => Some(peer),
        Event::Chat { .. } => panic!("refused peer delivered a message"),
        _ => None,
    })
    .await;
    assert_eq!(peer, a.endpoint_id());
    // the app's hook can let it in
    let a_id = a.endpoint_id();
    b.access().set_hook(Some(move |p, alpn: &[u8]| p == a_id && alpn == exponential_net::CHAT_ALPN));
    a.send_chat(b.endpoint_id(), b"now".to_vec()).await.expect("hook allows");
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn blob_add_and_fetch() {
    let (a, _ea) = node().await;
    let (b, _eb) = node().await;
    pair(&a, &b);
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let r = a.blob_add(data.clone()).await.unwrap();
    assert_eq!(r.size, data.len() as u64);
    assert!(r.ticket.starts_with("blob"));
    assert_eq!(a.blob_get(r.hash).await.unwrap().as_deref(), Some(&data[..]));
    assert_eq!(b.blob_get(r.hash).await.unwrap(), None);
    let got = b.blob_fetch(a.endpoint_id(), r.hash).await.unwrap();
    assert_eq!(got, data);
    assert_eq!(b.blob_get(r.hash).await.unwrap().as_deref(), Some(&data[..]));
    // by ticket, on a third node
    let (c, _ec) = node().await;
    a.access().allow(c.endpoint_id());
    assert_eq!(c.blob_fetch_ticket(&r.ticket).await.unwrap(), data);
    // unknown hash
    assert!(b.blob_fetch(a.endpoint_id(), [7u8; 32]).await.is_err());
    for n in [a, b, c] {
        n.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn records_converge_lww_and_reject_bad_signatures() {
    let space = [42u8; 32];
    let (a, _ea) = node().await;
    let (b, mut eb) = node().await;
    let (c, _ec) = node().await;
    pair(&a, &b);
    pair(&b, &c);
    pair(&a, &c);

    // a writes before anyone joined: c must get it by sync on join
    a.join_space(space, vec![]).await.unwrap();
    a.put_record(space, "title", b"from a".to_vec()).await.unwrap();
    a.put_record(space, "early", b"1".to_vec()).await.unwrap();
    b.join_space(space, vec![a.endpoint_id()]).await.unwrap();
    let rec = next_matching(&mut eb, |e| match e {
        Event::Record { record } if record.key == "early" => Some(record),
        _ => None,
    })
    .await;
    assert_eq!(rec.value, b"1");
    assert_eq!(rec.author, a.endpoint_id());

    c.join_space(space, vec![b.endpoint_id()]).await.unwrap();
    // live write from c reaches a and b over gossip
    wait_for(space, &c, "early", b"1").await;
    c.put_record(space, "title", b"from c".to_vec()).await.unwrap();
    wait_for(space, &a, "title", b"from c").await;
    wait_for(space, &b, "title", b"from c").await;

    // LWW conflict: same clock stamp, different authors -> author tie-break,
    // regardless of the order they are applied in.
    let clock = Clock { millis: u64::MAX / 2, counter: 0 };
    let ra = Record::sign(a.identity(), space, "k".into(), b"A".to_vec(), clock).unwrap();
    let rb = Record::sign(b.identity(), space, "k".into(), b"B".to_vec(), clock).unwrap();
    let winner = if ra.supersedes(&rb) { ra.clone() } else { rb.clone() };
    assert_ne!(ra.supersedes(&rb), rb.supersedes(&ra));
    a.apply_records(vec![ra.clone(), rb.clone()]).unwrap();
    b.apply_records(vec![rb.clone(), ra.clone()]).unwrap();
    assert_eq!(a.get_record(&space, "k").unwrap(), winner);
    assert_eq!(b.get_record(&space, "k").unwrap(), winner);
    // a later clock beats both
    let later = Record::sign(c.identity(), space, "k".into(), b"C".to_vec(), Clock { millis: u64::MAX / 2, counter: 1 }).unwrap();
    assert_eq!(a.apply_records(vec![later.clone()]).unwrap(), 1);
    assert_eq!(a.get_record(&space, "k").unwrap(), later);
    // older loses
    assert_eq!(a.apply_records(vec![ra.clone()]).unwrap(), 0);

    // bad signature: tampered value, forged author
    let mut forged = later.clone();
    forged.value = b"evil".to_vec();
    forged.clock.counter = 9;
    assert_eq!(a.apply_records(vec![forged]).unwrap_err(), NetError::BadSignature);
    let mut stolen = ra.clone();
    stolen.author = c.endpoint_id();
    assert_eq!(a.apply_records(vec![stolen]).unwrap_err(), NetError::BadSignature);
    assert_eq!(a.get_record(&space, "k").unwrap(), later);
    // wire round trip
    assert_eq!(Record::decode(&later.encode()).unwrap(), later);

    // a local write after observing a far-future clock still wins (HLC)
    let r = a.put_record(space, "k", b"newest".to_vec()).await.unwrap();
    assert!(r.clock > later.clock);
    wait_for(space, &c, "k", b"newest").await;

    for n in [a, b, c] {
        n.shutdown().await.unwrap();
    }
}

async fn wait_for(space: [u8; 32], n: &Node, key: &str, value: &[u8]) {
    tokio::time::timeout(WAIT, async {
        loop {
            if n.get_record(&space, key).is_some_and(|r| r.value == value) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key} did not converge on {n:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn call_frames_round_trip() {
    let (a, mut ea) = node().await;
    let (b, mut eb) = node().await;
    pair(&a, &b);
    let call = a.start_call(b.endpoint_id()).await.unwrap();
    assert!(a.send_frame(call, b"too early").await.is_err());
    let (incoming, from) = next_matching(&mut eb, |e| match e {
        Event::CallIncoming { call, from } => Some((call, from)),
        _ => None,
    })
    .await;
    assert_eq!(from, a.endpoint_id());
    b.accept_call(incoming).await.unwrap();
    next_matching(&mut ea, |e| matches!(e, Event::CallAccepted { call: c } if c == call).then_some(())).await;

    // small frames go as datagrams
    for i in 0..10u8 {
        assert_eq!(a.send_frame(call, &[i; 80]).await.unwrap(), i as u32);
    }
    // an oversize frame takes the stream fallback
    let big = vec![9u8; 20_000];
    assert_eq!(a.send_frame(call, &big).await.unwrap(), 10);
    let mut got = std::collections::BTreeMap::new();
    while got.len() < 11 {
        let (seq, data) = next_matching(&mut eb, |e| match e {
            Event::CallFrame { call: c, seq, data } if c == incoming => Some((seq, data)),
            _ => None,
        })
        .await;
        got.insert(seq, data);
    }
    assert_eq!(got[&3], vec![3u8; 80]);
    assert_eq!(got[&10], big);
    // the other direction
    b.send_frame(incoming, b"opus").await.unwrap();
    let data = next_matching(&mut ea, |e| match e {
        Event::CallFrame { call: c, data, .. } if c == call => Some(data),
        _ => None,
    })
    .await;
    assert_eq!(data, b"opus");

    b.hang_up(incoming).unwrap();
    let reason = next_matching(&mut ea, |e| match e {
        Event::CallEnded { call: c, reason } if c == call => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, CallEndReason::Remote);
    let reason = next_matching(&mut eb, |e| match e {
        Event::CallEnded { call: c, reason } if c == incoming => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, CallEndReason::Local);
    assert!(a.send_frame(call, b"x").await.is_err());
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[test]
fn push_seal_open_via_public_api() {
    let id = Identity::generate();
    let kp = exponential_net::PushKeypair::derive(&id);
    let sealed = exponential_net::push_seal(&kp.public(), b"{\"t\":\"chat\"}").unwrap();
    assert_eq!(exponential_net::push_open(&kp.secret(), &sealed).unwrap(), b"{\"t\":\"chat\"}");
    let mut bad = sealed.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(exponential_net::push_open(&kp.secret(), &bad).is_err());
}

#[test]
fn identity_bytes_round_trip() {
    let id = Identity::generate();
    let back = Identity::from_slice(&id.to_bytes()).unwrap();
    assert_eq!(back.endpoint_id(), id.endpoint_id());
    assert!(Identity::from_slice(&[1, 2, 3]).is_err());
}
