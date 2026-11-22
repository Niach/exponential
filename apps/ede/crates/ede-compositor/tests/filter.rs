//! #EDE-125 headless: the person's seat, outputs and shell globals are
//! announced to the person's clients only, and a global of another desk to
//! none of them.

mod common;

use common::Session;
use ede_compositor::{
    backend::headless::{Harness, FOREIGN_DESK},
    desk, Config, Ede,
};
use smithay::output::{Output, PhysicalProperties, Subpixel};

/// What only a client of the person's desk is told about.
const PERSONS: [&str; 8] = [
    "wl_seat",
    "wl_output",
    "zwlr_layer_shell_v1",
    "exponential_popup_namespace_v1",
    "zxdg_decoration_manager_v1",
    "ext_session_lock_manager_v1",
    "zwp_input_method_manager_v2",
    "zwp_virtual_keyboard_manager_v1",
];

fn harness() -> Harness {
    let _ = tracing_subscriber::fmt().with_test_writer().with_env_filter("warn").try_init();
    Harness::start(Config::default(), &[(800, 600)])
}

fn count(session: &Session, interface: &str) -> usize {
    session.state.globals.iter().filter(|g| *g == interface).count()
}

#[test]
fn a_client_of_another_desk_sees_no_global_of_the_person() {
    let h = harness();
    let person = Session::connect(h.connect());
    for global in PERSONS {
        assert_eq!(count(&person, global), 1, "the person's client sees {global}");
    }

    let foreign = Session::connect(h.connect_foreign());
    for global in PERSONS {
        assert_eq!(count(&foreign, global), 0, "a client of another desk sees no {global}");
    }
    // Everything else is announced to both, in the same order.
    let rest: Vec<&String> = person.state.globals.iter().filter(|g| !PERSONS.contains(&g.as_str())).collect();
    assert_eq!(foreign.state.globals.iter().collect::<Vec<_>>(), rest);
    assert!(foreign.state.seat.is_none() && foreign.state.outputs.is_empty());
}

#[test]
fn a_seat_and_an_output_of_another_desk_reach_no_client_of_the_person() {
    let h = harness();
    let mut before = Session::connect(h.connect());
    h.with(|state| {
        let dh = state.display_handle.clone();
        let mut seat = state.seat_state.new_wl_seat_with_filter(&dh, "seat-foreign", desk::on_desk(FOREIGN_DESK));
        seat.add_pointer();
        let output = Output::new(
            "FOREIGN-1".to_string(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "test".into(), serial_number: "Unknown".into() },
        );
        output.create_global_with_filter::<Ede, _>(&dh, desk::on_desk(FOREIGN_DESK));
    });

    // A client that was there before hears of neither, one that comes after
    // sees the person's one seat and one output.
    before.roundtrip();
    let mut after = Session::connect(h.connect());
    after.roundtrip();
    for person in [&before, &after] {
        assert_eq!((count(person, "wl_seat"), count(person, "wl_output")), (1, 1));
        assert_eq!(person.state.seat_names, ["seat0"]);
    }

    // The other desk's client sees exactly its own.
    let mut foreign = Session::connect(h.connect_foreign());
    foreign.roundtrip();
    assert_eq!((count(&foreign, "wl_seat"), count(&foreign, "wl_output")), (1, 1));
    assert_eq!(foreign.state.seat_names, ["seat-foreign"]);
}
