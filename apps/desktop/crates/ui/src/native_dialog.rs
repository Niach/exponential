//! Native dialog windows (EXP-284): every dialog/alert spawns as a real OS
//! window instead of gpui-component's in-window Dialog overlay layer.
//!
//! The shared shape is [`open_dialog_window`] (content dialogs) and
//! [`open_alert`] (confirm/alert dialogs) over one [`DialogShell`] root view:
//!
//! - **Window options** (EXP-287, Linux split in EXP-308): on macOS/Windows a
//!   dialog is an ORDINARY secondary app window — `WindowKind::Normal` with
//!   the main window's chrome (`gpui_component::TitleBar::title_bar_options()`),
//!   minimizable on Windows only (a minimized macOS dialog folds into the
//!   app's Dock icon instead of earning its own tile — nothing visible left
//!   to restore it from). That is what earns
//!   it its own taskbar/window-list button, which is the point: dialogs are
//!   managed by hand now, so they must be findable. `WindowKind::Floating`
//!   could not deliver that on macOS — it allocs an `NSPanel` (never listed,
//!   and `hidesOnDeactivate` defaults YES, so every dialog blinked out
//!   whenever the app deactivated). On LINUX the trade goes the other way
//!   (EXP-308): `Normal` dialogs opened wherever the WM's placement policy
//!   dropped them (top-left cascade on GNOME) instead of over the IDE window,
//!   because our centered origin never survives — gpui's X11 backend sets no
//!   position hint in `WM_NORMAL_HINTS`, so the WM ignores the CreateWindow
//!   origin, and Wayland has no client positioning at all. The ONLY
//!   parent-centered placement either backend offers is the transient parent
//!   relationship, so Linux dialogs are `Floating` (`WM_TRANSIENT_FOR` /
//!   `xdg_toplevel.set_parent` on the opener — WMs center transients over
//!   their parent), accepting that the taskbar folds them into the opener's
//!   button; a lost dialog is still recoverable by re-triggering its opener
//!   ([`raise_existing_dialog`]). `Floating` also survives everywhere for
//!   [`DialogSpec::chromeless`] dialogs (the ⌘K palette, the image lightbox):
//!   a palette is not a document window and wants to stay above its opener.
//! - **Parent-relative positioning**: bounds are centered over the opener's
//!   window bounds at open time (honoured on macOS/Windows; on Linux the
//!   WM/compositor centers the transient itself — see above).
//! - **Focus/dismiss semantics**: a dialog closes only when the user says so
//!   — Escape, the titlebar's close control, or the OS close request
//!   (Alt-F4 / WM close), all gated by the per-dialog `can_close` so a busy
//!   submit keeps its window up. Enter runs the per-dialog `on_enter` submit
//!   fallback. **There is no auto-dismissal** (EXP-287 removed EXP-285's
//!   opener-activation observer): clicking, minimizing or ⌘-tabbing away
//!   from the opener leaves the dialog exactly where it is. Re-triggering an
//!   opener that already has a dialog RAISES that window instead of silently
//!   dropping the request.
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
    Render, SharedString, Size, Styled, Subscription, Window, WindowBounds, WindowControlArea,
    WindowId, WindowKind, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Root, Sizable as _,
};
use theme::tokens as t;
use crate::icons::registry;

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
    /// This dialog's own window — EXP-287 raises it when its opener
    /// re-triggers, and closes it when its opener dies.
    handle: AnyWindowHandle,
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
fn dialog_open_here(window: &Window, cx: &App) -> bool {
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

/// The live dialog window this window opened, if any.
fn live_dialog_of(window: &Window, cx: &App) -> Option<AnyWindowHandle> {
    let id = window.window_handle().window_id();
    let registry = registry(cx)?;
    let registry = registry.read(cx);
    registry
        .openers
        .values()
        .find(|row| row.opener.window_id() == id)
        .map(|row| row.handle)
}

/// EXP-287: the never-stack guard, now with a recovery path. Returns `true`
/// when the caller must NOT open a dialog — either because it already has a
/// live one (which this raises: dialogs no longer auto-dismiss, so a repeat
/// trigger has to be able to find the window it already opened, and the
/// buried-behind-the-opener case has to be recoverable), or because opening
/// is illegal here at all (this window IS a dialog — never nest — or its
/// dialog is spawned but not yet up).
///
/// Called by [`open_dialog_window`] itself and by the two openers that must
/// decide before building any state (⌘K spam, an invite deep link arriving
/// mid-dialog).
pub(crate) fn raise_existing_dialog(window: &mut Window, cx: &mut App) -> bool {
    if let Some(existing) = live_dialog_of(window, cx) {
        // Deferred: every caller is inside a window update.
        cx.defer(move |cx| {
            let _ = existing.update(cx, |_, window, _| window.activate_window());
        });
        return true;
    }
    dialog_open_here(window, cx)
}

fn opener_of(window: &Window, cx: &App) -> Option<AnyWindowHandle> {
    let id = window.window_handle().window_id();
    registry(cx).and_then(|registry| registry.read(cx).openers.get(&id).map(|row| row.opener))
}

/// EXP-287: a shell window is going away — close the dialogs it opened.
/// Nothing else would: a dialog is an independent `WindowKind::Normal` window
/// now, with its own taskbar button and no auto-dismissal, so an orphan would
/// linger as a user-visible zombie pointing at a dead opener. Mirrors
/// [`crate::undock::on_shell_released`], which the same hook site calls for
/// undocked terminals.
pub(crate) fn on_opener_released(opener: WindowId, cx: &mut App) {
    let Some(registry) = registry(cx) else {
        return;
    };
    let doomed: Vec<AnyWindowHandle> = registry
        .read(cx)
        .openers
        .values()
        .filter(|row| row.opener.window_id() == opener)
        .map(|row| row.handle)
        .collect();
    if doomed.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for handle in doomed {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    });
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
    /// OS window title — the taskbar/window-list label, and (EXP-287) the
    /// label the shell's titlebar strip draws for native-chrome dialogs.
    pub title: SharedString,
    /// Inner content size, below the titlebar strip. Callers cap against the
    /// opener's viewport.
    pub size: Size<Pixels>,
    /// EXP-285: user-resizable window with this floor. `None` = fixed size.
    min_size: Option<Size<Pixels>>,
    /// EXP-287: full window chrome — the main window's titlebar options plus
    /// the shell's [`crate::title_bar::TitleBar`] strip. `false` = fully
    /// frameless AND still a floating panel (search palette, image preview:
    /// chrome would overlap the input/image, and neither is a window the user
    /// manages by hand).
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

    /// Opt out of the window chrome: no titlebar strip, no native decoration,
    /// and the window stays a floating panel above its opener.
    pub(crate) fn chromeless(mut self) -> Self {
        self.native_chrome = false;
        self
    }
}

