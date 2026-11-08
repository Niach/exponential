//! Dialogs (EXP-1557): every dialog/alert draws INSIDE the window that
//! opened it, over a scrim, like the web's `Dialog` — the approved #EXP-1549
//! call ("desktop dialogs go in-window over a scrim like web"). EXP-284 to
//! EXP-720 made them real OS windows (own traffic lights, no backdrop, the
//! app behind fully lit); that is gone.
//!
//! The shared shape is [`open_dialog_window`] (content dialogs) and
//! [`open_alert`] (confirm/alert dialogs) over one [`DialogShell`] layer per
//! open dialog, kept per window in an app-global registry and painted by
//! [`render_layer`] above the window's content (`Shell::render`, the
//! undocked windows):
//!
//! - **Look**: the web `DIALOG_CLASSES` — a `black/60` scrim over the whole
//!   window, the centred panel (`rounded-2xl`, `border-glass-stroke-card`,
//!   `shadow-2xl`) capped to the viewport less a 1rem gutter, the title
//!   `text-lg font-semibold` top-left and the ✕ top-right. gpui cannot blur
//!   what is behind a view, so the web's `backdrop-blur-sm` is the scrim
//!   alone (the eUI gpui renderer's layers make the same trade).
//! - **Focus/dismiss**: Escape, the ✕ or a press on the scrim close the top
//!   layer, all gated by the per-dialog `can_close` so a busy submit keeps it
//!   up. Enter runs the per-dialog `on_enter` submit fallback. Focus moves
//!   into the dialog on open (unless its content claimed it) and returns to
//!   whatever held it on close.
//! - **Never stacks** a content dialog: a second open while one is up is a
//!   no-op ([`raise_existing_dialog`]). An ALERT may stack over a dialog
//!   (a confirm raised from inside Device settings).
//! - **Result plumbing**: [`close_dialog_window`] closes the top layer;
//!   [`close_then`] additionally runs a callback in the same window once it
//!   is gone (navigate-after-create, the next dialog).
//!
//! The names keep their EXP-284 `_window` suffix so the 60-odd callers did
//! not churn: a "dialog window" is now a dialog layer of a window.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    actions, deferred, div, prelude::FluentBuilder as _, px, size, AnyElement, AnyView, App,
    AppContext as _, Entity, FocusHandle, Focusable, FontWeight, Global,
    InteractiveElement as _, IntoElement, KeyBinding, MouseButton, ParentElement, Pixels, Render,
    ScrollHandle, SharedString, Size, Styled, StatefulInteractiveElement as _, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    scroll::{ScrollableElement as _, Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use domain::prompts::{Prompt, Role};
use theme::tokens as t;
use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};

const CONTEXT: &str = "NativeDialog";

actions!(native_dialog, [CancelNativeDialog, ConfirmNativeDialog]);

/// The web panel's `sm:w-[calc(100%-2rem)]` / `max-h-[calc(100dvh-2rem)]`
/// gutter: a dialog never comes closer than this to the window edge.
const GUTTER: f32 = 16.;
/// The web `rounded-2xl` panel radius.
const PANEL_RADIUS: f32 = 16.;
/// The header row's height above the content: the ✕/title band
/// (`top-4` + the `text-lg` line + the panel gap).
const HEADER_H: f32 = 48.;

/// Register the Escape/Enter bindings and the dialog registry. Called once
/// from `ui::init`.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelNativeDialog, Some(CONTEXT)),
        KeyBinding::new("enter", ConfirmNativeDialog, Some(CONTEXT)),
    ]);
    let registry = cx.new(|_| DialogRegistry::default());
    cx.set_global(DialogRegistryGlobal(registry));
}

/// Runtime registry: window id → its open dialog layers, bottom first.
#[derive(Default)]
struct DialogRegistry {
    layers: HashMap<WindowId, Vec<Entity<DialogShell>>>,
}

struct DialogRegistryGlobal(Entity<DialogRegistry>);

impl Global for DialogRegistryGlobal {}

fn registry(cx: &App) -> Option<Entity<DialogRegistry>> {
    cx.try_global::<DialogRegistryGlobal>()
        .map(|global| global.0.clone())
}

fn top_layer(window: &Window, cx: &App) -> Option<Entity<DialogShell>> {
    let id = window.window_handle().window_id();
    registry(cx)?.read(cx).layers.get(&id)?.last().cloned()
}

/// Whether a dialog is up in this window.
pub(crate) fn has_open_dialog(window: &Window, cx: &App) -> bool {
    top_layer(window, cx).is_some()
}

/// The never-stack guard: `true` when the caller must NOT open a dialog
/// because this window already shows one (it is in plain sight over its
/// scrim, so there is nothing to raise any more — EXP-287's raise was for
/// a dialog window buried behind its opener). Called by
/// [`open_dialog_window`] itself and by the openers that must decide before
/// building any state (⌘K spam, an invite deep link arriving mid-dialog).
pub(crate) fn raise_existing_dialog(window: &mut Window, cx: &mut App) -> bool {
    has_open_dialog(window, cx)
}

/// N-6's host rule asked whether `window` IS a (small) dialog window, to
/// send its menus to a menu window and its pickers inline. A dialog is a
/// layer of its window now, so its menus and pickers open in-window like
/// everywhere else: never.
pub(crate) fn is_dialog_window(_window: &Window, _cx: &App) -> bool {
    false
}

/// A shell window is going away — drop the dialog layers it holds.
pub(crate) fn on_opener_released(opener: WindowId, cx: &mut App) {
    if let Some(registry) = registry(cx) {
        registry.update(cx, |registry, _| {
            registry.layers.remove(&opener);
        });
    }
}

