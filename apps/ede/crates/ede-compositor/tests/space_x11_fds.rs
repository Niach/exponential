//! #EDE-138 headless, with Xwayland: a space's Xwayland leaves nothing in
//! the compositor once its space closed. Its own test binary: it counts
//! the process's event fds, which tests running beside it would move.
#![cfg(feature = "xwayland")]

mod common;

use std::time::{Duration, Instant};

use common::space::{spaces_settled, start_with_spaces};
use ede_compositor::desk::DeskId;
use ede_protocol::{Capability, SessionId};

/// The event fds this process holds (calloop's pings and channels).
fn eventfds() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().flatten().filter(|fd| std::fs::read_link(fd.path()).is_ok_and(|link| link.as_os_str() == "anon_inode:[eventfd]")).count()
}

/// Three spaces, one after the other, each with an Xwayland of its own
/// that ran a window manager: once each is closed and settled, the
/// compositor holds as many event fds as before the first (smithay's
/// window manager left its focus release's source in the loop, one fd per
/// server ever started).
#[test]
fn a_closed_spaces_xwayland_leaves_no_event_source_behind() {
    if std::process::Command::new("Xwayland").arg("-version").output().is_err() {
        eprintln!("skipped: no Xwayland binary");
        return;
    }
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    h.with(|state| state.spaces.x11 = true);
    let grant = h.ipc().mint(SessionId::new("run-1").unwrap(), vec![Capability::Space], None).unwrap();
    let mut agent = h.ipc_with(grant.token).unwrap();
    let mut before: Option<usize> = None;
    for round in 0..3 {
        let space = agent.create_space(640, 480).unwrap();
        let desk = DeskId::from_space_id(&space.id).unwrap();
        let out = dir.path().join(format!("{round}.display"));
        agent.launch(vec!["sh".into(), "-c".into(), format!("echo \"$DISPLAY\" > {0}.tmp && mv {0}.tmp {0}; exec sleep 60", out.display())], Default::default(), None).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !h.with(move |state| state.space_xwm_ready(desk)) || !out.exists() {
            assert!(Instant::now() < deadline, "the space's Xwayland never became ready");
            std::thread::sleep(Duration::from_millis(20));
        }
        agent.destroy_space(space.id.clone()).unwrap();
        spaces_settled(&h);
        // The count to come back to: after the first space, so what any
        // first Xwayland starts for good is in it.
        let Some(at) = before else {
            std::thread::sleep(Duration::from_millis(300));
            before = Some(eventfds());
            continue;
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while eventfds() > at {
            assert!(Instant::now() < deadline, "round {round}: {} event fds, {at} before", eventfds());
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
