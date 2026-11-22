//! Exponential patch (#EXP-2041): `exponential_popup_namespace_v1`, the
//! client side of Exponential Desktop's popup names
//! (`apps/desktop/crates/ede-protocol/protocols/exponential-popup-namespace-v1.xml`).
//! An xdg_popup has no name; eDE's compositor reads this one, as it reads a
//! layer surface's namespace, and keeps the shell's Agent spaces list out of
//! captures of the person's output. Every other compositor lacks the global,
//! and nothing is sent.

#![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables, non_upper_case_globals, non_snake_case, unused_imports, missing_docs, clippy::all)]

use wayland_client;
use wayland_client::protocol::*;

pub mod __interfaces {
    use wayland_client::backend as wayland_backend;
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("../../crates/ede-protocol/protocols/exponential-popup-namespace-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("../../crates/ede-protocol/protocols/exponential-popup-namespace-v1.xml");