/// Pop this window's top layer and hand focus back to what held it.
fn pop_layer(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let Some(registry) = registry(cx) else {
        return;
    };
    let popped = registry.update(cx, |registry, _| {
        let layers = registry.layers.get_mut(&id)?;
        let popped = layers.pop();
        if layers.is_empty() {
            registry.layers.remove(&id);
        }
        popped
    });
    if let Some(shell) = popped {
        if let Some(previous) = shell.read(cx).previous_focus.clone() {
            window.focus(&previous, cx);
        }
    }
    window.refresh();
}

/// Close this window's top dialog (deferred — safe from inside its own
/// update).
pub(crate) fn close_dialog_window(window: &mut Window, cx: &mut App) {
    window.defer(cx, pop_layer);
}

/// Close this window's top dialog, then run `f` in the same window. This is
/// the result path back to the surface under the dialog: navigation after a
/// create, notifications, terminal-tab spawns, the next dialog (Add account
/// → the remote sign-in dialog — the never-stack guard sees the closed one
/// gone because the pop runs first).
pub(crate) fn close_then(
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    window.defer(cx, move |window, cx| {
        pop_layer(window, cx);
        f(window, cx);
    });
}

// ---------------------------------------------------------------------------
// Content dialogs
// ---------------------------------------------------------------------------

/// VAPP-121: re-title this window's top dialog, for a dialog whose content
/// switches step (New app → Import an app).
pub(crate) fn set_dialog_title(window: &mut Window, cx: &mut App, title: impl Into<SharedString>) {
    let title = title.into();
    if let Some(shell) = top_layer(window, cx) {
        shell.update(cx, |shell, cx| {
            shell.title = title;
            cx.notify();
        });
    }
}

/// Per-dialog shape.
pub(crate) struct DialogSpec {
    /// The panel title (the web `DialogTitle`).
    pub title: SharedString,
    /// Content size, below the header row. Capped to the window less the
    /// gutter at paint time.
    pub size: Size<Pixels>,
    /// `false` = no header row (search palette, image preview: a title would
    /// sit on the input/image). The name is EXP-287's (it was the OS chrome).
    native_chrome: bool,
    /// EXP-716: the panel's top edge as a fraction of the window's height
    /// instead of centered — the web palette's `sm:top-[15%]`, so a panel
    /// that grows with its content extends downward like a dropdown.
    anchor_top: Option<f32>,
    /// #EXP-1588: the panel HUGS its content (the web `DialogContent`'s
    /// auto height) up to the viewport cap instead of `size.height`; the
    /// body scrolls past the cap. Every alert.
    hug: bool,
}

impl DialogSpec {
    pub(crate) fn new(title: impl Into<SharedString>, size: Size<Pixels>) -> Self {
        Self {
            title: title.into(),
            size,
            native_chrome: true,
            anchor_top: None,
            hug: false,
        }
    }

    /// Pin the panel's top edge `fraction` of the window's height down
    /// (EXP-716). The web palette's `sm:top-[15%]`.
    pub(crate) fn anchor_top(mut self, fraction: f32) -> Self {
        self.anchor_top = Some(fraction);
        self
    }

    /// EXP-285's resize floor. An in-window panel is not user-resizable; kept
    /// so the callers read the same.
    pub(crate) fn resizable(self, _min_size: Size<Pixels>) -> Self {
        self
    }

    /// #EXP-1588: size the panel to its content (see [`DialogSpec::hug`]);
    /// `size.height` is ignored.
    pub(crate) fn hug(mut self) -> Self {
        self.hug = true;
        self
    }

    /// No header row: the content owns the whole panel.
    pub(crate) fn chromeless(mut self) -> Self {
        self.native_chrome = false;
        self
    }
}

type CanCloseFn = Rc<dyn Fn(&App) -> bool>;
type OnEnterFn = Rc<dyn Fn(&mut Window, &mut App)>;
type TitleContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// What a dialog's build closure hands back: the content view plus the
/// shell-level semantics (`title`, `keyboard(!busy)`, `on_ok`).
pub(crate) struct DialogContent {
    view: AnyView,
    /// A header row (title + ✕) for a [`DialogSpec::chromeless`] dialog —
    /// the image lightbox, where it carries the title anyway.
    header: Option<SharedString>,
    /// EXP-426: extra controls in the header's right cluster, before the ✕
    /// (the image lightbox's "Open in browser").
    header_actions: Option<TitleContentFn>,
    /// Standard 16px padding + overflow scrolling around the content
    /// (`false` = the view owns the whole content box).
    padded: bool,
    /// EXP-291: keep the padding but hand the view a DEFINITE full-height
    /// body box instead of the shell's scroller (see
    /// [`DialogContent::self_scrolling`]).
    self_scrolling: bool,
    /// Gates Escape / ✕ / a scrim press. Default: always closable.
    can_close: Option<CanCloseFn>,
    /// Enter fallback when no focused editor consumed it (the old `on_ok`).
    on_enter: Option<OnEnterFn>,
}

impl DialogContent {
    pub(crate) fn new(view: impl Into<AnyView>) -> Self {
        Self {
            view: view.into(),
            header: None,
            header_actions: None,
            padded: true,
            self_scrolling: false,
            can_close: None,
            on_enter: None,
        }
    }

    /// A header row (title + ✕) on a [`DialogSpec::chromeless`] dialog.
    pub(crate) fn chromeless_header(mut self, title: impl Into<SharedString>) -> Self {
        self.header = Some(title.into());
        self
    }

    /// EXP-426: extra header controls next to the ✕.
    pub(crate) fn chromeless_header_actions(
        mut self,
        actions: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.header_actions = Some(Rc::new(actions));
        self
    }

    pub(crate) fn padless(mut self) -> Self {
        self.padded = false;
        self
    }

