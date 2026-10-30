//! The facade from Rust: what the Swift/Kotlin hosts do, over localhost.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use exponential_net_ffi::*;

fn local_config() -> EnetConfig {
    EnetConfig { relays_enabled: false, bind_addr: Some("127.0.0.1:0".into()), ..default_config() }
}

async fn pair() -> (Arc<EnetNode>, Arc<EnetNode>) {
    let a = EnetNode::start(local_config(), generate_identity()).await.unwrap();
    let b = EnetNode::start(local_config(), generate_identity()).await.unwrap();
    assert_eq!(a.add_ticket(b.ticket()).unwrap(), b.endpoint_id());
    assert_eq!(b.add_ticket(a.ticket()).unwrap(), a.endpoint_id());
    a.allow_peer(b.endpoint_id()).unwrap();
    b.allow_peer(a.endpoint_id()).unwrap();
    (a, b)
}

async fn next(n: &EnetNode) -> EnetEvent {
    tokio::time::timeout(Duration::from_secs(20), n.next_event()).await.expect("event in time").expect("node alive")
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_records_calls_through_the_facade() {
    let (a, b) = pair().await;
    let sent = a.send_chat(b.endpoint_id(), b"hi".to_vec()).await.unwrap();
    match next(&b).await {
        EnetEvent::Chat { from, message } => {
            assert_eq!(from, a.endpoint_id());
            assert_eq!(message.body, b"hi");
            assert_eq!(message.id, sent.id);
        }
        e => panic!("{e:?}"),
    }

    let space = vec![5u8; 32];
    a.join_space(space.clone(), vec![]).await.unwrap();
    a.put_record(space.clone(), "k".into(), b"v".to_vec()).await.unwrap();
    b.join_space(space.clone(), vec![a.endpoint_id()]).await.unwrap();
    match next(&b).await {
        EnetEvent::Record { record } => {
            assert_eq!((record.key.as_str(), record.value.as_slice()), ("k", &b"v"[..]));
            assert_eq!(record.author, a.endpoint_id());
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(b.records(space.clone()).unwrap().len(), 1);
    assert!(b.join_space(vec![1, 2, 3], vec![]).await.is_err());

    let blob = a.blob_add(vec![1, 2, 3]).await.unwrap();
    assert_eq!(b.blob_fetch(a.endpoint_id(), blob.hash.clone()).await.unwrap(), vec![1, 2, 3]);

    let call = a.start_call(b.endpoint_id()).await.unwrap();
    let incoming = match next(&b).await {
        EnetEvent::CallIncoming { call, from } => {
            assert_eq!(from, a.endpoint_id());
            call
        }
        e => panic!("{e:?}"),
    };
    b.accept_call(incoming).await.unwrap();
    assert!(matches!(next(&a).await, EnetEvent::CallAccepted { call: c } if c == call));
    a.send_frame(call, b"opus".to_vec()).await.unwrap();
    match next(&b).await {
        EnetEvent::CallFrame { seq, data, .. } => assert_eq!((seq, data.as_slice()), (0, &b"opus"[..])),
        e => panic!("{e:?}"),
    }
    a.hang_up(call).unwrap();
    assert!(matches!(next(&b).await, EnetEvent::CallEnded { reason, .. } if reason == "remote"));
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

struct Collect(Mutex<Vec<EnetEvent>>);
impl EnetListener for Collect {
    fn on_event(&self, event: EnetEvent) {
        self.0.lock().unwrap().push(event);
    }
}

struct Deny;
impl EnetAccessPolicy for Deny {
    fn allow(&self, _peer: String, _alpn: String) -> bool {
        false
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn listener_and_refusal() {
    let a = EnetNode::start(local_config(), generate_identity()).await.unwrap();
    let b = EnetNode::start(local_config(), generate_identity()).await.unwrap();
    a.add_ticket(b.ticket()).unwrap();
    let got = Arc::new(Collect(Mutex::new(Vec::new())));
    b.set_listener(Some(got.clone()));
    b.set_access_policy(Some(Arc::new(Deny)));
    assert!(matches!(a.send_chat(b.endpoint_id(), b"x".to_vec()).await, Err(EnetError::Refused { .. })));
    b.set_access_policy(None);
    b.set_open(true);
    a.send_chat(b.endpoint_id(), b"y".to_vec()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if got.0.lock().unwrap().iter().any(|e| matches!(e, EnetEvent::Chat { message, .. } if message.body == b"y")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(got.0.lock().unwrap().iter().any(|e| matches!(e, EnetEvent::PeerRefused { peer, .. } if *peer == a.endpoint_id())));
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[test]
fn free_functions() {
    let id = generate_identity();
    assert_eq!(id.len(), 32);
    let kp = push_keypair_derive(id.clone()).unwrap();
    let sealed = push_seal(kp.public.clone(), b"wake".to_vec()).unwrap();
    assert_eq!(push_open(kp.secret.clone(), sealed.clone()).unwrap(), b"wake");
    assert!(matches!(push_open(push_keypair_generate().secret, sealed), Err(EnetError::Crypto { .. })));
    assert!(push_open(vec![0; 3], vec![]).is_err());
    assert!(endpoint_id_of(id).is_ok());
    assert!(parse_ticket("nope".into()).is_err());
}

#[test]
fn ticket_parses() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let n = EnetNode::start(local_config(), generate_identity()).await.unwrap();
        let t = parse_ticket(n.ticket()).unwrap();
        assert_eq!(t.endpoint_id, n.endpoint_id());
        assert!(t.addrs.iter().any(|a| a.starts_with("127.0.0.1:")));
        n.shutdown().await.unwrap();
    });
}
