//! Native dialog windows (EXP-284): every dialog/alert spawns as a real OS
//! window instead of gpui-component's in-window Dialog overlay layer.
//!
//! The shared shape is [`open_dialog_window`] (content dialogs) and
//! [`open_alert`] (confirm/alert dialogs) over one [`DialogShell`] root view:
//!
//! - **Window options** (EXP-285): `WindowKind::Floating` — a plain floating
//!   panel above the opener on every platform (never a macOS sheet or a
//!   Windows owner-modal popup: those are undraggable and block/disable the
//!   opener, which click-away dismissal needs alive). Dialogs carry NATIVE
//!   decoration on macOS (`titlebar: Some(appears_transparent)` → traffic
//!   lights float over the content; only close is enabled) unless the spec
//!   opts out via [`DialogSpec::chromeless`] (search palette, image preview).
//!   Windows/Linux stay visually frameless (Windows suppresses the caption
//!   for transparent titlebars, Linux CSD draws the rounded `window_frame`)
//!   and the shell's header row doubles as the drag region.
//! - **Parent-relative positioning**: bounds are centered over the opener's
//!   window bounds at open time; dialog headers drag the window (the
//!   `should_move`/`start_window_move` titlebar pattern).
//! - **Focus/dismiss semantics**: Escape, the header ✕, and the native close
//!   button close (gated by the per-dialog `can_close` — busy submits keep
//!   the window up), Enter runs the per-dialog `on_enter` submit fallback,
//!   and the OS close request (Alt-F4 / WM close) consults the same
//!   `can_close`. EXP-285: clicking the OPENER window again dismisses the
//!   dialog (an opener-activation observer — ⌘-tabbing away does not).
//! - **Result plumbing**: the shell registers `dialog window → opener` in an
//!   app-global registry; [`close_dialog_window`] closes the dialog and
//!   [`close_then`] additionally runs a callback inside the opener window
//!   (navigate-after-create, spawn-terminal-tab, notifications) — cross-window
//!   updates are deferred, mirroring `undock`'s re-entrancy rules.
//!
//! Dialog content views are created INSIDE the new window (input focus, key
//! contexts and overlay layers are per-window) and read the shared app
//! globals like every other surface.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    actions, div, point, prelude::FluentBuilder as _, px, size, AnyElement, AnyView,
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, FocusHandle, Focusable, FontWeight,
    Global, InteractiveElement as _, IntoElement, KeyBinding, MouseButton, ParentElement, Pixels,
    Render, SharedString, Size, Styled, Window, WindowBounds, WindowControlArea, WindowId,
    WindowKind, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Root, Sizable as _,
};
use theme::tokens as t;

const CONTEXT: &str = "NativeDialog";

actions!(native_dialog, [CancelNativeDialog, ConfirmNativeDialog]);

/// Register the Escape/Enter bindings and the dialog-window registry. Called
/// once from `ui::init`.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelNativeDialog, Some(CONTEXT)),
        KeyBinding::new("enter", ConfirmNativeDialog, Some(CONTEXT)),
    ]);
    let registry = cx.new(|_| DialogRegistry::default());
    cx.set_global(DialogRegistryGlobal(registry));
}

/// One live dialog window's registry row.
struct DialogRow {
    /// The window that opened this dialog.
    opener: AnyWindowHandle,
    /// Clone of the dialog's busy gate — the click-away dismiss observer
    /// consults it without touching the dialog window.
    can_close: Option<CanCloseFn>,
    /// The opener-activation observer driving click-away dismissal
    /// (EXP-285). Dropped with the row, detaching automatically.
    _dismiss: Option<gpui::Subscription>,
}

/// Runtime registry: dialog window id → its [`DialogRow`].
#[derive(Default)]
struct DialogRegistry {
    openers: HashMap<WindowId, DialogRow>,
    /// Openers whose dialog window is in flight — spawned but not opened yet.
    /// `openers` only fills in once the window exists, so without this a
    /// double-trigger (double-clicked button, ⌘N twice) would race past the
    /// never-stack guard and open two windows.
    pending: HashSet<WindowId>,
}

struct DialogRegistryGlobal(Entity<DialogRegistry>);

impl Global for DialogRegistryGlobal {}

fn registry(cx: &App) -> Option<Entity<DialogRegistry>> {
    cx.try_global::<DialogRegistryGlobal>()
        .map(|global| global.0.clone())
}