    /// EXP-291: keep the standard padded body + header row, but let the
    /// CONTENT view own its scrolling, so it can pin an action bar at the
    /// bottom and scroll only its own body (the Start-coding dialog's
    /// Cancel/Start used to scroll out of reach).
    pub(crate) fn self_scrolling(mut self) -> Self {
        self.self_scrolling = true;
        self
    }

    pub(crate) fn can_close(mut self, can_close: impl Fn(&App) -> bool + 'static) -> Self {
        self.can_close = Some(Rc::new(can_close));
        self
    }

    pub(crate) fn on_enter(mut self, on_enter: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_enter = Some(Rc::new(on_enter));
        self
    }
}

/// The content box of this window's top dialog (`None`: no dialog up) —
/// what the content-fitting dialogs measure their chrome against. It used
/// to be the dialog window's viewport.
pub(crate) fn dialog_size(window: &Window, cx: &App) -> Option<Size<Pixels>> {
    top_layer(window, cx).map(|shell| shell.read(cx).size)
}

/// EXP-716: resize this window's top dialog to a new CONTENT size. The
/// panel keeps its top edge when it is top-anchored (the search palette
/// grows downward) and stays centered otherwise.
pub(crate) fn resize_dialog_keeping_top(
    window: &mut Window,
    cx: &mut App,
    size: Size<Pixels>,
) {
    if let Some(shell) = top_layer(window, cx) {
        shell.update(cx, |shell, cx| {
            shell.size = size;
            cx.notify();
        });
    }
}

/// EXP-716: the vertical space a chromeless dialog's own chrome takes — the
/// panel's 1px edge top and bottom — so a content-sized dialog can convert
/// its content height into a panel height.
pub(crate) fn chromeless_vertical_chrome(_window: Option<&Window>) -> Pixels {
    px(2.)
}

/// Open a dialog over `window`. `build` runs in `window` and returns the
/// content view + semantics. Never stacks (see [`raise_existing_dialog`]).
pub(crate) fn open_dialog_window(
    window: &mut Window,
    cx: &mut App,
    spec: DialogSpec,
    build: impl FnOnce(&mut Window, &mut App) -> DialogContent + 'static,
) {
    if raise_existing_dialog(window, cx) {
        return;
    }
    push_layer(window, cx, spec, build);
}

fn push_layer(
    window: &mut Window,
    cx: &mut App,
    spec: DialogSpec,
    build: impl FnOnce(&mut Window, &mut App) -> DialogContent + 'static,
) {
    let previous_focus = window.focused(cx);
    let content = build(window, cx);
    // The content claimed focus while building (search focuses its input):
    // leave it there; otherwise the panel takes it, so Escape/Enter reach
    // the `CONTEXT` bindings from the first frame.
    let content_focused = window.focused(cx) != previous_focus;
    let shell = cx.new(|cx| DialogShell {
        content,
        title: spec.title,
        size: spec.size,
        header: spec.native_chrome,
        anchor_top: spec.anchor_top,
        hug: spec.hug,
        previous_focus,
        focus_handle: cx.focus_handle(),
    });
    if !content_focused {
        let handle = shell.read(cx).focus_handle.clone();
        window.focus(&handle, cx);
    }
    let id = window.window_handle().window_id();
    if let Some(registry) = registry(cx) {
        registry.update(cx, |registry, _| {
            registry.layers.entry(id).or_default().push(shell);
        });
    }
    window.refresh();
    // A tooltip already up on the opener stays over the scrim until the pointer
    // moves: once the scrim has drawn (it occludes the opener), report the
    // pointer where it is, so the opener's hover ends with its tooltip.
    window.on_next_frame(|window, _| {
        window.on_next_frame(|window, cx| {
            let event = gpui::MouseMoveEvent {
                position: window.mouse_position(),
                pressed_button: None,
                modifiers: window.modifiers(),
            };
            window.dispatch_event(gpui::PlatformInput::MouseMove(event), cx);
        });
    });
}

/// The paint priority of [`render_layer`]: gpui-component's sheet layer is
/// mounted by `Root` after the window content and its dialogs paint deferred
/// at `10 + id`, so 9 keeps the in-window dialogs above a sheet and below a
/// gpui-component dialog (the pre-0.9 composition order).
const LAYER_PRIORITY: usize = 9;

/// Every open dialog layer of `window`, bottom first — composed by each
/// window root ABOVE its content and below the toasts. `None` = none open.
pub(crate) fn render_layer(window: &Window, cx: &App) -> Option<AnyElement> {
    let id = window.window_handle().window_id();
    let layers = registry(cx)?.read(cx).layers.get(&id)?.clone();
    Some(
        deferred(
            div()
                .absolute()
                .inset_0()
                .children(layers.into_iter().map(|shell| shell.into_any_element())),
        )
        .with_priority(LAYER_PRIORITY)
        .into_any_element(),
    )
}

/// One open dialog: the scrim, the panel, the header, Escape/Enter.
struct DialogShell {
    content: DialogContent,
    /// The panel title.
    title: SharedString,
    /// The content box (below the header row).
    size: Size<Pixels>,
    /// Whether the panel draws the title + ✕ header row.
    header: bool,
    anchor_top: Option<f32>,
    /// See [`DialogSpec::hug`].
    hug: bool,
    /// What held focus before the dialog opened; it gets it back on close.
    previous_focus: Option<FocusHandle>,
    focus_handle: FocusHandle,
}

impl DialogShell {
    fn can_close(&self, cx: &App) -> bool {
        self.content
            .can_close
            .as_ref()
            .map(|can_close| can_close(cx))
            .unwrap_or(true)
    }
}

