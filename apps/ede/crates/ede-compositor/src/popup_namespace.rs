//! #EXP-2041: `exponential_popup_namespace_v1` (ede-protocol's
//! `protocols/exponential-popup-namespace-v1.xml`): a popup's namespace, as
//! a layer surface has one. The shell names the taskbar's Agent spaces list,
//! a hosted Popover (an `xdg_popup` of the taskbar layer, D 11) that shows
//! each agent space's picture, [`ede_protocol::SPACE_LIST_NAMESPACE`]; every
//! capture of the person's output draws that popup black
//! ([`crate::backend::capture_elements`]), as it draws the pinned view.
//!
//! The rule is the view's (#EXP-2024): the namespace AND the process that
//! sends it, `exponential-shell` ([`crate::sealed::is_shell`]), decided once
//! when the name arrives. Another client naming a popup the same stays in
//! every capture: a name never hides an app from a capture. The person's
//! own screen draws the list as always.

use smithay::{
    desktop::{PopupKind, PopupManager},
    reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource},
    wayland::{compositor::with_states, Dispatch2, GlobalDispatch2},
};

use crate::state::Ede;

#[allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables, non_upper_case_globals, non_snake_case, unused_imports, missing_docs, clippy::all)]
pub mod protocol {
    use smithay::reexports::wayland_server;
    use smithay::reexports::wayland_server::protocol::*;

    pub mod __interfaces {
        use smithay::reexports::wayland_server::backend as wayland_backend;
        use smithay::reexports::wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../../../desktop/crates/ede-protocol/protocols/exponential-popup-namespace-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_server_code!("../../../desktop/crates/ede-protocol/protocols/exponential-popup-namespace-v1.xml");
}

use protocol::exponential_popup_namespace_v1::{ExponentialPopupNamespaceV1, Request};

/// The global's version.
const VERSION: u32 = 1;

/// A surface's name, decided once when it arrived.
#[derive(Debug, Clone)]
pub struct PopupName {
    pub namespace: String,
    /// The shell named it the Agent spaces list: black in captures.
    pub space_list: bool,
}

/// The global's data: announced to the person's clients only, as the
/// layer shell is (a space's clients have no shell popups to name).
pub struct NamespaceGlobal;

/// A bound manager's data.
pub struct NamespaceManager;

impl GlobalDispatch2<ExponentialPopupNamespaceV1, Ede> for NamespaceGlobal {
    fn bind(&self, _state: &mut Ede, _handle: &DisplayHandle, _client: &Client, resource: New<ExponentialPopupNamespaceV1>, data_init: &mut DataInit<'_, Ede>) {
        data_init.init(resource, NamespaceManager);
    }

    fn can_view(&self, client: &Client) -> bool {
        crate::desk::on_person(client)
    }
}

impl Dispatch2<ExponentialPopupNamespaceV1, Ede> for NamespaceManager {
    fn request(&self, state: &mut Ede, client: &Client, _resource: &ExponentialPopupNamespaceV1, request: Request, dhandle: &DisplayHandle, _data_init: &mut DataInit<'_, Ede>) {
        match request {
            Request::SetNamespace { surface, namespace } => {
                let pid = client.get_credentials(dhandle).map(|c| c.pid as u32).unwrap_or(0);
                let space_list = namespace == ede_protocol::SPACE_LIST_NAMESPACE && crate::sealed::is_shell(state, client);
                let named = with_states(&surface, |s| s.data_map.insert_if_missing_threadsafe(|| PopupName { namespace: namespace.clone(), space_list }));
                if named && space_list {
                    tracing::info!(pid, surface = surface.id().protocol_id(), "the Agent spaces list: black in captures of the person's output");
                }
            }
            Request::Destroy => {}
        }
    }
}

/// `popup` (or a popup it sits in) is the shell's Agent spaces list: black
/// in every capture of the person's output.
pub fn black_in_captures(popups: &PopupManager, popup: &PopupKind) -> bool {
    let mut at = Some(popup.clone());
    while let Some(p) = at {
        if with_states(p.wl_surface(), |s| s.data_map.get::<PopupName>().is_some_and(|n| n.space_list)) {
            return true;
        }
        // An xdg popup's parent: a layer (the walk ends) or another popup.
        let PopupKind::Xdg(xdg) = &p else { break };
        at = xdg.get_parent_surface().and_then(|parent| popups.find_popup(&parent));
    }
    false
}

/// Announces `exponential_popup_namespace_v1` ([`Ede::new`]).
pub fn create_global(dh: &DisplayHandle) {
    dh.create_global::<Ede, ExponentialPopupNamespaceV1, _>(VERSION, NamespaceGlobal);
}