/// Whether a dialog is already up "here" — this window is a dialog, or a
/// dialog it opened is still alive (or on its way up). The native replacement
/// for the old `window.has_active_dialog` never-stack guards (⌘K spam, deep
/// links).
pub(crate) fn dialog_open_here(window: &Window, cx: &App) -> bool {
    let id = window.window_handle().window_id();
    registry(cx).is_some_and(|registry| {
        let registry = registry.read(cx);
        registry.openers.contains_key(&id)
            || registry.pending.contains(&id)
            || registry
                .openers
                .values()
                .any(|row| row.opener.window_id() == id)
    })
}

fn opener_of(window: &Window, cx: &App) -> Option<AnyWindowHandle> {
    let id = window.window_handle().window_id();
    registry(cx).and_then(|registry| registry.read(cx).openers.get(&id).map(|row| row.opener))
}

/// Close this dialog window (deferred — safe from inside its own update).
pub(crate) fn close_dialog_window(window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    });
}

/// Close this dialog window, then run `f` inside the OPENER window
/// (best-effort — the opener may already be gone). This is the result path
/// back to the surface that opened the dialog: navigation after a create,
/// notifications, terminal-tab spawns.
pub(crate) fn close_then(
    window: &mut Window,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    let opener = opener_of(window, cx);
    let handle = window.window_handle();
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        if let Some(opener) = opener {
            let _ = opener.update(cx, |_, window, cx| {
                f(window, cx);
                window.activate_window();
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Content dialogs
// ---------------------------------------------------------------------------

/// Per-dialog window shape.
pub(crate) struct DialogSpec {
    /// OS window title (a11y / window lists; the shell only draws it when
    /// [`DialogContent::header`] asks for a header row).
    pub title: SharedString,
    /// Inner content size. Callers cap against the opener's viewport.
    pub size: Size<Pixels>,
    /// EXP-285: user-resizable window with this floor. `None` = fixed size.
    min_size: Option<Size<Pixels>>,
    /// EXP-285: native decoration (macOS traffic lights — close only) over
    /// the content. `false` = fully frameless (search palette, image
    /// preview, where lights would overlap the input/image).
    native_chrome: bool,
}

impl DialogSpec {
    pub(crate) fn new(title: impl Into<SharedString>, size: Size<Pixels>) -> Self {
        Self {
            title: title.into(),
            size,
            min_size: None,
            native_chrome: true,
        }
    }

    /// Make the dialog window user-resizable, no smaller than `min_size`.
    pub(crate) fn resizable(mut self, min_size: Size<Pixels>) -> Self {
        self.min_size = Some(min_size);
        self
    }

    /// Opt out of the macOS native decoration (no traffic lights).
    pub(crate) fn chromeless(mut self) -> Self {
        self.native_chrome = false;
        self
    }
}

/// EXP-285: extra left inset for a dialog's own header row on macOS while the
/// native traffic lights float over the content (close enabled, the disabled
/// minimize/zoom siblings still reserve their slots).
pub(crate) fn macos_chrome_inset() -> Pixels {
    if cfg!(target_os = "macos") {
        px(56.)
    } else {
        px(0.)
    }
}

type CanCloseFn = Rc<dyn Fn(&App) -> bool>;
type OnEnterFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a dialog's build closure hands back: the content view plus the
/// shell-level semantics that used to live on gpui-component's `Dialog`
/// builder (`title`, `keyboard(!busy)`, `on_ok`).
pub(crate) struct DialogContent {
    view: AnyView,
    /// Draw the standard header row (title + ✕) above the content.
    header: Option<SharedString>,
    /// Standard 16px padding + overflow scrolling around the content
    /// (`false` = the view owns the full window, e.g. create-issue).
    padded: bool,
    /// Gates Escape / ✕ / the OS close request. Default: always closable.
    can_close: Option<CanCloseFn>,
    /// Enter fallback when no focused editor consumed it (the old `on_ok`).
    on_enter: Option<OnEnterFn>,
}

impl DialogContent {
    pub(crate) fn new(view: impl Into<AnyView>) -> Self {
        Self {
            view: view.into(),
            header: None,
            padded: true,
            can_close: None,
            on_enter: None,
        }
    }

    pub(crate) fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.header = Some(title.into());
        self
    }

    pub(crate) fn padless(mut self) -> Self {
        self.padded = false;
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

// Duplicates `app::channel::APP_ID` via the compile-time channel feature —
// the same CLOUD_INSTANCE/undock precedent (ui cannot depend on app).
#[cfg(not(feature = "staging"))]
const CHANNEL_APP_ID: &str = "at.exponential";
#[cfg(feature = "staging")]
const CHANNEL_APP_ID: &str = "at.exponential.staging";

/// Open a native dialog window over `window` (the opener). `build` runs
/// INSIDE the new window and returns the content view + semantics.
///
/// No-op when a dialog is already up here — called from a dialog window
/// (never nest dialogs — App-global shortcut handlers can land here while a
/// dialog is active), or this window's dialog is still alive/in flight (a
/// double-triggered opener must not stack two windows).
pub(crate) fn open_dialog_window(
    window: &mut Window,
    cx: &mut App,
    spec: DialogSpec,
    build: impl FnOnce(&mut Window, &mut App) -> DialogContent + 'static,
) {
    if dialog_open_here(window, cx) {
        return;
    }
    let opener = window.window_handle();
    let opener_id = opener.window_id();
    // Latch the in-flight marker synchronously — the window itself only
    // registers once the spawn below actually opens it.
    if let Some(registry) = registry(cx) {
        registry.update(cx, |registry, _| {
            registry.pending.insert(opener_id);
        });
    }
    // Parent-relative positioning: centered over the opener's outer bounds
    // (macOS ignores this — the sheet auto-positions under the titlebar).
    let opener_bounds = window.bounds();
    let origin = point(
        opener_bounds.origin.x + (opener_bounds.size.width - spec.size.width) / 2.,
        opener_bounds.origin.y + (opener_bounds.size.height - spec.size.height) / 2.,
    );
    let bounds = Bounds { origin, size: spec.size };
    let title = spec.title.clone();
    let native_chrome = spec.native_chrome;
    let min_size = spec.min_size;

    // The gpui-component-sanctioned pattern: open windows inside a foreground
    // spawn (also dodges the re-entrant window-update trap — every caller is
    // a click/action handler inside a window update).
    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // EXP-285: a plain floating panel — NOT `Dialog`, which is a
            // macOS sheet (undraggable, auto-positioned) and a Windows
            // owner-modal popup (parent disabled — click-away dismissal
            // needs the opener alive and clickable).
            kind: WindowKind::Floating,
            // Native decoration on macOS: a transparent titlebar puts the
            // traffic lights over the content (close enabled; minimize/zoom
            // stay off via the flags below). Windows keeps the caption
            // hidden for transparent titlebars, Linux CSD draws only the
            // rounded `window_frame` — both keep the shell's header row as
            // the visible chrome.
            titlebar: native_chrome.then(|| gpui::TitlebarOptions {
                title: Some(title.clone()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.), px(12.))),
            }),
            is_resizable: min_size.is_some(),
            is_minimizable: false,
            window_min_size: min_size,
            app_id: Some(CHANNEL_APP_ID.to_string()),
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };
        let opened = cx.open_window(options, move |window, cx| {
            let content = build(window, cx);
            let shell =
                cx.new(|cx| DialogShell::new(opener, content, native_chrome, window, cx));
            // Root MUST be the first view of every window (§3.3) — it hosts
            // the popover/menu/notification overlay layers the dialog content
            // (chip dropdowns, date popovers) paints into.
            cx.new(|cx| {
                let root = Root::new(shell, window, cx);
                #[cfg(target_os = "linux")]
                let root = {
                    use gpui::Styled as _;
                    root.bordered(false).bg(gpui::transparent_black())
                };
                root
            })
        });
        // Drop the in-flight marker either way: on success `DialogShell::new`
        // has already registered the real row (the build closure runs
        // synchronously), on failure nothing may stay latched.
        cx.update(|cx| {
            if let Some(registry) = registry(cx) {
                registry.update(cx, |registry, _| {
                    registry.pending.remove(&opener_id);
                });
            }
        });
        let handle = opened?;
        handle.update(cx, |_, window, cx| {
            window.set_window_title(&title);
            window.activate_window();
            let _ = cx;
        })?;

        // EXP-285: click-away dismissal — observe the OPENER's activation.
        // The dialog closes when the user clicks back into the window that
        // opened it (⌘-tabbing to another app and back does not fire this:
        // the observer only reacts when the opener itself becomes active
        // while its dialog row is still registered). Registered AFTER the
        // dialog's own `activate_window` so opening never self-dismisses.
        let dialog_handle: AnyWindowHandle = handle.into();
        let dialog_id = dialog_handle.window_id();
        let _ = opener.update(cx, |_, opener_window, app| {
            let Some(registry_entity) = registry(app) else {
                return;
            };
            registry_entity.update(app, |registry, cx| {
                let sub = cx.observe_window_activation(
                    opener_window,
                    move |registry: &mut DialogRegistry, opener_window, cx| {
                        if !opener_window.is_window_active() {
                            return;
                        }
                        // The dialog may already be gone (normal close raced
                        // the opener re-activation) — the row is the truth.
                        let Some(row) = registry.openers.get(&dialog_id) else {
                            return;
                        };
                        let can_close = row.can_close.clone();
                        cx.defer(move |cx| {
                            let allowed = can_close.as_ref().map(|f| f(cx)).unwrap_or(true);
                            if allowed {
                                let _ = dialog_handle
                                    .update(cx, |_, window, _| window.remove_window());
                            }
                        });
                    },
                );
                if let Some(row) = registry.openers.get_mut(&dialog_id) {
                    row._dismiss = Some(sub);
                }
            });
        });
        anyhow::Ok(())
    })
    .detach();
}