impl Focusable for DialogShell {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The web `DialogContent` panel frame: the glass card over the scrim.
/// Shared with [`alert_specimen`], so the styleguide pictures the product's
/// panel.
fn panel_frame(cx: &App) -> gpui::Div {
    v_flex()
        .flex_shrink_0()
        .overflow_hidden()
        .rounded(px(PANEL_RADIUS))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        // The web `bg-card/85` over its blur: the page ground with the
        // panel wash on top (gpui cannot blur, so the ground is opaque).
        .bg(cx.theme().background)
        .shadow_2xl()
        .text_color(cx.theme().foreground)
}

/// The web `DialogHeader` + the ✕ (`absolute top-4 right-4`): the title
/// `text-lg font-semibold` leading, `actions` then the ✕ trailing.
fn panel_header(
    title: SharedString,
    actions: Option<AnyElement>,
    close: Option<Button>,
) -> gpui::Div {
    h_flex()
        .h(px(HEADER_H))
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap_2()
        .pl_4()
        .pr_3()
        .pt_2()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .children(actions)
                .children(close),
        )
}

fn close_button(id: &'static str, closable: bool, cx: &App) -> Button {
    Button::new(id)
        .accessibility_label("Close")
        .ghost()
        .cursor_pointer()
        .xsmall()
        .icon(
            Icon::new(registry::UI_CLOSE)
                .small()
                .text_color(cx.theme().muted_foreground),
        )
        .disabled(!closable)
}

impl Render for DialogShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let closable = self.can_close(cx);
        let on_enter = self.content.on_enter.clone();
        let viewport = window.viewport_size();
        let hug = self.hug;

        // The header: the dialog's title + ✕ (every dialog with chrome), or
        // the chromeless lightbox's own header with its extra actions.
        let header_title = if self.header {
            Some(self.title.clone())
        } else {
            self.content.header.clone()
        };
        let header_actions: Option<AnyElement> = self
            .content
            .header_actions
            .clone()
            .map(|actions| actions(window, cx));
        let header = header_title.map(|title| {
            panel_header(
                title,
                header_actions,
                Some(close_button("native-dialog-close", closable, cx).on_click(
                    cx.listener(|this, _, window, cx| {
                        if this.can_close(cx) {
                            close_dialog_window(window, cx);
                        }
                    }),
                )),
            )
        });
        let header_h = if header.is_some() { px(HEADER_H) } else { px(0.) };

        // The panel: the content box under the header, capped to the window
        // less the gutter (the web `max-h-[calc(100dvh-2rem)]`).
        let width = self.size.width.min(viewport.width - px(2. * GUTTER));
        let top = self
            .anchor_top
            .map(|fraction| (viewport.height * fraction).max(px(GUTTER)));
        let max_h = viewport.height - top.unwrap_or(px(GUTTER)) - px(GUTTER);
        let height = (self.size.height + header_h + px(2.)).min(max_h);

        let body: AnyElement = if self.content.padded {
            let inner: AnyElement = if self.content.self_scrolling {
                div()
                    .size_full()
                    .child(self.content.view.clone())
                    .into_any_element()
            } else {
                v_flex()
                    .size_full()
                    .overflow_y_scrollbar()
                    .child(div().pr_1().child(self.content.view.clone()))
                    .into_any_element()
            };
            div()
                .flex_1()
                .min_h_0()
                .px_4()
                .pb_4()
                .when(header.is_none(), |this| this.pt_4())
                .child(inner)
                .into_any_element()
        } else if self.hug {
            // #EXP-1588: content-sized, shrinking (never growing) under the
            // cap.
            v_flex()
                .min_h_0()
                .child(self.content.view.clone())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .child(self.content.view.clone())
                .into_any_element()
        };

        let panel = panel_frame(cx)
            .id("native-dialog-panel")
            .debug_selector(|| "native-dialog-panel".into())
            .w(width)
            .map(|this| if hug { this.max_h(max_h) } else { this.h(height) })
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            // A press inside the panel never reaches the scrim's dismiss.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &CancelNativeDialog, window, cx| {
                if this.can_close(cx) {
                    close_dialog_window(window, cx);
                }
            }))
            .when_some(on_enter, |this, on_enter| {
                this.on_action(move |_: &ConfirmNativeDialog, window, cx| {
                    on_enter(window, cx);
                })
            })
            .child(
                v_flex()
                    .w_full()
                    .map(|this| if hug { this.min_h_0() } else { this.h_full() })
                    // EXP-771: the panel wash the dialog body always wore.
                    .bg(t::glass::FILL_PANEL.to_hsla())
                    .children(header)
                    .child(body),
            );

        // The web overlay: `fixed inset-0 bg-black/60`, a press on it
        // dismisses (Radix's outside-press).
        div()
            .id("native-dialog-scrim")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::black().opacity(0.6))
            .flex()
            .flex_col()
            .items_center()
            .when_some(top, |this, top| this.pt(top))
            .when(top.is_none(), |this| this.justify_center())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.can_close(cx) {
                        close_dialog_window(window, cx);
                    }
                }),
            )
            .child(panel)
    }
}

// ---------------------------------------------------------------------------
// Alerts (confirm dialogs)
// ---------------------------------------------------------------------------

/// Every alert's width.
const ALERT_WIDTH: f32 = 416.;

type OnOkFn = Rc<dyn Fn(&mut Window, &mut App) -> bool>;
type AlertContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// EXP-1212: the footer answer Enter gives (see [`AlertSpec::enter`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlertEnter {
    /// The OK button, every alert before it. A disabled OK swallows Enter.
    Ok,
    /// Enter dismisses the window, the Cancel answer: for a destructive OK
    /// that must be clicked (the draft discard confirm, where web, iOS and
    /// Android focus Cancel).
    Cancel,
    /// The [`AlertSpec::secondary`] answer (none configured = `Cancel`): the
    /// leave question hands Enter to Save draft while Create is disabled.
    Secondary,
}

