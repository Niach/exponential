//! #EXP-1629: the MCP server add / edit dialog's content in every state, at
//! the dialog window's content size (460 less its 16 px padding; 540 high
//! less the bottom padding).
use super::*;
use gpui::Entity;

use crate::settings::mcp_server_dialog::{McpDialogShot, McpServerDialogView};

const W: f32 = 428.;
const H: f32 = 524.;
/// A window dragged smaller than the form (the rows scroll under the footer).
const H_SHORT: f32 = 300.;

struct Column {
    content: Entity<McpServerDialogView>,
    height: f32,
}

impl Render for Column {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(W)).h(px(self.height)).child(self.content.clone())
    }
}

fn add() -> McpDialogShot {
    McpDialogShot { enabled: true, ..Default::default() }
}

fn filled() -> McpDialogShot {
    McpDialogShot { name: "linear", url: "https://mcp.linear.app/mcp", ..add() }
}

fn command() -> McpDialogShot {
    McpDialogShot { name: "filesystem", transport: "stdio", command: "npx", args: "-y @acme/mcp /srv/shared", advanced: true, ..add() }
}

/// The Runs on row's value, Sign-in's and Transport's (window points), read
/// off the pictures.
const RUNS_ON: (f32, f32) = (425., 128.);
const SIGN_IN: (f32, f32) = (425., 265.);
const TRANSPORT: (f32, f32) = (425., 307.);
/// The Advanced line's empty end (no glyph, no text under it).
const ADVANCED_END: (f32, f32) = (300., 231.);

/// The wheel over the form, as the platform sends it.
fn wheel(window: &mut Window, cx: &mut gpui::App, dy: f32) {
    let position = gpui::point(px(200.), px(150.));
    let moved = gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() };
    window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
    let event = gpui::ScrollEvent { position, delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))), modifiers: Default::default(), touch_phase: gpui::TouchPhase::Moved, source: Default::default() };
    window.dispatch_event(gpui::PlatformInput::Scroll(event), cx);
}

fn shoot(name: &str, height: f32, shot: McpDialogShot, clicks: &'static [(f32, f32)], settle: Option<u64>) {
    shoot_pane_acted(
        name,
        move |window, cx| {
            let content = cx.new(|cx| {
                let mut view = McpServerDialogView::for_shots(window, cx);
                view.show(shot, window, cx);
                view
            });
            Column { content, height }
        },
        |_, _| {},
        settle.map(std::time::Duration::from_millis),
        clicks,
        |_, _, _| {},
    );
}

#[test]
#[ignore]
fn settings_mcp_dialog_add_http_empty() {
    shoot("mcp-dialog-add-http-empty", H, add(), &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_add_http_filled() {
    shoot("mcp-dialog-add-http-filled", H, filled(), &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_add_stdio() {
    shoot("mcp-dialog-add-stdio", H, command(), &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_edit() {
    shoot("mcp-dialog-edit", H, McpDialogShot { edit: true, device: Some("d1"), enabled: false, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_secret_names() {
    shoot("mcp-dialog-secret-names", H, McpDialogShot { edit: true, auth: "secret", names: "X-Api-Key", advanced: true, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_secret_stdio() {
    shoot("mcp-dialog-secret-stdio", H, McpDialogShot { auth: "secret", names: "ACME_TOKEN", ..command() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_oauth_scopes() {
    shoot("mcp-dialog-oauth-scopes", H, McpDialogShot { auth: "oauth", scopes: "read write", advanced: true, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_advanced_open() {
    shoot("mcp-dialog-advanced-open", H, McpDialogShot { advanced: true, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_field_error() {
    shoot("mcp-dialog-field-error", H, McpDialogShot { attempted: true, ..add() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_server_error() {
    shoot("mcp-dialog-server-error", H, McpDialogShot { error: Some("The server refused that URL: the host does not resolve from here, so check it and try again."), ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_saving() {
    shoot("mcp-dialog-saving", H, McpDialogShot { busy: true, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_mcp_dialog_device_open() {
    shoot("mcp-dialog-device-open", H, McpDialogShot { device: Some("d1"), ..filled() }, &[RUNS_ON], Some(1200));
}

#[test]
#[ignore]
fn settings_mcp_dialog_sign_in_open() {
    shoot("mcp-dialog-sign-in-open", H, McpDialogShot { auth: "secret", names: "X-Api-Key", advanced: true, ..filled() }, &[SIGN_IN], Some(1200));
}

#[test]
#[ignore]
fn settings_mcp_dialog_transport_open() {
    shoot("mcp-dialog-transport-open", H, McpDialogShot { advanced: true, ..filled() }, &[TRANSPORT], Some(1200));
}

#[test]
#[ignore]
fn settings_mcp_dialog_short() {
    shoot("mcp-dialog-short", H_SHORT, McpDialogShot { auth: "secret", names: "ACME_TOKEN", ..command() }, &[], None);
}

/// Scrolled to the end, once the overlay bar has faded (its fade runs on the
/// real clock).
#[test]
#[ignore]
fn settings_mcp_dialog_scrolled() {
    let shot = McpDialogShot { auth: "secret", names: "ACME_TOKEN", ..command() };
    shoot_pane_acted(
        "mcp-dialog-scrolled",
        move |window, cx| {
            let content = cx.new(|cx| {
                let mut view = McpServerDialogView::for_shots(window, cx);
                view.show(shot, window, cx);
                view
            });
            Column { content, height: H_SHORT }
        },
        |_, _| {},
        Some(std::time::Duration::from_millis(2500)),
        &[],
        |_, window, cx| wheel(window, cx, -2000.),
    );
}

/// The pointer on the Advanced line, off its text (the whole line is the
/// target).
#[test]
#[ignore]
fn settings_mcp_dialog_advanced_hover() {
    shoot_pane_acted(
        "mcp-dialog-advanced-hover",
        move |window, cx| {
            let content = cx.new(|cx| {
                let mut view = McpServerDialogView::for_shots(window, cx);
                view.show(filled(), window, cx);
                view
            });
            Column { content, height: H }
        },
        |_, _| {},
        None,
        &[],
        |_, window, cx| {
            let position = gpui::point(px(ADVANCED_END.0), px(ADVANCED_END.1));
            window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }), cx);
        },
    );
}