type CanCloseFn = Rc<dyn Fn(&App) -> bool>;
type OnEnterFn = Rc<dyn Fn(&mut Window, &mut App)>;
type TitleContentFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
type TitleObservesFn = Box<dyn FnOnce(&mut gpui::Context<DialogShell>) -> Subscription>;

/// What a dialog's build closure hands back: the content view plus the
/// shell-level semantics that used to live on gpui-component's `Dialog`
/// builder (`title`, `keyboard(!busy)`, `on_ok`).
pub(crate) struct DialogContent {
    view: AnyView,
    /// In-content header row (title + ✕) above the content. Honoured ONLY by
    /// [`DialogSpec::chromeless`] dialogs — see
    /// [`DialogContent::chromeless_header`].
    header: Option<SharedString>,
    /// EXP-426: extra controls rendered in the chromeless header's right
    /// cluster, before the ✕ (the image lightbox's "Open in browser").
    header_actions: Option<TitleContentFn>,
    /// EXP-287: rich titlebar label for native-chrome dialogs (create-issue's
    /// board pill). `None` = plain `DialogSpec::title` text.
    title_content: Option<TitleContentFn>,
    /// EXP-449: installs the subscription that repaints the SHELL when the
    /// content view changes — see [`DialogContent::title_follows`].
    title_observes: Option<TitleObservesFn>,
    /// Standard 16px padding + overflow scrolling around the content
    /// (`false` = the view owns the full window, e.g. create-issue).
    padded: bool,
    /// EXP-291: keep the padded/header chrome but hand the view a DEFINITE
    /// full-height body box instead of the shell's scroller (see
    /// [`DialogContent::self_scrolling`]).
    self_scrolling: bool,
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
            header_actions: None,
            title_content: None,
            title_observes: None,
            padded: true,
            self_scrolling: false,
            can_close: None,
            on_enter: None,
        }
    }

    /// Draw an in-content header row (title + ✕) above the content.
    ///
    /// EXP-287 narrowed this to [`DialogSpec::chromeless`] dialogs: a
    /// native-chrome dialog draws `DialogSpec::title` in its real titlebar
    /// strip, so a second header row would just repeat it. The one caller
    /// left is the image lightbox, where the ✕ is the only mouse dismissal.
    pub(crate) fn chromeless_header(mut self, title: impl Into<SharedString>) -> Self {
        self.header = Some(title.into());
        self
    }

    /// EXP-426: extra header controls in the chromeless header's right
    /// cluster, next to the ✕. They render inside the shared
    /// [`crate::app_title_bar::interactive`] wrapper, so clicks reach them
    /// instead of starting a window drag. Requires [`Self::chromeless_header`].
    pub(crate) fn chromeless_header_actions(
        mut self,
        actions: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.header_actions = Some(Rc::new(actions));
        self
    }

    /// EXP-287: replace the titlebar strip's plain title text with custom
    /// content (create-issue's board pill breadcrumb). Native-chrome dialogs
    /// only — the shell wraps it in [`crate::app_title_bar::interactive`]
    /// like every other bar occupant.
    pub(crate) fn title_content(
        mut self,
        title_content: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.title_content = Some(Rc::new(title_content));
        self
    }

    /// EXP-449: repaint the shell whenever `entity` notifies. The shell holds
    /// the [`Self::title_content`] closure but observes nothing, so a title
    /// that reads the content view's state (create-issue's live board select)
    /// would otherwise render once and go stale.
    pub(crate) fn title_follows<V: 'static>(mut self, entity: &Entity<V>) -> Self {
        let entity = entity.clone();
        self.title_observes = Some(Box::new(move |cx| cx.observe(&entity, |_, _, cx| cx.notify())));
        self
    }

    pub(crate) fn padless(mut self) -> Self {
        self.padded = false;
        self
    }

    /// EXP-291: keep the standard padded body + header row, but let the
    /// CONTENT view own its scrolling. The view's root is handed a definite
    /// full-height box (a `size_full()` root resolves against it, exactly
    /// like [`DialogContent::padless`]), so it can pin an action bar at the
    /// bottom and scroll only its own body. The default wrapper instead
    /// scrolls the WHOLE view — footer included, which is what made the
    /// Start-coding dialog's Cancel/Start buttons scroll out of reach.
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