/// A confirm dialog: title, description, optional extra content block
/// (e.g. a typed-confirm input), Cancel + OK footer. The old
/// `open_alert_dialog` + `DialogButtonProps` surface.
pub(crate) struct AlertSpec {
    title: SharedString,
    /// EXP-1230: the prompt's ONE question, above the description (a fixture
    /// prompt's `title`; the window title above stays the alert's name).
    /// `None` = the description alone, every alert before it.
    question: Option<SharedString>,
    description: SharedString,
    /// EXP-1230: the Cancel answer's label (the fixture's `cancel` action).
    cancel_text: SharedString,
    ok_text: SharedString,
    ok_variant: ButtonVariant,
    /// EXP-1167: a leading glyph on the OK button, where the web dialog's
    /// primary carries one ("Merge stack" wears the merge glyph). `None` =
    /// label-only, every alert before it.
    ok_icon: Option<ExpIcon>,
    /// Window width; [`ALERT_WIDTH`] unless a footer needs more room.
    /// The height is the content's (#EXP-1588, the web dialog's).
    width: Pixels,
    /// Whether the footer draws Cancel next to OK. `false` (EXP-697) is for
    /// the informational alerts that only have something to dismiss — the
    /// window's own ✕ / Escape does the same job as a Cancel there.
    cancel: bool,
    /// EXP-1557: whether the footer draws at all. `false` = an alert that
    /// only shows something (Add device): the ✕, Escape and the scrim close
    /// it, the web dialog's shape.
    footer: bool,
    /// Extra block between the description and the footer.
    content: Option<AlertContentFn>,
    /// EXP-897: a THIRD button between Cancel and OK, the second real
    /// choice an alert sometimes has ("Start anyway" beside "Stacked PR").
    /// Outline-styled like Cancel, because the primary answer stays the OK.
    /// `None` (every alert before it) draws the two-button footer unchanged.
    secondary: Option<(SharedString, OnOkFn)>,
    /// EXP-1212: a DESTRUCTIVE choice set apart on the footer's LEADING
    /// edge, the rest pushed trailing (the draft leave question's Discard,
    /// across from Create issue and Save draft). Same contract as
    /// [`Self::on_ok`]. `None` (every alert before it) changes nothing.
    destructive: Option<(SharedString, OnOkFn)>,
    /// EXP-1212: runs once the alert's window is GONE, answered or not (Esc,
    /// the ✕, an answer). Lets a caller tell a dismiss from an answer.
    on_closed: Option<Rc<dyn Fn(&mut App)>>,
    /// EXP-980: the OK button is SHOWN but not pressable: the answer stays
    /// visible (with a caption saying why it is off) instead of vanishing.
    ok_disabled: bool,
    /// EXP-1650: drawn as web's prompt CARD (`PromptLayout`): no title band
    /// and no ✕, the question leads (Escape and the scrim still dismiss).
    card: bool,
    /// EXP-1212: which answer Enter gives. `Ok` is every alert before it;
    /// an alert whose default answer is NOT its primary (a destructive OK,
    /// a disabled primary) moves Enter to Cancel or the secondary.
    enter: AlertEnter,
    /// Return `true` to close the window (a `false` keeps it open — the
    /// typed-confirm mismatch case). Runs inside the dialog window.
    on_ok: OnOkFn,
}

