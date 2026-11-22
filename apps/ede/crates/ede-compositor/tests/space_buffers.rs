//! #EDE-162 headless: GPU buffers for a space's clients and only for them.
//! A client of a space that draws on the GPU sees the spaces' own
//! linux-dmabuf global; the person's clients (headless: their output is
//! drawn in software), a space in software, a client of no desk and the new
//! clients of a space that latched back to software do not. A buffer the
//! spaces' renderer refuses latches its space. On CI
//! the "GPU" is Mesa's software EGL device (as in `space_gpu.rs`).
#![cfg(feature = "gpu")]

mod common;

use std::os::{fd::AsFd, unix::net::UnixStream};

use common::{space::start_with_spaces, Session};
use ede_compositor::{backend::headless::Harness, desk::DeskId, spaces::frames::IMPORTS_PER_SECOND};
use ede_protocol::SessionId;
use wayland_client::{
    protocol::{wl_buffer, wl_registry},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1};

const DMABUF: &str = "zwp_linux_dmabuf_v1";

fn start_on_the_gpu_path() -> Option<(Harness, tempfile::TempDir)> {
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

fn space(h: &Harness, session: &str) -> (DeskId, std::path::PathBuf) {
    let session = SessionId::new(session).unwrap();
    h.with(move |state| {
        let desk = state.create_space(session, 640, 480).expect("a space");
        (desk, state.spaces.get(desk).unwrap().socket())
    })
}

fn sees_dmabuf(stream: UnixStream) -> bool {
    Session::connect(stream).state.globals.iter().any(|g| g == DMABUF)
}

#[test]
fn only_a_gpu_spaces_clients_see_its_gpu_buffers() {
    // No GPU renderer: a space's clients get wl_shm only, as before F7.
    let (soft, _d) = start_with_spaces(&[(800, 600)]);
    let (_, socket) = space(&soft, "run-1");
    assert!(!sees_dmabuf(UnixStream::connect(&socket).unwrap()), "a space in software offers no GPU buffers");

    let Some((h, _dir)) = start_on_the_gpu_path() else { return };
    assert!(!sees_dmabuf(h.connect()), "the headless person draws in software: no GPU buffers");
    assert!(!sees_dmabuf(h.connect_foreign()), "a client of no desk sees nothing of a space");
    let (desk, socket) = space(&h, "run-1");
    assert!(sees_dmabuf(UnixStream::connect(&socket).unwrap()), "a GPU space's client sees the spaces' linux-dmabuf");
    assert!(h.with(move |state| state.space_gpu_allowed(desk)));
}

/// A client that asks for one dmabuf the spaces' renderer cannot import
/// (a plain file, not a GPU buffer): the answer it gets.
#[derive(Default)]
struct Asker {
    dmabuf: Option<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1>,
    pair: Option<(u32, u64)>,
    answer: Option<bool>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Asker {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            if interface == DMABUF {
                state.dmabuf = Some(registry.bind(name, 3, qh, ()));
            }
        }
    }
}

impl Dispatch<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, ()> for Asker {
    fn event(state: &mut Self, _: &zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, event: zwp_linux_dmabuf_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwp_linux_dmabuf_v1::Event::Modifier { format, modifier_hi, modifier_lo } = event {
            state.pair.get_or_insert((format, (modifier_hi as u64) << 32 | modifier_lo as u64));
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, ()> for Asker {
    fn event(state: &mut Self, _: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, event: zwp_linux_buffer_params_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwp_linux_buffer_params_v1::Event::Created { .. } => state.answer = Some(true),
            zwp_linux_buffer_params_v1::Event::Failed => state.answer = Some(false),
            _ => {}
        }
    }

    wayland_client::event_created_child!(Asker, zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, [
        zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (wl_buffer::WlBuffer, ()),
    ]);
}

impl Dispatch<wl_buffer::WlBuffer, ()> for Asker {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

/// Every GPU buffer is imported (so validated) on the spaces' renderer, on
/// the person's thread: a client that makes more than
/// [`IMPORTS_PER_SECOND`] a second is refused and its space latches back
/// to software. (Mesa's software device imports any mappable file, so here
/// the budget is what refuses; a GPU refuses what it cannot draw the same way.)
#[test]
fn a_client_that_floods_gpu_buffers_is_refused_and_latches_its_space() {
    let Some((h, _dir)) = start_on_the_gpu_path() else { return };
    let (desk, socket) = space(&h, "run-1");
    let conn = Connection::from_socket(UnixStream::connect(&socket).unwrap()).unwrap();
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut asker = Asker::default();
    queue.roundtrip(&mut asker).unwrap();
    queue.roundtrip(&mut asker).unwrap();
    let dmabuf = asker.dmabuf.clone().expect("a GPU space's client sees the spaces' linux-dmabuf");
    let (format, modifier) = asker.pair.expect("it names a format and modifier");

    let (w, hh) = (64u32, 64u32);
    let file = tempfile::tempfile().unwrap();
    file.set_len((w * hh * 4) as u64).unwrap();
    let mut answers = Vec::new();
    for _ in 0..=IMPORTS_PER_SECOND {
        asker.answer = None;
        let params = dmabuf.create_params(&qh, ());
        params.add(file.as_fd(), 0, 0, w * 4, (modifier >> 32) as u32, modifier as u32);
        params.create(w as i32, hh as i32, format, zwp_linux_buffer_params_v1::Flags::empty());
        while asker.answer.is_none() {
            queue.blocking_dispatch(&mut asker).unwrap();
        }
        answers.push(asker.answer.unwrap());
    }
    assert!(answers[..IMPORTS_PER_SECOND].iter().all(|ok| *ok), "within the budget every buffer imports");
    assert!(!answers[IMPORTS_PER_SECOND], "one more in the same second is refused");

    let why = h.with(move |state| state.space_latched(desk).map(str::to_string)).expect("the space latched");
    assert!(why.contains("GPU buffers a second"), "{why}");
    assert!(!sees_dmabuf(UnixStream::connect(&socket).unwrap()), "its new clients draw in software");
}