/// The dialog window's root view: registry lifecycle, glass chrome, header
/// row, Escape/Enter handling, and the Root overlay layers.
struct DialogShell {
    opener: AnyWindowHandle,
    content: DialogContent,
    /// Whether the window carries macOS traffic lights over the content —
    /// header rows inset past them (EXP-285).
    native_chrome: bool,
    /// Header drag state (the titlebar `should_move` pattern, EXP-285).
    should_move: bool,
    focus_handle: FocusHandle,
}

impl DialogShell {
    fn new(
        opener: AnyWindowHandle,
        content: DialogContent,
        native_chrome: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Register FIRST so `close_then` resolves the opener from the first
        // frame; the release hook below unregisters symmetrically.
        let id = window.window_handle().window_id();
        if let Some(registry) = registry(cx) {
            let row = DialogRow {
                opener,
                can_close: content.can_close.clone(),
                _dismiss: None,
            };
            registry.update(cx, |registry, cx| {
                registry.openers.insert(id, row);
                cx.notify();
            });
        }

        // The OS close request (Alt-F4 / WM close / the sheet's implicit
        // dismissals) consults the same busy gate as Escape.
        if let Some(can_close) = content.can_close.clone() {
            window.on_window_should_close(cx, move |_, cx| can_close(cx));
        }

        cx.on_release(move |_, cx| {
            if let Some(registry) = registry(cx) {
                registry.update(cx, |registry, cx| {
                    registry.openers.remove(&id);
                    cx.notify();
                });
            }
            // App-global action handlers can lazily create per-window
            // registries against a focused dialog window — mirror the
            // undocked-window teardown so nothing leaks past the close.
            crate::navigation::remove_window(id, cx);
            crate::repo_resolver::remove_window(id, cx);
            crate::sidebar::remove_window(id, cx);
        })
        .detach();

        Self {
            opener,
            content,
            native_chrome,
            should_move: false,
            focus_handle: cx.focus_handle(),
        }
    }

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

impl Render for DialogShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // gpui builds the key dispatch path from the FOCUSED node and falls
        // back to the ROOT node when the window has no focus at all
        // (`Window::dispatch_key_event` → `focus_node_id_in_rendered_frame`),
        // so the `key_context(CONTEXT)` div below is off the path and
        // Escape/Enter are dead in a freshly opened dialog window until the
        // first click lands inside it. Claim focus while nothing else holds it
        // — the gpui-component overlay `Dialog` focused itself on open the
        // same way. Content views that autofocus an input still win: they do
        // it either in the `build` closure (search — runs before this view
        // exists) or in their own `render`, which runs after this one.
        if window.focused(cx).is_none() {
            self.focus_handle.focus(window, cx);
        }