// Duplicates `app::channel::APP_ID` via the compile-time channel feature —
// the same CLOUD_INSTANCE/undock precedent (ui cannot depend on app).
#[cfg(not(feature = "staging"))]
const CHANNEL_APP_ID: &str = "at.exponential";
#[cfg(feature = "staging")]
const CHANNEL_APP_ID: &str = "at.exponential.staging";

/// Open a native dialog window over `window` (the opener). `build` runs
/// INSIDE the new window and returns the content view + semantics.
///
/// Never stacks: called from a dialog window this is a no-op (never nest —
/// App-global shortcut handlers can land here while a dialog is active), and
/// a repeat trigger from an opener that already has one RAISES that window
/// (EXP-287 — see [`raise_existing_dialog`]).
pub(crate) fn open_dialog_window(
    window: &mut Window,
    cx: &mut App,
    spec: DialogSpec,
    build: impl FnOnce(&mut Window, &mut App) -> DialogContent + 'static,
) {
    if raise_existing_dialog(window, cx) {
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
    let native_chrome = spec.native_chrome;
    // EXP-287: `DialogSpec::size` is the CONTENT box; a native-chrome dialog
    // now carries a real 34px titlebar strip above it. Grow the window by that
    // strip and re-apply the opener-viewport cap every caller already respects
    // (several ask for `0.85 * viewport`), so no dialog can outgrow the screen
    // it was sized against.
    let mut size = spec.size;
    if native_chrome {
        size.height += crate::title_bar::TITLE_BAR_HEIGHT;
        size.height = size.height.min(window.viewport_size().height);
    }
    // Parent-relative positioning: centered over the opener's outer bounds.
    let opener_bounds = window.bounds();
    let origin = point(
        opener_bounds.origin.x + (opener_bounds.size.width - size.width) / 2.,
        opener_bounds.origin.y + (opener_bounds.size.height - size.height) / 2.,
    );
    let bounds = Bounds { origin, size };
    let title = spec.title.clone();
    let min_size = spec.min_size.map(|mut min| {
        if native_chrome {
            min.height += crate::title_bar::TITLE_BAR_HEIGHT;
        }
        min
    });

    // The gpui-component-sanctioned pattern: open windows inside a foreground
    // spawn (also dodges the re-entrant window-update trap — every caller is
    // a click/action handler inside a window update).
    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // EXP-287: an ordinary secondary app window, exactly like an
            // undocked terminal (`undock::undocked_window_options`) — that is
            // what earns it a taskbar/window-list button of its own, which a
            // hand-managed, never-auto-dismissed dialog needs. `Floating`
            // cannot: on macOS it allocs an NSPanel (never listed, and
            // `hidesOnDeactivate` defaults YES, which blinked every dialog out
            // whenever the app deactivated).
            //
            // EXP-308: Linux is the exception — `Normal` dialogs landed
            // wherever the WM's placement policy put them (top-left cascade)
            // instead of over the opener: gpui's X11 backend sets no position
            // hint in WM_NORMAL_HINTS so the WM discards `bounds.origin`, and
            // Wayland cannot position toplevels at all. The transient parent
            // relationship `Floating` stamps (WM_TRANSIENT_FOR /
            // `xdg_toplevel.set_parent` on the keyboard-focused opener) is the
            // one mechanism both backends have that centers a window over its
            // parent, so Linux trades the dialog's own taskbar button for
            // correct placement (`raise_existing_dialog` keeps a buried dialog
            // recoverable from its opener).
            //
            // Chromeless dialogs stay `Floating` on every platform: the ⌘K
            // palette and the image lightbox are transient surfaces, not
            // windows the user manages, and both want to stay above their
            // opener.
            kind: if native_chrome && !cfg!(target_os = "linux") {
                WindowKind::Normal
            } else {
                WindowKind::Floating
            },
            // EXP-287: the main window's titlebar options (`app::windows`) —
            // macOS traffic lights at (9,9) floating over the shell's own
            // `TitleBar` strip, Windows caption suppressed, Linux CSD. The
            // strip below carries the drag region and, off macOS, the
            // min/close controls.
            titlebar: native_chrome.then(|| gpui::TitlebarOptions {
                title: Some(title.clone()),
                ..gpui_component::TitleBar::title_bar_options()
            }),
            is_resizable: min_size.is_some(),
            // EXP-287 wanted minimize wherever the dialog has its own
            // taskbar/window-list button. EXP-308: that is Windows-only now —
            // the Linux dialog is a transient with NO taskbar button, and a
            // minimized macOS dialog folds into the app's Dock icon instead
            // of earning a tile of its own — on both, minimizing would strand
            // the window with no visible restore affordance. (On macOS this
            // is what disables the yellow traffic light.)
            is_minimizable: cfg!(target_os = "windows"),
            window_min_size: min_size,
            app_id: Some(CHANNEL_APP_ID.to_string()),
            // EXP-290 glass: non-opaque + behind-window blur, same as the shell
            // (`app::windows` carries the per-platform rationale). A small
            // window over the desktop is the surface the glass reads best on —
            // and on macOS the blurred backdrop is what keeps its 0.92-alpha
            // page from showing the raw desktop through. Windows stays Opaque.
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };
        let shell_title = title.clone();
        let opened = cx.open_window(options, move |window, cx| {
            let content = build(window, cx);
            let shell = cx.new(|cx| {
                DialogShell::new(
                    opener,
                    content,
                    shell_title,
                    native_chrome,
                    min_size.is_some(),
                    window,
                    cx,
                )
            });
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
                // EXP-290 macOS glass: clear Root's opaque full-window
                // `tokens.background` fill — it would sit between the blurred
                // backdrop and the translucent page gradient.
                #[cfg(target_os = "macos")]
                let root = {
                    use gpui::Styled as _;
                    root.bg(gpui::transparent_black())
                };
                root
            })
        });
        // EXP-303: give the new Blurred window a working frosted backdrop
        // (gpui's own BlurredView renders none on current macOS).
        #[cfg(target_os = "macos")]
        crate::macos_blur::ensure_window_blur();
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

        // EXP-287: on X11 the dialog needs the `_NET_WM_ICON` stamp — alt-tab
        // (and, pre-EXP-308, its taskbar button) reads it, and the launch-time
        // pass in `app::x11_window_icon` is long gone by the time one opens.
        // No-op everywhere else.
        cx.update(|cx| crate::window_hooks::notify_window_opened(cx));
        anyhow::Ok(())
    })
    .detach();
}

