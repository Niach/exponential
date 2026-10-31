//! Input methods (ibus, fcitx5): input-method-v2 for the IME, text-input-v3
//! for apps, and the virtual keyboard the IME forwards keys through.
//! Protected surfaces and the lock screen stay out of reach of all three
//! (see [`crate::protected`]).

use smithay::{
    delegate_input_method_manager,
    desktop::{PopupKind, PopupManager},
    reexports::{
        wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{self, ZwpTextInputV3},
        wayland_protocols_misc::zwp_virtual_keyboard_v1::server::zwp_virtual_keyboard_v1::{self, ZwpVirtualKeyboardV1},
        wayland_server::{protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle},
    },
    utils::{Logical, Rectangle},
    wayland::{
        input_method::{InputMethodHandler, PopupSurface},
        text_input::{TextInputManagerState, TextInputUserData},
        virtual_keyboard::{VirtualKeyboardManagerState, VirtualKeyboardUserData},
    },
};

use crate::state::Ede;

impl InputMethodHandler for Ede {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("input method popup: {e}");
        }
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|p| p.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_for_surface(parent).and_then(|w| self.space.element_geometry(&w)).unwrap_or_default()
    }
}
delegate_input_method_manager!(Ede);

// text-input-v3 through smithay, except that a protected surface (or the
// lock screen) never turns the input method on: the IME neither sees its
// text nor commits into it.
impl Dispatch<ZwpTextInputV3, TextInputUserData> for Ede {
    fn request(
        state: &mut Self,
        client: &Client,
        resource: &ZwpTextInputV3,
        request: zwp_text_input_v3::Request,
        data: &TextInputUserData,
        dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if !matches!(request, zwp_text_input_v3::Request::Destroy) && state.keyboard_focus_protected() {
            return;
        }
        <TextInputManagerState as Dispatch<ZwpTextInputV3, TextInputUserData, Self>>::request(state, client, resource, request, data, dh, data_init);
    }

    fn destroyed(state: &mut Self, client: smithay::reexports::wayland_server::backend::ClientId, resource: &ZwpTextInputV3, data: &TextInputUserData) {
        <TextInputManagerState as Dispatch<ZwpTextInputV3, TextInputUserData, Self>>::destroyed(state, client, resource, data);
    }
}

// The virtual keyboard types into whatever has focus, so its keys stop at
// protected surfaces and the lock screen (a keymap upload still goes
// through).
impl Dispatch<ZwpVirtualKeyboardV1, VirtualKeyboardUserData<Ede>> for Ede {
    fn request(
        state: &mut Self,
        client: &Client,
        resource: &ZwpVirtualKeyboardV1,
        request: zwp_virtual_keyboard_v1::Request,
        data: &VirtualKeyboardUserData<Ede>,
        dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let types = matches!(request, zwp_virtual_keyboard_v1::Request::Key { .. } | zwp_virtual_keyboard_v1::Request::Modifiers { .. });
        if types && state.keyboard_focus_protected() {
            return;
        }
        <VirtualKeyboardManagerState as Dispatch<ZwpVirtualKeyboardV1, VirtualKeyboardUserData<Ede>, Self>>::request(
            state, client, resource, request, data, dh, data_init,
        );
    }
}

smithay::reexports::wayland_server::delegate_global_dispatch!(Ede: [
    smithay::reexports::wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3: ()
] => TextInputManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(Ede: [
    smithay::reexports::wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3: ()
] => TextInputManagerState);
smithay::reexports::wayland_server::delegate_global_dispatch!(Ede: [
    smithay::reexports::wayland_protocols_misc::zwp_virtual_keyboard_v1::server::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1: smithay::wayland::virtual_keyboard::VirtualKeyboardManagerGlobalData
] => VirtualKeyboardManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(Ede: [
    smithay::reexports::wayland_protocols_misc::zwp_virtual_keyboard_v1::server::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1: ()
] => VirtualKeyboardManagerState);
