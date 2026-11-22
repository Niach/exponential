//! #EDE-162 headless: a space's apps present at most once per tick of its
//! clock. A burst of new buffers (an app with vsync off) is held: one goes
//! through per tick, at most two wait, the newest wins. The person's apps
//! are never held.

mod common;

use std::{os::unix::net::UnixStream, time::Duration};

use common::{space::wait_for, space::start_with_spaces, Session};
use ede_compositor::{backend::headless::Harness, desk::DeskId, spaces::frames::pace::HELD_MAX};
use ede_protocol::SessionId;

const COLORS: [u32; 5] = [0xff10_0000, 0xff20_0000, 0xff30_0000, 0xff40_0000, 0xff50_0000];

fn space_with_a_window(h: &Harness) -> (DeskId, Session, common::Toplevel, (i32, i32)) {
    let (desk, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    });
    let mut b = Session::connect(UnixStream::connect(socket).expect("the space's socket"));
    let window = b.toplevel("b");
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    let size = b.state.toplevels[0].size;
    b.draw(&window, size.0, size.1, 0xffff_ffff);
    (desk, b, window, size)
}

fn settled(h: &Harness, desk: DeskId) -> (u64, u64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (scheduled, ticks, serial, held) = h.with(move |state| {
            let s = state.spaces.get(desk).unwrap();
            (s.scheduled, s.ticks, s.serial, state.space_held_commits(desk))
        });
        if !scheduled && held == 0 {
            return (ticks, serial);
        }
        assert!(std::time::Instant::now() < deadline, "the space's clock never rests");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_burst_of_buffers_goes_through_one_per_tick_and_the_newest_wins() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, mut b, window, (w, hh)) = space_with_a_window(&h);
    let (ticks, serial) = settled(&h, desk);

    b.draw_burst(&window, w, hh, &COLORS);
    // The first went through, two wait, the two between were dropped.
    assert_eq!(h.with(move |state| state.space_held_commits(desk)), HELD_MAX);
    assert_eq!(h.with(move |state| state.capture_space(desk)).unwrap().at(320, 240), COLORS[0]);

    let (after, moved) = settled(&h, desk);
    assert_eq!(h.with(move |state| state.capture_space(desk)).unwrap().at(320, 240), COLORS[4], "the newest is shown");
    assert!(after - ticks >= HELD_MAX as u64, "one held commit per tick: {} ticks", after - ticks);
    assert!(moved - serial <= after - ticks, "at most one new frame per tick: serial +{} in {} ticks", moved - serial, after - ticks);
    assert_eq!(h.with(|state| state.spaces_paced_surfaces()), 0, "a settled surface is no longer listed");
}

#[test]
fn a_space_closed_while_its_commits_wait_is_forgotten() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let (desk, mut b, window, (w, hh)) = space_with_a_window(&h);
    settled(&h, desk);
    b.draw_burst(&window, w, hh, &COLORS);
    assert_eq!(h.with(move |state| state.space_held_commits(desk)), HELD_MAX);
    assert!(h.with(move |state| state.close_space(desk)));
    drop(b);

    // Another space's ticks forget it.
    let (other, mut c, window, (w, hh)) = space_with_a_window(&h);
    settled(&h, other);
    c.draw_burst(&window, w, hh, &COLORS);
    settled(&h, other);
    assert_eq!(h.with(|state| state.spaces_paced_surfaces()), 0);
}

#[test]
fn the_persons_apps_are_never_held() {
    let (h, _dir) = start_with_spaces(&[(800, 600)]);
    let mut a = Session::connect(h.connect());
    let window = a.toplevel("a");
    a.draw(&window, 200, 150, 0xffff_ffff);
    a.draw_burst(&window, 200, 150, &COLORS);
    let (x, y) = h.with(|state| {
        let w = state.desk.space.elements().next().cloned().unwrap();
        let g = state.desk.space.element_geometry(&w).unwrap();
        (g.loc.x + 10, g.loc.y + 10)
    });
    assert_eq!(h.capture(0).at(x, y), COLORS[4], "every commit of the person's applied at once");
}
