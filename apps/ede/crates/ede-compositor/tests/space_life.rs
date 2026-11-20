//! #EXP-1514 (5-life): the compositor's side of a space that lives as long
//! as its session. Expired grants leave the list in the 1 s tick; a wall
//! clock that jumps ahead (a suspend) gives every space one grant's life
//! of grace; a full desktop gives way: the space unused longest, neither in
//! use nor busy, closes for a new one, and `limit` only when none may.

mod common;

use std::time::Duration;

use common::space::{spaces_settled, start_with_spaces};
use ede_compositor::backend::headless::Harness;
use ede_protocol::{Capability, Client, Error, ErrorCode, InputEvent, Request, SessionId};

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn space_grant(h: &Harness, session: &str, ms: Option<i64>) -> Client {
    let grant = h.ipc().mint(SessionId::new(session).unwrap(), vec![Capability::Space], ms.map(|ms| now_ms() + ms)).unwrap();
    h.ipc_with(grant.token).unwrap()
}

fn code<T: std::fmt::Debug>(reply: Result<T, Error>) -> ErrorCode {
    match reply {
        Err(Error::Failed(f)) => f.code,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn listed(h: &Harness) -> Vec<String> {
    h.ipc().spaces().unwrap().iter().map(|s| s.session_id.as_str().to_string()).collect()
}

fn tick(h: &Harness, n: u32) {
    h.with(move |state| (0..n).for_each(|_| state.spaces_tick()));
}

/// After a grant's life the list holds only live grants: the root's, the
/// ones without an expiry and the ones still in time.
#[test]
fn expired_grants_leave_the_list_in_the_tick() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    let mint = |root: &mut Client, session: &str, capabilities: Vec<Capability>, ms: Option<i64>| root.mint(SessionId::new(session).unwrap(), capabilities, ms.map(|ms| now_ms() + ms)).unwrap();
    for n in 0..3 {
        mint(&mut root, &format!("run-{n}"), vec![Capability::Space], Some(100));
        mint(&mut root, &format!("person-{n}"), vec![Capability::Capture], Some(100));
    }
    let kept = mint(&mut root, "epilot", vec![Capability::Capture], None);
    let live = mint(&mut root, "run-live", vec![Capability::Space], Some(60_000));
    let before = h.with(|state| state.ipc.grants.len());
    std::thread::sleep(Duration::from_millis(150));
    tick(&h, 1);
    let tokens = h.with(|state| state.ipc.grants.iter().map(|g| g.token.clone()).collect::<Vec<_>>());
    assert_eq!(before - tokens.len(), 6, "every expired grant went");
    assert!(tokens.contains(&kept.token) && tokens.contains(&live.token));
    assert!(root.windows().is_ok(), "the root's stays");
}

/// A suspend longer than a grant's life: every grant expired at once.
/// The tick sees the wall clock ahead of its own and keeps every space for
/// one grant's life, so a keeper that renews in it keeps its space; one
/// that does not loses it once the grace is over.
#[test]
fn a_clock_jump_keeps_a_space_whose_keeper_renews_within_one_life() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    h.with(|state| state.spaces.expiry_grace = 3);
    for session in ["run-kept", "run-gone"] {
        space_grant(&h, session, Some(100)).create_space(640, 480).unwrap();
    }
    std::thread::sleep(Duration::from_millis(150));
    // The tick before the suspend, two hours of wall clock ago.
    tick(&h, 1);
    h.with(|state| state.spaces.clock = Some((std::time::Instant::now(), now_ms() - 2 * 3600 * 1000)));
    tick(&h, 10);
    assert_eq!(listed(&h), ["run-kept", "run-gone"], "no space expires in the grace");
    // The keeper renews within the grace; the other session's never does.
    let mut renewed = space_grant(&h, "run-kept", Some(60_000));
    tick(&h, 2);
    // The grace is over.
    h.with(|state| state.spaces.grace_until = Some(now_ms()));
    tick(&h, 3);
    assert_eq!(listed(&h), ["run-kept"]);
    assert_eq!(renewed.spaces().unwrap().len(), 1, "the renewed one drives its same space");
    // A tick on time is no jump.
    tick(&h, 1);
    assert!(h.with(|state| state.spaces.grace_until.is_some_and(|until| until <= now_ms())));
    spaces_settled(&h);
}

/// `space_busy` is a space grant's word for its own space: the person's
/// side is refused it, a space grant without a space finds none.
#[test]
fn space_busy_is_a_space_grants_own() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    assert_eq!(code(root.call(Request::SpaceBusy)), ErrorCode::Forbidden);
    let watcher = root.mint(SessionId::new("watcher").unwrap(), vec![Capability::Capture], None).unwrap();
    assert_eq!(code(h.ipc_with(watcher.token).unwrap().space_busy()), ErrorCode::Forbidden);
    let mut agent = space_grant(&h, "run-1", None);
    assert_eq!(code(agent.space_busy()), ErrorCode::NotFound);
    agent.create_space(640, 480).unwrap();
    agent.space_busy().unwrap();
}

/// D 10 = B: a full desktop (four here: no slices) closes the space unused
/// longest that is neither in use (driven in the window) nor busy (its
/// keeper said `space_busy`) for a new session's; a session's own space
/// comes back before any count; with every space in use or busy the
/// answer is `limit` and none closes. Only the caller's space is busy.
#[test]
fn a_full_desktop_gives_way_to_a_new_space() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let window = Duration::from_millis(1500);
    h.with(move |state| state.spaces.in_use_for = window);
    let mut agents: Vec<Client> = (1..=4)
        .map(|n| {
            let mut agent = space_grant(&h, &format!("run-{n}"), None);
            agent.create_space(640, 480).unwrap();
            agent
        })
        .collect();
    std::thread::sleep(window + Duration::from_millis(50));
    // run-2 is driven, run-3's turn runs: neither gives way.
    let drive = |agent: &mut Client| agent.input(InputEvent::PointerMotion { x: 10.0, y: 10.0 }).unwrap();
    drive(&mut agents[1]);
    agents[2].space_busy().unwrap();
    // A session's own space first, whatever the count.
    assert_eq!(agents[0].create_space(640, 480).unwrap().id.as_str(), "space-1");
    let mut fifth = space_grant(&h, "run-5", None);
    assert_eq!(fifth.create_space(640, 480).unwrap().id.as_str(), "space-5");
    assert_eq!(listed(&h), ["run-2", "run-3", "run-4", "run-5"], "the space unused longest gave way");
    drive(&mut agents[1]);
    let mut sixth = space_grant(&h, "run-6", None);
    sixth.create_space(640, 480).unwrap();
    assert_eq!(listed(&h), ["run-2", "run-3", "run-5", "run-6"], "run-5 and run-6 are new: in use from their creation");
    // Every space in use or busy: `limit`, and nothing closes.
    drive(&mut agents[1]);
    let mut seventh = space_grant(&h, "run-7", None);
    assert_eq!(code(seventh.create_space(640, 480)), ErrorCode::Limit);
    assert_eq!(listed(&h), ["run-2", "run-3", "run-5", "run-6"]);
    // Out of the window: the session whose space gave way gets a new one,
    // and the one unused longest (run-5, never driven) gives way for it;
    // run-3 is busy still.
    h.with(|state| state.spaces.in_use_for = Duration::ZERO);
    assert_eq!(agents[0].create_space(640, 480).unwrap().id.as_str(), "space-7");
    assert_eq!(listed(&h), ["run-2", "run-3", "run-6", "run-1"]);
    spaces_settled(&h);
}