        let _ = self.opener;
        let closable = self.can_close(cx);
        let on_enter = self.content.on_enter.clone();

        let header = self.content.header.clone().map(|title| {
            h_flex()
                .flex_shrink_0()
                .items_center()
                .justify_between()
                // EXP-285: clear the macOS traffic lights floating over the
                // content, and make the row a window-drag region (floating
                // windows have no other draggable chrome).
                .when(self.native_chrome, |header| header.pl(macos_chrome_inset()))
                .window_control_area(WindowControlArea::Drag)
                .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = true),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = false),
                )
                .on_mouse_move(cx.listener(|this, _, window, _| {
                    if this.should_move {
                        this.should_move = false;
                        window.start_window_move();
                    }
                }))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(crate::app_title_bar::interactive(
                    Button::new("native-dialog-close")
                        .ghost()
                        .xsmall()
                        .icon(
                            Icon::new(IconName::Close)
                                .small()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(!closable)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.can_close(cx) {
                                close_dialog_window(window, cx);
                            }
                        })),
                ))
        });

        let body: AnyElement = if self.content.padded {
            v_flex()
                .size_full()
                .p_4()
                .gap_3()
                .children(header)
                .child(
                    // Overflowing forms scroll instead of clipping the footer
                    // (the old overlay grew with its content; a window can't).
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(v_flex().size_full().overflow_y_scrollbar().child(
                            div().pr_1().child(self.content.view.clone()),
                        )),
                )
                .into_any_element()
        } else {
            div()
                .size_full()
                .child(self.content.view.clone())
                .into_any_element()
        };

        // Popover/menu/notification overlay layers — same composition rule as
        // `Shell::render`; without them chip dropdowns and `push_notification`
        // would silently never paint in this window.
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        crate::window_frame::window_frame().child(
            div()
                .size_full()
                // The glass dialog surface — the same page gradient as the
                // main window (EXP-285), opaque: there is no dimmed
                // in-window content behind a native window.
                .bg(theme::background_gradient())
                .border_1()
                .border_color(t::glass::STROKE_CARD.to_hsla())
                .text_color(cx.theme().foreground)
                .key_context(CONTEXT)
                .track_focus(&self.focus_handle)
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
                .child(body)
                .children(sheet_layer)
                .children(dialog_layer)
                .children(notification_layer),
        )
    }
}

