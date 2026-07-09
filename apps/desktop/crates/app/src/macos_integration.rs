//! macOS: assert THIS bundle as the default `exponential://` handler at
//! startup — the macOS analog of the Linux [`crate::desktop_integration`]
//! (§5.7).
//!
//! Unlike Linux, macOS needs no in-app file writing to REGISTER the handler:
//! Launch Services auto-registers a launched `.app` from its Info.plist
//! `CFBundleURLTypes` (`assets/packaging/Info.plist` declares `exponential`).
//! What macOS has no automatic equivalent for is Linux's "re-assert myself as
//! the *default* each launch" (the `mimeapps.list` default write). This does
//! that one thing: `LSSetDefaultHandlerForURLScheme(exponential, <our bundle
//! id>)`, so whichever channel (prod vs staging) was launched most recently
//! wins the callback — mirroring the Linux behaviour and surviving a stray
//! reappearance of another `exponential:` claimant.
//!
//! No-op unless we are actually running from a bundle whose id we can read: a
//! bare `cargo run` binary has no Info.plist identifier, and registering an
//! id that isn't on disk would break the callback. Best-effort throughout —
//! a failure just means the user may need to pick the handler once; it must
//! never block startup.

#![cfg(target_os = "macos")]

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};

/// The deep-link scheme (EXP-41 single source) — must match the
/// `CFBundleURLSchemes` entry in the packaged Info.plist.
const SCHEME: &str = api::login::OAUTH_CALLBACK_SCHEME;

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    /// Deprecated-but-functional LaunchServices call (still used by Zed et al.).
    /// Silent for private app schemes like `exponential://` (only the protected
    /// http/https/mailto handlers prompt). Returns an `OSStatus` (0 == ok).
    fn LSSetDefaultHandlerForURLScheme(scheme: CFStringRef, bundle_id: CFStringRef) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFBundleGetMainBundle() -> *mut c_void;
    fn CFBundleGetIdentifier(bundle: *mut c_void) -> CFStringRef;
}

/// Assert this running bundle as the default `exponential://` handler. Safe
/// to call unconditionally: it returns early when unbundled (dev `cargo run`).
pub fn ensure_scheme_registered() {
    let Some(bundle_id) = main_bundle_identifier() else {
        // Unbundled (dev `cargo run`): there is nothing to register. Build and
        // launch the `.app` (`bun run run:desktop:mac`) for exponential://
        // routing.
        return;
    };

    let scheme = CFString::new(SCHEME);
    let id = CFString::new(&bundle_id);
    let status = unsafe {
        LSSetDefaultHandlerForURLScheme(scheme.as_concrete_TypeRef(), id.as_concrete_TypeRef())
    };
    if status != 0 {
        eprintln!(
            "[exp-desktop] {SCHEME}:// default-handler assertion returned OSStatus {status}"
        );
    }
}

/// The running process's bundle identifier, or `None` when not launched from a
/// `.app` (a bare executable has a main bundle but no `CFBundleIdentifier`).
fn main_bundle_identifier() -> Option<String> {
    unsafe {
        let bundle = CFBundleGetMainBundle();
        if bundle.is_null() {
            return None;
        }
        let id_ref = CFBundleGetIdentifier(bundle);
        if id_ref.is_null() {
            return None;
        }
        // `CFBundleGetIdentifier` follows the Get Rule (we do not own the ref),
        // so wrap under the get rule (retain-on-wrap / release-on-drop).
        Some(CFString::wrap_under_get_rule(id_ref).to_string())
    }
}
