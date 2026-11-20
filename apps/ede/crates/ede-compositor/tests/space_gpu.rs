//! #EDE-162 headless: a space captured on the GPU path. CI has no GPU, so
//! the path runs on Mesa's software EGL device (llvmpipe), which spaces
//! never take for real: the same GLES composite, asynchronous readback and
//! fence as on a GPU. Without Mesa's EGL the tests skip, unless
//! `EDE_EXPECT_EGL=1` (the `ede` CI job) makes a skip a failure.
#![cfg(feature = "gpu")]

mod common;

use std::os::unix::net::UnixStream;

use common::{
    space::{start_with_spaces, wait_for},
    Session,
};
use ede_compositor::{backend::headless::Harness, desk::DeskId};
use ede_protocol::{CaptureTarget, SessionId, SpaceId};

const RED: u32 = 0xffff_0000;
const GREEN: u32 = 0xff00_ff00;
const BLUE: u32 = 0xff00_00ff;
const BLACK: u32 = 0xff00_0000;

/// A compositor whose spaces render on the software EGL device, or `None`
/// (skipped) where there is none.
fn start_on_the_gpu_path() -> Option<(Harness, tempfile::TempDir)> {
    // llvmpipe takes a thread per core in every test process otherwise.
    std::env::set_var("LP_NUM_THREADS", "2");
    let (h, dir) = start_with_spaces(&[(800, 600)]);
    match h.with(|state| state.open_space_gpu_for_tests()) {
        Ok(()) => Some((h, dir)),
        Err(e) if std::env::var("EDE_EXPECT_EGL").is_ok_and(|v| v == "1") => panic!("EDE_EXPECT_EGL=1 but no software EGL device: {e}"),
        Err(e) => {
            eprintln!("skipped: no software EGL device ({e})");
            None
        }
    }
}

fn space(h: &Harness) -> (DeskId, SpaceId, Session) {
    let (desk, id, socket) = h.with(|state| {
        let desk = state.create_space(SessionId::new("run-1").unwrap(), 640, 480).expect("a space");
        let space = state.spaces.get(desk).unwrap();
        (desk, space.id.clone(), space.socket())
    });
    (desk, id, Session::connect(UnixStream::connect(socket).expect("the space's socket")))
}

/// The GPU capture shows what the software one does, pixel for pixel
/// (opaque content), comes back asynchronously, answers `since` like the
/// software path and follows new pixels.
#[test]
fn a_space_on_the_gpu_path_captures_what_software_draws() {
    let Some((h, _dir)) = start_on_the_gpu_path() else { return };
    let (desk, id, mut b) = space(&h);
    let window = b.toplevel("b");
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&window, 64, 64, RED);
    let _sub = b.subsurface(&window.surface, (10, 10), (20, 20), GREEN);
    let mut root = h.ipc();
    let target = || CaptureTarget::Space { space: id.clone() };

    let gpu = root.capture(target()).expect("a capture on the GPU path");
    assert!(h.with(move |state| state.space_on_gpu(desk)), "the space is drawn on the GPU path");
    assert_eq!((gpu.info.width, gpu.info.height), (640, 480));
    assert_eq!((gpu.at(320, 240), gpu.at(15, 15)), (RED, GREEN));
    let software = h.with(move |state| state.capture_space(desk)).expect("a software capture");
    let software: Vec<u8> = software.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    assert!(gpu.pixels == software, "the GPU path and pixman draw the same pixels");

    assert!(root.capture_since(target(), Some(gpu.info.serial)).unwrap().is_none(), "nothing new: no pixels");
    b.draw_configured(&window, 64, 64, BLUE);
    let next = root.capture_since(target(), Some(gpu.info.serial)).unwrap().expect("new pixels");
    assert!(next.info.serial > gpu.info.serial);
    assert_eq!((next.at(320, 240), next.at(15, 15)), (BLUE, GREEN));
    assert_eq!(h.with(move |state| state.space_passes(desk)), 2);
}

/// A protected window is black on the GPU path as in software.
#[test]
fn a_protected_window_is_black_on_the_gpu_path() {
    let Some((h, _dir)) = start_on_the_gpu_path() else { return };
    let (_desk, id, mut b) = space(&h);
    let secret = b.toplevel("vault");
    secret.toplevel.set_app_id("org.keepassxc.KeePassXC".into());
    wait_for(&mut b, |b| b.state.toplevels[0].size.0 > 0);
    b.draw_configured(&secret, 64, 64, RED);
    let frame = h.ipc().capture(CaptureTarget::Space { space: id }).unwrap();
    assert_eq!(frame.at(320, 240), BLACK);
    assert!(!frame.pixels.chunks_exact(4).any(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]]) == RED));
}
