//! `exp-desktop` — the Exponential desktop IDE binary (masterplan-v3 §3.1).
//!
//! The §3.6 bootstrap, verbatim. Thin — it wires, it does not implement:
//! `gpui_platform::application()` (the cross-platform backend-selecting
//! entrypoint — NOT `gpui::Application::new()`, which does not exist at the
//! pinned rev) → embedded assets → the `on_open_urls` OAuth-callback channel
//! → `run`: `gpui_component::init` FIRST, forced Exponential Dark theme,
//! panel registry, keymap/menubar, the global `Store`, then the first window
//! inside a foreground `cx.spawn`.

mod actions;
mod assets;
#[cfg(target_os = "macos")]
mod menus;
mod windows;

fn main() {
    let app = gpui_platform::application().with_assets(assets::Assets);

    // on_open_urls is the macOS OAuth-callback surface (exp:// → §5.7).
    // Real signature: FnMut(Vec<String>) — NO cx. Marshal into the App via a
    // channel drained by a foreground task (§3.6).
    let (url_tx, url_rx) = flume::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = url_tx.send(urls);
    });

    // macOS: clicking the dock icon with no windows open reopens one.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            windows::open_workspace_window(cx);
        }
    });

    app.run(move |cx| {
        // MUST be first — installs the component globals + base keys (§3.3).
        gpui_component::init(cx);

        // Fonts before theme: the theme sets font_family = "Inter", which the
        // embedded TTFs make resolvable with no runtime font path (§3.2).
        assets::load_embedded_fonts(cx);

        // ORDER IS LOAD-BEARING (§3.6/§4.3): theme::init forces dark FIRST
        // (Theme::change reassigns colors AND tokens from the stock config),
        // THEN overwrites the palette from the generated tokens and rebuilds
        // tokens. Dark-only, like web — never sync_system_appearance.
        theme::init(cx);

        // Panel-name → constructor registry for DockAreaState rehydration
        // (§3.3), before any window can load a persisted layout.
        ui::init(cx);

        actions::init(cx);
        #[cfg(target_os = "macos")]
        menus::install_menubar(cx);

        // The global Store (§3.6). Phase 2 gives open() its real job
        // (rusqlite/WAL + the 14 shape threads); Phase 1 installs the shared
        // state every window reads — the multi-window gate.
        let store = sync::Store::open(cx);
        cx.set_global(store);

        // Foreground drain for the OAuth-callback URLs (on_open_urls has no
        // cx). Phase 2 routes these into the auth flow (§5.7 exp:// deep
        // links + §4.2 invite links); the drain exists now so the marshalling
        // is proven and Phase 2 only swaps the handler body.
        cx.spawn(async move |_cx| {
            while let Ok(urls) = url_rx.recv_async().await {
                // Stub handler — auth comes in Phase 2.
                eprintln!("[exp-desktop] open-urls (unhandled until Phase 2): {urls:?}");
            }
        })
        .detach();

        cx.activate(true);
        windows::open_workspace_window(cx);

        // Dev hook: EXP_WINDOWS=N opens N workspace windows at startup so the
        // §3.10 multi-window gate ("a second window opens sharing the global
        // Store") is demonstrable headlessly/in CI without synthesizing menu
        // clicks. Users open further windows via File ▸ New Window /
        // cmd-shift-n (the real §3.6 paths).
        if let Some(extra) = std::env::var("EXP_WINDOWS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .map(|count| count.clamp(1, 4) - 1)
        {
            for _ in 0..extra {
                windows::open_workspace_window(cx);
            }
        }
    });
}
