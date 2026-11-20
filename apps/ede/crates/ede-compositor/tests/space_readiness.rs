//! #EXP-1514: `space-readiness`, dark until the flip (#EXP-2011). Without
//! the dev switch this compositor says only `spaces` (as eDE 0.2.29 does)
//! and reads the op as an older compositor does: ePilot makes no space on
//! it. The dev switch (a debug build's `EDE_DEV_SPACE_READINESS=1`, here
//! `pretend_space_readiness`) says it and answers `ready`; a space grant
//! is never asked.

mod common;

use common::space::start_with_spaces;
use ede_protocol::{Capability, Error, ErrorCode, SessionId, FEATURE_SPACES, FEATURE_SPACE_READINESS};

#[test]
fn without_the_dev_switch_no_desktop_says_a_space_is_ready() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut root = h.ipc();
    assert!(root.hello.features.iter().any(|f| f == FEATURE_SPACES), "spaces, as 0.2.29 says");
    assert!(!root.hello.says_space_readiness());
    assert!(matches!(root.space_readiness(), Err(Error::Unsupported(_))), "read as an older compositor does");
    assert!(root.windows().is_ok(), "the connection goes on");
}

#[test]
fn the_dev_switch_says_it_and_answers_ready_to_a_person_scope_only() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    h.with(|state| state.pretend_space_readiness());
    let mut root = h.ipc();
    assert!(root.hello.offers_spaces() && root.hello.features.iter().any(|f| f == FEATURE_SPACE_READINESS));
    let answer = root.space_readiness().unwrap();
    assert!(answer.ready && answer.block.is_none(), "{answer:?}");
    assert_eq!(answer.max, Some(4), "no user manager here: today's four");
    // Any person-scope grant may ask, as `list_windows`.
    let watcher = root.mint(SessionId::new("watcher").unwrap(), vec![Capability::Capture], None).unwrap();
    assert!(h.ipc_with(watcher.token).unwrap().space_readiness().unwrap().ready);
    // A space grant never.
    let grant = root.mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    assert!(agent.hello.says_space_readiness());
    assert!(agent.space_readiness().is_err_and(|err| err.has_code(ErrorCode::Forbidden)));
}

#[test]
fn a_compositor_that_makes_no_spaces_says_no_readiness_whatever_the_switch() {
    let h = ede_compositor::backend::headless::Harness::start(ede_compositor::Config::default(), &[(800, 600)]);
    h.with(|state| state.pretend_space_readiness());
    let mut root = h.ipc();
    assert!(!root.hello.offers_spaces() && !root.hello.says_space_readiness());
    assert!(matches!(root.space_readiness(), Err(Error::Unsupported(_))));
}