/// The dialog window's root view: registry lifecycle, glass chrome, the
/// titlebar strip, Escape/Enter handling, and the Root overlay layers.
struct DialogShell {
    opener: AnyWindowHandle,
    content: DialogContent,
    /// Window title — the titlebar strip's label unless the content supplies
    /// its own [`DialogContent::title_content`].
    title: SharedString,
    /// Whether this window carries the full chrome: a real titlebar strip
    /// (EXP-287) and, on macOS, the native traffic lights floating over it.
    native_chrome: bool,
    /// Whether the window is user-resizable — a fixed-size window must not
    /// draw a maximize control it cannot honour.
    resizable: bool,
    /// Chromeless header drag state (the `should_move` titlebar pattern).
    should_move: bool,
    focus_handle: FocusHandle,
    /// EXP-449: repaints this shell when the content view notifies — see
    /// [`DialogContent::title_follows`].
    _title_subscription: Option<Subscription>,
}

impl DialogShell {
    fn new(
        opener: AnyWindowHandle,
        mut content: DialogContent,
        title: SharedString,
        native_chrome: bool,
        resizable: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Register FIRST so `close_then` resolves the opener from the first
        // frame; the release hook below unregisters symmetrically.
        let handle = window.window_handle();
        let id = handle.window_id();
        if let Some(registry) = registry(cx) {
            let row = DialogRow { opener, handle };
            registry.update(cx, |registry, cx| {
                registry.openers.insert(id, row);
                cx.notify();
            });
        }

        // The OS close request (Alt-F4 / WM close / the macOS traffic light /
        // the Windows caption hitbox) consults the same busy gate as Escape.
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

        let title_subscription = content.title_observes.take().map(|observe| observe(cx));

        Self {
            opener,
            content,
            title,
            native_chrome,
            resizable,
            should_move: false,
            focus_handle: cx.focus_handle(),
            _title_subscription: title_subscription,
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

        // EXP-287: native-chrome dialogs wear the MAIN WINDOW's chrome — the
        // vendored `TitleBar` strip (drag zone, double-click zoom, the Linux
        // right-click WM menu, the macOS traffic-light gutter, and off macOS
        // the min/close controls), exactly like an undocked window
        // (`undock::UndockedScreenWindow::render`). Under the Linux
        // server-decoration fallback the WM already draws all of that, so the
        // strip degrades to a plain labelled band.
        //
        // Chromeless dialogs (⌘K palette, image lightbox) keep the old
        // in-content header row: a titlebar would sit on the search input,
        // and the row's ✕ is the lightbox's only mouse dismissal.
        let title_child: AnyElement = match self.content.title_content.clone() {
            Some(title_content) => title_content(window, cx),
            None => div().text_sm().child(self.title.clone()).into_any_element(),
        };
        let chrome: Option<AnyElement> = self.native_chrome.then(|| {
            if crate::app_title_bar::client_chrome(window) {
                crate::title_bar::TitleBar::new()
                    // A fixed-size dialog has no maximize affordance at the OS
                    // level, so the control would be dead chrome. EXP-308: on
                    // Linux the dialog is a taskbar-less transient, so the
                    // minimize control goes too — the strip's Linux handler
                    // calls `minimize_window()` directly, which would strand
                    // the window with nothing to restore it from.
                    .window_controls(!cfg!(target_os = "linux"), self.resizable)
                    // Linux only (upstream's gate, kept): `remove_window` never
                    // consults `on_window_should_close`, so this is the only
                    // place a busy submit can refuse the strip's own ✕. The
                    // Windows/macOS close controls route through the OS and hit
                    // the same gate via `on_window_should_close`.
                    .on_close_window(cx.listener(|this, _, window, cx| {
                        if this.can_close(cx) {
                            close_dialog_window(window, cx);
                        }
                    }))
                    .child(crate::app_title_bar::interactive(title_child))
                    .into_any_element()
            } else {
                h_flex()
                    .h(crate::title_bar::TITLE_BAR_HEIGHT)
                    .w_full()
                    .flex_shrink_0()
                    .items_center()
                    .px_3()
                    .child(title_child)
                    .into_any_element()
            }
        });

        let header_actions: Option<AnyElement> = self
            .content
            .header_actions
            .clone()
            .map(|actions| actions(window, cx));
        let header = self.content.header.clone().map(|title| {
            h_flex()
                .flex_shrink_0()
                .items_center()
                .justify_between()
                // The header carries its own padding (the body's p_4 used to
                // leave a 16px undraggable band above it).
                .px_4()
                .pt_3()
                // Chromeless windows have no other draggable chrome.
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
                    h_flex()
                        .items_center()
                        .gap_1()
                        .children(header_actions)
                        .child(
                            Button::new("native-dialog-close")
                                .ghost()
                                .xsmall()
                                .icon(
                                    Icon::new(registry::UI_CLOSE)
                                        .small()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .disabled(!closable)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.can_close(cx) {
                                        close_dialog_window(window, cx);
                                    }
                                })),
                        ),
                ))
        });

        let body: AnyElement = if self.content.padded {
            let has_header = header.is_some();
            // Overflowing forms scroll instead of clipping the footer (the
            // old overlay grew with its content; a window can't) — unless the
            // view is `self_scrolling` (EXP-291), which trades the wrapper
            // for a plain definite-height box the view fills itself.
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
            let content = div().flex_1().min_h_0().child(inner);
            if has_header {
                // EXP-288: the header owns the top padding (drag region
                // reaches y=0); only the content wrapper pads the rest.
                v_flex()
                    .size_full()
                    .gap_3()
                    .children(header)
                    .child(content.px_4().pb_4())
                    .into_any_element()
            } else {
                v_flex()
                    .size_full()
                    .p_4()
                    .gap_3()
                    .child(content)
                    .into_any_element()
            }
        } else if let Some(row) = header {
            // EXP-415: a padless view can still wear the chromeless header
            // (search) — the row carries its own padding, the view keeps the
            // rest of the window.
            v_flex()
                .size_full()
                .child(row)
                .child(div().flex_1().min_h_0().child(self.content.view.clone()))
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
            // EXP-269 corners: see `window_frame::frame_radii` — an
            // edge-painting layer must round itself (this one carries the
            // dialog's own border, which follows the same arc).
            crate::window_frame::round_to_frame(div(), window)
                .size_full()
                // The glass dialog surface — the same page gradient as the
                // main window (EXP-285). It carries EXP-290's 0.92 alpha like
                // every other window root, so the blurred backdrop shows
                // through; there is never dimmed in-window content behind a
                // native window, only the desktop.
                .bg(theme::background_gradient())
                // Off Linux there is no `window_frame` stroke, so the dialog
                // draws its own card edge. Under Linux CSD `window_frame`
                // already paints a 1px `window_border` around the rounded
                // frame, and a second stroke inside it both doubles the line
                // and shifts the inner arc 1px off the radius `TitleBar` (and
                // its close button's hover fill) rounds to — the EXP-269
                // square-corner class of bug. `Shell` and the undocked windows
                // carry no border here for the same reason.
                .when(!cfg!(target_os = "linux"), |root| {
                    root.border_1().border_color(t::glass::STROKE_CARD.to_hsla())
                })
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
                .child(
                    v_flex()
                        .size_full()
                        .children(chrome)
                        .child(div().flex_1().min_h_0().child(body)),
                )
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
    /// Total window height, titlebar strip included (see [`AlertSpec::height`]).
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

    /// Total window height (the titlebar strip is part of it — EXP-287 kept
    /// this meaning so the existing per-alert overrides still read true).
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
    // `AlertSpec::height` is the whole window; `DialogSpec::size` is the
    // content below the titlebar strip (EXP-287).
    let content_height = (spec.height - crate::title_bar::TITLE_BAR_HEIGHT).max(px(0.));
    let dialog_size = size(px(416.), content_height);
    let title = spec.title.clone();
    open_dialog_window(window, cx, DialogSpec::new(title, dialog_size), move |_, cx| {
        let view = cx.new(|_| AlertView { spec });
        let on_enter = view.clone();
        DialogContent::new(view)
            .padless()
            .on_enter(move |window, cx| AlertView::confirm(&on_enter, window, cx))
    });
}

struct AlertView {
    spec: AlertSpec,
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

        // EXP-287: no header row here — the shell's titlebar strip carries the
        // alert title (which is also the OS window title) and the drag region.
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    // EXP-287: a description longer than the alert's fixed
                    // height used to clip silently; scroll it instead (the
                    // footer below stays pinned either way).
                    .overflow_y_scrollbar()
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
