//! NEW-59 probe: what the work-screen rules read on this machine now.
//! `cargo run -p computer --example work_screen [-- PICKED]` prints the
//! displays, the fullscreen ones and the work displays (PICKED = the 1-based
//! Work screen setting; absent = Auto). Make a window fullscreen and run it
//! again: its display leaves the work list.

fn main() {
    let picked = std::env::args().nth(1).and_then(|arg| arg.parse().ok());
    computer::set_work_display(picked);
    println!("readiness: {:?}", computer::readiness());
    println!("picked: {picked:?}");
    for display in computer::displays() {
        println!("display: {display:?} pixels {:?}", computer::pixel_size(&display));
    }
    match computer::screen_state() {
        Some(state) => println!("fullscreen: {:?}\nwork: {:?}", state.fullscreen, state.work),
        None => println!("no screen state (no display)"),
    }
}
