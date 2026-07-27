//! EXP-303 macOS: a working frosted-glass backdrop for every blurred window.
//!
//! gpui's `WindowBackgroundAppearance::Blurred` installs its own
//! `NSVisualEffectView` subclass ("BlurredView", material `Selection`) and
//! then performs private-layer surgery in `updateLayer` (hides
//! `CAChameleonLayer`, strips filters, nils layer backgrounds) to get a
//! colorless blur. On current macOS that surgery leaves the effect view's
//! layer tree without any `CABackdropLayer` at all — the window renders
//! SHARP see-through instead of frosted glass (verified live with lldb:
//! the BlurredView's layer had one plain CALayer sublayer and no backdrop).
//!
//! We do not patch gpui (LD: no fork). Instead this inserts a stock backdrop
//! view — `NSGlassEffectView` (the macOS 26+ Liquid Glass API, the look the
//! product targets) when the class exists, else a stock `NSVisualEffectView`
//! (material `underWindowBackground`, behind-window blending, state active) —
//! as a sibling directly ABOVE gpui's BlurredView and below the `GPUIView`
//! metal view, so the app's translucent page gradients composite over a real
//! frosted backdrop. Both variants were live-injected via lldb and visually
//! confirmed to frost the window; zero layer surgery on either.
//!
//! [`ensure_window_blur`] scans ALL of the app's windows and is idempotent
//! (a backdrop subview of either stock class marks a window as done — gpui's
//! own subclass has a distinct class name), so callers just invoke it after
//! opening any window: the Shell windows, undocked windows and native
//! dialogs all go through it.

#![cfg(target_os = "macos")]

use cocoa::base::{id, nil, BOOL, NO};
use cocoa::foundation::{NSRect, NSString};
use objc::runtime::Class;
use objc::{class, msg_send, sel, sel_impl};

/// `NSVisualEffectMaterial.underWindowBackground` — the semantic material for
/// exactly this job (the backdrop under a window's own background). gpui's
/// `Selection` material is what breaks under its layer surgery; we use the
/// stock window-backdrop material with no surgery at all.
const MATERIAL_UNDER_WINDOW_BACKGROUND: i64 = 21;
/// `NSVisualEffectState.active` — always frosted, matching gpui's own choice
/// (the app's glass must not turn off when the window deactivates).
const STATE_ACTIVE: i64 = 1;
/// `NSVisualEffectBlendingMode.behindWindow` — sample the desktop behind the
/// window, not sibling in-window content.
const BLEND_BEHIND_WINDOW: i64 = 0;
/// `NSViewWidthSizable | NSViewHeightSizable`.
const AUTORESIZE_WH: u64 = (1 << 1) | (1 << 4);
/// `NSWindowOrderingMode.NSWindowAbove`.
const NS_WINDOW_ABOVE: i64 = 1;

/// Give every gpui-blurred window of this app a REAL frosted backdrop.
/// Idempotent and cheap — call after opening any window. Main thread only
/// (all call sites run inside gpui window/open contexts).
pub fn ensure_window_blur() {
    unsafe {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        let windows: id = msg_send![app, windows];
        let count: usize = msg_send![windows, count];
        for i in 0..count {
            let win: id = msg_send![windows, objectAtIndex: i];
            let opaque: BOOL = msg_send![win, isOpaque];
            if opaque != NO {
                continue;
            }
            let content: id = msg_send![win, contentView];
            if content == nil {
                continue;
            }
            let subviews: id = msg_send![content, subviews];
            let n: usize = msg_send![subviews, count];
            let mut gpui_blur: id = nil;
            let mut have_ours = false;
            for j in 0..n {
                let v: id = msg_send![subviews, objectAtIndex: j];
                let cls: &Class = msg_send![v, class];
                match cls.name() {
                    "BlurredView" => gpui_blur = v,
                    "NSVisualEffectView" | "NSGlassEffectView" => have_ours = true,
                    _ => {}
                }
            }
            // Only windows gpui itself marked Blurred (identified by its
            // BlurredView) get the backdrop — other non-opaque windows are
            // none of our business. Skip windows already fixed.
            if have_ours || gpui_blur == nil {
                continue;
            }
            let bounds: NSRect = msg_send![content, bounds];
            // Prefer the macOS 26+ Liquid Glass view (runtime-gated — the
            // class simply does not exist on older systems); fall back to the
            // classic frosted material everywhere else.
            let ev: id = if let Some(glass_cls) = Class::get("NSGlassEffectView") {
                let ev: id = msg_send![glass_cls, alloc];
                let ev: id = msg_send![ev, initWithFrame: bounds];
                ev
            } else {
                let ev: id = msg_send![class!(NSVisualEffectView), alloc];
                let ev: id = msg_send![ev, initWithFrame: bounds];
                let _: () = msg_send![ev, setMaterial: MATERIAL_UNDER_WINDOW_BACKGROUND];
                let _: () = msg_send![ev, setBlendingMode: BLEND_BEHIND_WINDOW];
                let _: () = msg_send![ev, setState: STATE_ACTIVE];
                ev
            };
            let _: () = msg_send![ev, setAutoresizingMask: AUTORESIZE_WH];
            // The app is dark-only (web `html.dark` parity) — pin the
            // material's appearance so a light-mode system doesn't render a
            // milky bright frost under the dark page gradients.
            let dark_name: id = NSString::alloc(nil).init_str("NSAppearanceNameDarkAqua");
            let dark: id = msg_send![class!(NSAppearance), appearanceNamed: dark_name];
            let _: () = msg_send![dark_name, release];
            if dark != nil {
                let _: () = msg_send![ev, setAppearance: dark];
            }
            // Directly above gpui's (inert) BlurredView, below the GPUIView
            // metal surface — the translucent page composites over our frost.
            let _: () = msg_send![content, addSubview: ev positioned: NS_WINDOW_ABOVE relativeTo: gpui_blur];
            let _: () = msg_send![ev, release];
        }
    }
}
