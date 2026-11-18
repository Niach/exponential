//! #EDE-125 headless: what a closed space leaves on disk.

mod common;

use common::space::{spaces_settled, start_with_spaces};
use ede_protocol::SessionId;

/// What ran in a space can keep its directory busy at the close (a FUSE
/// mount that goes a moment later): the removal is tried again on the
/// tick until the directory is gone, and given up after a while.
#[test]
fn a_busy_directory_is_removed_later() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static BUSY: AtomicU32 = AtomicU32::new(0);
    fn busy(dir: &std::path::Path) -> std::io::Result<()> {
        if BUSY.load(Ordering::SeqCst) > 0 {
            BUSY.fetch_sub(1, Ordering::SeqCst);
            return Err(std::io::ErrorKind::ResourceBusy.into());
        }
        std::fs::remove_dir_all(dir)
    }

    let (h, dir) = start_with_spaces(&[(800, 600)]);
    let owner = |name: &str| SessionId::new(name).unwrap();
    h.with(|state| state.spaces.remove_dir = busy);
    let close_one = |busy_for: u32| {
        let own = h.with(move |state| {
            let desk = state.create_space(owner("run-1"), 640, 480).expect("a space");
            let own = state.spaces.get(desk).unwrap().socket().parent().unwrap().to_path_buf();
            std::fs::write(own.join("left-by-an-app"), b"x").unwrap();
            BUSY.store(busy_for, Ordering::SeqCst);
            assert!(state.close_space(desk));
            own
        });
        assert_eq!(own.parent(), Some(dir.path().join("wayland-test.spaces").as_path()));
        own
    };
    let tick = |n: u32| h.with(move |state| (0..n).for_each(|_| state.spaces_tick()));

    // Busy at the close and at the next try: it stays, and goes with the try after.
    let own = close_one(2);
    assert!(own.exists() && !h.with(|state| state.spaces.settled()));
    tick(1);
    assert!(own.exists());
    tick(1);
    assert!(!own.exists());
    spaces_settled(&h);

    // Busy for good: tried for a while, then given up; the compositor's
    // end does not find it in its way.
    let own = close_one(u32::MAX);
    tick(ede_compositor::spaces::DIR_RETRIES + 1);
    assert!(own.exists());
    assert_eq!(BUSY.load(Ordering::SeqCst), u32::MAX - ede_compositor::spaces::DIR_RETRIES - 1, "one try at the close, then one per tick, and no more");
    spaces_settled(&h);
    BUSY.store(0, Ordering::SeqCst);
}