// ---------------------------------------------------------------------------
// Alerts (confirm dialogs)
// ---------------------------------------------------------------------------

type OnOkFn = Rc<dyn Fn(&mut Window, &mut App) -> bool>;
type AlertContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A native confirm window: title, description, optional extra content block
/// (e.g. a typed-confirm input), Cancel + OK footer. The old
/// `open_alert_dialog` + `DialogButtonProps` surface.
pub(crate) struct AlertSpec {
    title: SharedString,
    description: SharedString,
    ok_text: SharedString,
    ok_variant: ButtonVariant,
    height: Pixels,
    /// Extra block between the description and the footer.
    content: Option<AlertContentFn>,
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
            description: description.into(),
            ok_text: ok_text.into(),
            ok_variant: ButtonVariant::Primary,
            // EXP-285: trimmed 240 → 220 — alerts size closer to content.
            height: px(220.),
            content: None,
            on_ok: Rc::new(|_, _| true),
        }
    }

    pub(crate) fn ok_variant(mut self, variant: ButtonVariant) -> Self {
        self.ok_variant = variant;
        self
    }

    pub(crate) fn height(mut self, height: Pixels) -> Self {
        self.height = height;
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
}

/// Open a native confirm window over `window` (the opener).
pub(crate) fn open_alert(window: &mut Window, cx: &mut App, spec: AlertSpec) {
    let dialog_size = size(px(416.), spec.height);
    let title = spec.title.clone();
    open_dialog_window(window, cx, DialogSpec::new(title, dialog_size), move |_, cx| {
        let view = cx.new(|_| AlertView { spec, should_move: false });
        let on_enter = view.clone();
        DialogContent::new(view)
            .padless()
            .on_enter(move |window, cx| AlertView::confirm(&on_enter, window, cx))
    });
}

struct AlertView {
    spec: AlertSpec,
    /// Header drag state (the titlebar `should_move` pattern, EXP-285).
    should_move: bool,
}

impl AlertView {
    fn confirm(view: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let on_ok = view.read(cx).spec.on_ok.clone();
        if on_ok(window, cx) {
            close_dialog_window(window, cx);
        }
    }
}

impl Render for AlertView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let extra = self.spec.content.as_ref().map(|content| content(window, cx));

        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    // EXP-285: clear the macOS traffic lights + drag region
                    // (alerts always carry native chrome).
                    .pl(macos_chrome_inset())
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = true),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| this.should_move = false),
                    )
                    .on_mouse_move(cx.listener(|this, _, window, _| {
                        if this.should_move {
                            this.should_move = false;
                            window.start_window_move();
                        }
                    }))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.spec.title.clone()),
                    )
                    .child(crate::app_title_bar::interactive(
                        Button::new("native-alert-close")
                            .ghost()
                            .xsmall()
                            .icon(
                                Icon::new(IconName::Close)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .on_click(|_, window, cx| close_dialog_window(window, cx)),
                    )),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.spec.description.clone()),
                    )
                    .children(extra),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("native-alert-cancel")
                            .outline()
                            .small()
                            .label("Cancel")
                            .on_click(|_, window, cx| close_dialog_window(window, cx)),
                    )
                    .child(
                        Button::new("native-alert-ok")
                            .with_variant(self.spec.ok_variant)
                            .small()
                            .label(self.spec.ok_text.clone())
                            .on_click(move |_, window, cx| {
                                Self::confirm(&view, window, cx);
                            }),
                    ),
            )
    }
}