impl AlertSpec {
    pub(crate) fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        ok_text: impl Into<SharedString>,
    ) -> Self {
        Self {
            title: title.into(),
            question: None,
            description: description.into(),
            cancel_text: "Cancel".into(),
            ok_text: ok_text.into(),
            ok_variant: ButtonVariant::Primary,
            ok_icon: None,
            width: px(ALERT_WIDTH),
            cancel: true,
            footer: true,
            content: None,
            secondary: None,
            destructive: None,
            on_closed: None,
            ok_disabled: false,
            card: false,
            enter: AlertEnter::Ok,
            on_ok: Rc::new(|_, _| true),
        }
    }

    /// EXP-1230: a confirm built from a `prompts.json` entry
    /// (`domain::prompts`): the question line, the body and both answers are
    /// the fixture's; `window_title` only names the window. Roles map onto the
    /// footer: `cancel` = the Cancel button, the answer = OK (`destructive`
    /// Danger, `primary` Primary, `default` plain); `focus` picks the
    /// [`AlertEnter`] answer, so Return never lands on a destructive one.
    /// The caller attaches the answer with [`Self::on_ok`].
    pub(crate) fn from_prompt(window_title: impl Into<SharedString>, prompt: &Prompt) -> Self {
        let answers: Vec<_> = prompt.actions.iter().filter(|a| a.role != Role::Cancel).collect();
        assert_eq!(
            answers.len(),
            1,
            "{}: a prompt with more than one answer wires its own buttons",
            prompt.id
        );
        let answer = answers[0];
        let ok_variant = match answer.role {
            Role::Primary => ButtonVariant::Primary,
            Role::Destructive | Role::QuietDestructive => ButtonVariant::Danger,
            Role::Default | Role::Cancel => ButtonVariant::Default,
        };
        let mut spec = Self::new(
            window_title,
            prompt.body.clone().unwrap_or_default(),
            answer.label,
        )
        .ok_variant(ok_variant);
        spec.question = Some(prompt.title.clone().into());
        match prompt.actions.iter().find(|a| a.role == Role::Cancel) {
            Some(cancel) => spec.cancel_text = cancel.label.into(),
            None => spec.cancel = false,
        }
        spec.enter = if prompt.focused().role == Role::Cancel {
            AlertEnter::Cancel
        } else {
            AlertEnter::Ok
        };
        spec
    }

    /// EXP-1212: hand Enter to another footer answer (see [`AlertEnter`]).
    pub(crate) fn enter(mut self, enter: AlertEnter) -> Self {
        self.enter = enter;
        self
    }

    /// The answer Enter gives (the draft alerts' tests).
    #[cfg(test)]
    pub(crate) fn enter_answer(&self) -> AlertEnter {
        self.enter
    }

    /// Whether the OK button is shown disabled (the draft alerts' tests).
    #[cfg(test)]
    pub(crate) fn is_ok_disabled(&self) -> bool {
        self.ok_disabled
    }

    /// Drop the Cancel button — an informational alert has nothing to cancel,
    /// only to dismiss (EXP-697). Escape / ✕ still close the window.
    /// EXP-1557: no footer at all (see [`AlertSpec::footer`]).
    pub(crate) fn without_footer(mut self) -> Self {
        self.footer = false;
        self
    }

    pub(crate) fn without_cancel(mut self) -> Self {
        self.cancel = false;
        self
    }

    /// A `Danger` OK also hands Enter to Cancel (EXP-1230, the prompts
    /// contract: focus never lands on a destructive answer).
    pub(crate) fn ok_variant(mut self, variant: ButtonVariant) -> Self {
        self.ok_variant = variant;
        if variant == ButtonVariant::Danger && self.enter == AlertEnter::Ok {
            self.enter = AlertEnter::Cancel;
        }
        self
    }

    /// The question line, the description and the button labels in display
    /// order (the prompt tests).
    #[cfg(test)]
    pub(crate) fn words(&self) -> (Option<String>, String, Vec<String>) {
        let mut buttons = Vec::new();
        if let Some((label, _)) = &self.destructive {
            buttons.push(label.to_string());
        }
        if self.cancel {
            buttons.push(self.cancel_text.to_string());
        }
        if let Some((label, _)) = &self.secondary {
            buttons.push(label.to_string());
        }
        buttons.push(self.ok_text.to_string());
        (
            self.question.as_ref().map(|q| q.to_string()),
            self.description.to_string(),
            buttons,
        )
    }

    /// The OK button's variant (the prompt tests).
    #[cfg(test)]
    pub(crate) fn ok_variant_of(&self) -> ButtonVariant {
        self.ok_variant
    }

    /// EXP-1167: lead the OK label with `icon` (see [`AlertSpec::ok_icon`]).
    pub(crate) fn ok_icon(mut self, icon: ExpIcon) -> Self {
        self.ok_icon = Some(icon);
        self
    }

    /// EXP-1167: a wider window, for a three-button footer whose labels
    /// overflow [`ALERT_WIDTH`].
    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// EXP-1650: the prompt CARD shape (see [`AlertSpec::card`]).
    pub(crate) fn as_card(mut self) -> Self {
        self.card = true;
        self
    }

    /// Whether the alert draws as the prompt card (the consent tests).
    #[cfg(test)]
    pub(crate) fn is_card(&self) -> bool {
        self.card
    }

    /// #EXP-1553: the bold question line over the description (web's
    /// `DialogTitle`); the window title stays the alert's short name.
    pub(crate) fn question(mut self, question: impl Into<SharedString>) -> Self {
        self.question = Some(question.into());
        self
    }

    pub(crate) fn content(
        mut self,
        content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.content = Some(Rc::new(content));
        self
    }

    pub(crate) fn on_ok(mut self, on_ok: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Self {
        self.on_ok = Rc::new(on_ok);
        self
    }

    /// EXP-980: show the OK button disabled (see [`AlertSpec::ok_disabled`]).
    pub(crate) fn ok_disabled(mut self, disabled: bool) -> Self {
        self.ok_disabled = disabled;
        self
    }

    /// EXP-897: the optional middle button (see [`AlertSpec::secondary`]).
    /// Same contract as [`Self::on_ok`]: `true` closes the window.
    pub(crate) fn secondary(
        mut self,
        text: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.secondary = Some((text.into(), Rc::new(on_click)));
        self
    }

    /// EXP-1212: the optional destructive answer (see
    /// [`AlertSpec::destructive`]). Same contract as [`Self::on_ok`].
    pub(crate) fn destructive(
        mut self,
        text: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.destructive = Some((text.into(), Rc::new(on_click)));
        self
    }

    /// EXP-1212: see [`AlertSpec::on_closed`].
    pub(crate) fn on_closed(mut self, on_closed: impl Fn(&mut App) + 'static) -> Self {
        self.on_closed = Some(Rc::new(on_closed));
        self
    }
}

/// Open a confirm dialog over `window`. Unlike a content dialog it may
/// STACK over an open one (a confirm raised from inside Device settings).
pub(crate) fn open_alert(window: &mut Window, cx: &mut App, spec: AlertSpec) {
    // #EXP-1588: the panel hugs the alert (web's auto-height dialog).
    let dialog_size = size(spec.width, px(0.));
    let title = spec.title.clone();
    let mut dialog = DialogSpec::new(title, dialog_size).hug();
    if spec.card {
        dialog = dialog.chromeless();
    }
    push_layer(window, cx, dialog, move |_, cx| {
        let on_closed = spec.on_closed.clone();
        let view = cx.new(|cx| {
            if let Some(on_closed) = on_closed {
                cx.on_release(move |_, cx| on_closed(cx)).detach();
            }
            AlertView::new(spec)
        });
        let on_enter = view.clone();
        DialogContent::new(view)
            .padless()
            .on_enter(move |window, cx| AlertView::enter(&on_enter, window, cx))
    });
}

