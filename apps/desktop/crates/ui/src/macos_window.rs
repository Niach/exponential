//! macOS: resize a window while keeping its TOP edge where it is (EXP-716).
//!
//! gpui's `Window::resize` is `-[NSWindow setContentSize:]`, and AppKit's
//! coordinate space grows from the bottom-left — so a content-sized palette
//! that grows as results land would climb UP the screen instead of extending
//! down like a dropdown (Windows' `SetWindowPos(SWP_NOMOVE)` and X11's
//! configure both keep the top-left, so macOS is the odd one out). This
//! re-derives the frame from the content rect with the top pinned and hands
//! it to `-setFrame:display:`, which takes the same `windowDidResize` path a
//! hand-drag does, so gpui's viewport follows on its own.
//!
//! Runs OFF any window/app borrow: `setFrame:` notifies the delegate
//! synchronously, and gpui's resize callback re-enters the App to update the
//! window's bounds — the reason gpui's own `resize` spawns onto the
//! foreground executor first. We do the same; the NSWindow is retained across
//! the hop and `isVisible` gates a window closed in between (gpui opens its
//! windows `releasedWhenClosed: NO`, so the pointer stays valid either way).

#![cfg(target_os = "macos")]

use cocoa::base::{id, nil, BOOL, NO, YES};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use gpui::{App, Pixels, Size, Window};
use objc::{msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Resize `window`'s CONTENT box to `size`, keeping its top-left corner fixed.
/// Returns `false` when the native window cannot be reached — callers fall
/// back to `Window::resize`.
pub(crate) fn resize_keeping_top(window: &Window, cx: &App, size: Size<Pixels>) -> bool {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return false;
    };
    let ns_view = appkit.ns_view.as_ptr() as id;
    let ns_window: id = unsafe { msg_send![ns_view, window] };
    if ns_window == nil {
        return false;
    }
    let _: id = unsafe { msg_send![ns_window, retain] };
    let width = f64::from(size.width);
    let height = f64::from(size.height);
    cx.foreground_executor()
        .spawn(async move {
            unsafe {
                let visible: BOOL = msg_send![ns_window, isVisible];
                if visible != NO {
                    let frame: NSRect = msg_send![ns_window, frame];
                    let content: NSRect = msg_send![ns_window, contentRectForFrameRect: frame];
                    let top = content.origin.y + content.size.height;
                    let next = NSRect::new(
                        NSPoint::new(content.origin.x, top - height),
                        NSSize::new(width, height),
                    );
                    let next_frame: NSRect = msg_send![ns_window, frameRectForContentRect: next];
                    let _: () = msg_send![ns_window, setFrame: next_frame display: YES];
                }
                let _: () = msg_send![ns_window, release];
            }
        })
        .detach();
    true
}
