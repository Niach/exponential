//! EXP-1249 — THE native file prompt. Every IDE file picker (the composer's
//! "+", the steer reply's attach, the description editors, the timeline and
//! the issue Files rails) goes through [`prompt_for_paths`], never
//! `cx.prompt_for_paths` directly.
//!
//! Why (the Linux "+" crash): gpui_linux runs the xdg-desktop-portal
//! FileChooser through ashpd → zbus, spawned on gpui's FOREGROUND executor.
//! Since EXP-1196 the IDE links cua's `platform-linux`, which turns on zbus'
//! `tokio` feature; cargo unifies it, so zbus connects its D-Bus socket with
//! `tokio::net::UnixStream::from_std` and spawns through
//! `tokio::task::spawn_blocking` — both panic ("there is no reactor running,
//! must be called from the context of a Tokio 1.x runtime") on gpui's main
//! thread, which has no runtime. The portal future is POLLED on that thread
//! long after the call returns, so wrapping the call in a scoped `enter()`
//! would not cover it: the main thread enters the steer runtime's context
//! ONCE, for the life of the process, before the first prompt.

use gpui::{App, PathPromptOptions};

/// The `cx.prompt_for_paths` every IDE file picker calls. On Linux the main
/// thread first enters the steer Tokio runtime ([`enter_tokio_context`]);
/// everywhere else it is the plain gpui call.
pub(crate) fn prompt_for_paths(
    cx: &mut App,
    options: PathPromptOptions,
) -> impl std::future::Future<
    Output = Result<anyhow::Result<Option<Vec<std::path::PathBuf>>>, impl std::fmt::Debug>,
> + 'static {
    if needs_tokio_context() {
        enter_tokio_context(cx);
    }
    cx.prompt_for_paths(options)
}

/// Whether this platform's file portal rides zbus (and so tokio): Linux and
/// the BSDs gpui_linux serves. macOS and Windows prompt natively.
pub(crate) const fn needs_tokio_context() -> bool {
    cfg!(any(target_os = "linux", target_os = "freebsd"))
}

thread_local! {
    /// Whether THIS thread already holds the process-long runtime context.
    static ENTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Enter the steer runtime's context on the calling (main) thread, once.
///
/// The guard is deliberately leaked: Tokio keeps the handle in a
/// thread-local, so every later poll of the portal future on this thread
/// finds a reactor. Nothing else on the main thread holds an `enter()` guard
/// across a click handler, so the leaked guard sits at the bottom of the
/// thread's context stack and every scoped `enter()` above it still nests.
/// `Handle::enter` sets the CURRENT handle only (not "inside a runtime"), so
/// a later `block_on` on this thread is unaffected. A build whose steer
/// runtime failed to start has nothing to enter: the prompt then behaves as
/// before.
fn enter_tokio_context(cx: &App) {
    if ENTERED.with(|entered| entered.get()) {
        return;
    }
    let Some(runtime) = crate::steer_wiring::runtime(cx) else {
        log::warn!("file picker: no steer runtime to enter; the portal may refuse");
        return;
    };
    std::mem::forget(runtime.handle().enter());
    ENTERED.with(|entered| entered.set(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context is entered only where the portal rides zbus.
    #[test]
    fn only_portal_platforms_enter_the_tokio_context() {
        assert_eq!(
            needs_tokio_context(),
            cfg!(any(target_os = "linux", target_os = "freebsd"))
        );
        #[cfg(target_os = "macos")]
        assert!(!needs_tokio_context());
    }
}