/// An alert OUTSIDE the scrim, for the styleguide's dialog specimens: the
/// real [`AlertView`] over the SAME [`AlertSpec`] the product opens, in the
/// product's panel ([`panel_frame`] + [`panel_header`]) at the alert's own
/// width, hugging its content like the product. `key` keeps the view across frames
/// (`window.use_keyed_state`).
pub(crate) fn alert_specimen(
    key: &'static str,
    spec: AlertSpec,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Div {
    let title = spec.title.clone();
    let width = spec.width;
    let view = window.use_keyed_state(key, cx, |_, _| AlertView::new(spec));
    panel_frame(cx).w(width).child(
        v_flex()
            .w_full()
            .bg(t::glass::FILL_PANEL.to_hsla())
            .child(panel_header(title, None, Some(close_button("native-alert-specimen-close", true, cx))))
            .child(view),
    )
}

/// #EXP-1553: an alert drawn as web's prompt CARD (no window-title band):
/// the question leads inside the card, then the body and the footer, on
/// the dialog panel's rounded glass. For styleguide specimens whose web twin
/// draws `PromptLayout` (the chat delete confirm).
pub(crate) fn alert_card_specimen(
    key: &'static str,
    spec: AlertSpec,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Div {
    let width = spec.width;
    let view = window.use_keyed_state(key, cx, |_, _| AlertView::new(spec));
    v_flex()
        .w(width)
        .flex_shrink_0()
        .rounded(px(t::radius::XL))
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_PANEL.to_hsla())
        .shadow_2xl()
        .text_color(cx.theme().foreground)
        .child(div().w_full().p_1().child(view))
}

struct AlertView {
    spec: AlertSpec,
    body_scroll: ScrollHandle,
}

impl AlertView {
    fn new(spec: AlertSpec) -> Self {
        Self { spec, body_scroll: ScrollHandle::new() }
    }

    /// The OK button's click (and Enter, for [`AlertEnter::Ok`]).
    fn confirm(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        if view.read(cx).spec.ok_disabled {
            return;
        }
        let on_ok = view.read(cx).spec.on_ok.clone();
        if on_ok(window, cx) {
            close_dialog_window(window, cx);
        }
    }

    /// EXP-1212: Enter, routed per [`AlertSpec::enter`].
    fn enter(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        match view.read(cx).spec.enter {
            AlertEnter::Ok => Self::confirm(view, window, cx),
            AlertEnter::Cancel => close_dialog_window(window, cx),
            AlertEnter::Secondary => {
                let Some((_, on_click)) = view.read(cx).spec.secondary.clone() else {
                    close_dialog_window(window, cx);
                    return;
                };
                if on_click(window, cx) {
                    close_dialog_window(window, cx);
                }
            }
        }
    }
}

impl Render for AlertView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let extra = self.spec.content.as_ref().map(|content| content(window, cx));

        // No header row here — the dialog panel's header carries the title.
        // #EXP-1588: content-sized; under the panel's viewport cap the
        // body shrinks and scrolls, the footer stays pinned.
        v_flex()
            .w_full()
            .min_h_0()
            .px_4()
            .pb_4()
            // EXP-1650: a card has no header band above it.
            .when(self.spec.card, |this| this.pt_4())
            .gap_3()
            .child(
                // gpui's own scroll + the overlay scrollbar (the create
                // board/team dialogs' shape): gpui-component's
                // `overflow_y_scrollbar` wrapper measures 0 in an auto-height
                // parent, which hid the body of every hugging alert.
                v_flex()
                    .relative()
                    .min_h_0()
                    .child(
                v_flex()
                    .id("native-alert-body")
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .gap_3()
                    .children(self.spec.question.clone().map(|question| {
                        // #EXP-1553: web's dialog title (bold, a step up).
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(question)
                    }))
                    .when(!self.spec.description.is_empty(), |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(self.spec.description.clone()),
                        )
                    })
                    .children(extra),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .child(Scrollbar::new(&self.body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .when(self.spec.footer, |this| { this.child(
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    // EXP-1212: the destructive answer stands APART on the
                    // leading edge (a spacer pushes the rest trailing), the
                    // Thunderbird save-prompt shape.
                    .children(self.spec.destructive.as_ref().map(|(label, on_click)| {
                        let on_click = on_click.clone();
                        Button::new("native-alert-destructive")
                            .danger().cursor_pointer()
                            .web_sm()
                            .label(label.clone())
                            .on_click(move |_, window, cx| {
                                if on_click(window, cx) {
                                    close_dialog_window(window, cx);
                                }
                            })
                    }))
                    .when(self.spec.destructive.is_some(), |this| {
                        this.child(div().flex_1())
                    })
                    .when(self.spec.cancel, |this| {
                        this.child(
                            Button::new("native-alert-cancel")
                                .outline().cursor_pointer()
                                .web_sm()
                                .label(self.spec.cancel_text.clone())
                                .on_click(|_, window, cx| close_dialog_window(window, cx)),
                        )
                    })
                    .children(self.spec.secondary.as_ref().map(|(label, on_click)| {
                        let on_click = on_click.clone();
                        Button::new("native-alert-secondary")
                            .outline().cursor_pointer()
                            .web_sm()
                            .label(label.clone())
                            .on_click(move |_, window, cx| {
                                if on_click(window, cx) {
                                    close_dialog_window(window, cx);
                                }
                            })
                    }))
                    .child(
                        Button::new("native-alert-ok")
                            .debug_selector(|| "native-alert-ok".into())
                            .with_variant(self.spec.ok_variant)
                            .web_sm()
                            .when_some(self.spec.ok_icon.clone(), |button, icon| {
                                button.icon(Icon::new(icon))
                            })
                            .label(self.spec.ok_text.clone())
                            .disabled(self.spec.ok_disabled)
                            .on_click(move |_, window, cx| {
                                Self::confirm(&view, window, cx);
                            }),
                    ),
            ) })
    }
}

#[cfg(test)]
mod prompt_tests {
    use super::*;
    use domain::prompts::{self, ALL};

    /// EXP-1230: every fixture prompt lands on the native alert with the
    /// fixture's question, body and button labels in display order; the
    /// answer's role picks the OK variant and `focus` the Return answer.
    #[test]
    fn every_fixture_prompt_maps_onto_the_alert() {
        for spec in ALL {
            let title_key = spec.copy.iter().map(|(k, _)| *k).find(|k| k.starts_with("title")).unwrap();
            let body_key = spec.copy.iter().map(|(k, _)| *k).find(|k| k.starts_with("body"));
            let prompt = spec.render(title_key, body_key, &[]);
            let alert = AlertSpec::from_prompt("Window", &prompt);
            let (question, description, buttons) = alert.words();
            assert_eq!(question.as_deref(), Some(prompt.title.as_str()), "{}", spec.id);
            assert_eq!(description, prompt.body.clone().unwrap_or_default(), "{}", spec.id);
            let labels: Vec<String> = spec.actions.iter().map(|a| a.label.to_string()).collect();
            assert_eq!(buttons, labels, "{}", spec.id);
            let answer = spec.actions.last().unwrap();
            let variant = match answer.role {
                Role::Primary => ButtonVariant::Primary,
                Role::Destructive => ButtonVariant::Danger,
                _ => ButtonVariant::Default,
            };
            assert_eq!(alert.ok_variant_of(), variant, "{}", spec.id);
            // `focus` names an action id; its ROLE decides where Return lands
            // (the consent prompt focuses `deny`, a cancel-role action).
            let focused_role = spec.actions.iter().find(|a| a.id == spec.focus).map(|a| a.role);
            let enter = if focused_role == Some(Role::Cancel) { AlertEnter::Cancel } else { AlertEnter::Ok };
            assert_eq!(alert.enter_answer(), enter, "{}", spec.id);
        }
    }

    #[test]
    fn return_never_lands_on_a_destructive_answer() {
        let stop = AlertSpec::from_prompt("Stop session", &prompts::stop_run());
        assert_eq!(stop.enter_answer(), AlertEnter::Cancel);
        let make_owner = AlertSpec::from_prompt("Make owner", &prompts::make_owner("Ada"));
        assert_eq!(make_owner.enter_answer(), AlertEnter::Ok);
        // A hand-written destructive confirm follows the same rule.
        let danger = AlertSpec::new("Reset", "Gone.", "Reset").ok_variant(ButtonVariant::Danger);
        assert_eq!(danger.enter_answer(), AlertEnter::Cancel);
    }
}

#[cfg(test)]
mod hug_tests {
    use super::*;
    use gpui::{point, Bounds, VisualTestContext};

    struct Probe(Pixels);

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().debug_selector(|| "probe-view".into()).w_full().h(self.0)
        }
    }

    /// Opens `spec` in a test window and returns the panel's, the probe
    /// content's and the OK button's bounds, and the viewport height.
    fn layout(
        spec: impl FnOnce(&mut App) -> AlertSpec,
        cx: &mut gpui::TestAppContext,
    ) -> (Bounds<Pixels>, Option<Bounds<Pixels>>, Option<Bounds<Pixels>>, Pixels) {
        let cx: &mut VisualTestContext = cx.add_empty_window();
        cx.update(|window, cx| {
            gpui_component::init(cx);
            init(cx);
            let spec = spec(cx);
            open_alert(window, cx, spec);
        });
        for _ in 0..2 {
            cx.draw(point(px(0.), px(0.)), size(px(900.), px(600.)), |window, cx| {
                div()
                    .size_full()
                    .children(render_layer(window, cx))
            });
        }
        (
            cx.debug_bounds("native-dialog-panel").expect("the panel draws"),
            cx.debug_bounds("probe-view"),
            cx.debug_bounds("native-alert-ok"),
            cx.update(|window, _| window.viewport_size().height),
        )
    }

    /// #EXP-1588: the panel hugs a content block (#1590's Add device
    /// collapsed to its header): header + the block + the padding, nothing
    /// more, nothing less.
    #[gpui::test]
    fn an_alert_hugs_its_content(cx: &mut gpui::TestAppContext) {
        let (panel, probe, ok, _) = layout(
            |cx| {
                let probe = cx.new(|_| Probe(px(240.)));
                AlertSpec::new("Add device", "", "Done")
                    .without_footer()
                    .width(px(512.))
                    .content(move |_, _| v_flex().gap_3().child(probe.clone()).into_any_element())
            },
            cx,
        );
        let probe = probe.expect("the content draws");
        assert_eq!(probe.size.height, px(240.), "the content keeps its height");
        assert!(ok.is_none(), "no footer");
        // header + content + `pb_4` + the 1px border top and bottom.
        assert_eq!(panel.size.height, px(HEADER_H + 240. + 16. + 2.));
    }

    /// A plain confirm: as tall as its words and footer, far under the old
    /// fixed 220.
    #[gpui::test]
    fn a_confirm_hugs_its_footer(cx: &mut gpui::TestAppContext) {
        let (panel, _, ok, _) = layout(|_| AlertSpec::new("Delete", "Gone for good.", "Delete"), cx);
        let ok = ok.expect("the footer draws");
        assert!(panel.size.height < px(200.), "hugs: {:?}", panel.size.height);
        assert!(ok.bottom() <= panel.bottom() - px(16.), "the footer sits inside the padding");
    }

    /// Past the window the panel caps at the viewport less the gutters;
    /// the body scrolls and the footer stays on the panel.
    #[gpui::test]
    fn a_tall_alert_caps_and_keeps_its_footer(cx: &mut gpui::TestAppContext) {
        let (panel, _, ok, viewport_h) = layout(
            |cx| {
                let probe = cx.new(|_| Probe(px(4000.)));
                AlertSpec::new("Long", "", "OK").content(move |_, _| probe.clone().into_any_element())
            },
            cx,
        );
        assert_eq!(panel.size.height, viewport_h - px(2. * GUTTER));
        let ok = ok.expect("the footer draws");
        assert!(ok.bottom() <= panel.bottom(), "the footer stays on the panel");
    }
}

