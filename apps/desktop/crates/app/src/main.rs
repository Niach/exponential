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
mod channel;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod desktop_integration;
#[cfg(target_os = "macos")]
mod macos_integration;
#[cfg(target_os = "macos")]
mod menus;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod single_instance;
mod windows;

fn main() {
    // OAuth-callback channel (exp:// → §5.7): filled by the macOS
    // `on_open_urls` surface AND — on Linux, where gpui never invokes that —
    // by the single-instance datagram bridge. Drained by a foreground task
    // (§3.6) into `ui::handle_open_urls`.
    let (url_tx, url_rx) = flume::unbounded::<Vec<String>>();

    // Linux/BSD: enforce a single instance and route the browser's exp://
    // deep link into the RUNNING window (gpui's on_open_urls is macOS-only).
    // A forwarding launch exits here BEFORE we spin up any display/GPU state;
    // the primary registers itself as the exp:// handler so the callback can
    // reach it at all (AppImage/dev builds register nothing otherwise).
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    match single_instance::acquire(url_tx.clone()) {
        single_instance::Instance::Forwarded => return,
        single_instance::Instance::Primary => desktop_integration::ensure_scheme_registered(),
    }

    // macOS: re-assert ourselves as the default exp:// handler each launch
    // (self-registration parity with Linux). No-op when run unbundled. macOS
    // needs no single-instance socket — Launch Services delivers exp:// to the
    // already-running bundle via on_open_urls below.
    #[cfg(target_os = "macos")]
    macos_integration::ensure_scheme_registered();

    let app = gpui_platform::application().with_assets(assets::Assets);

    // on_open_urls is the macOS OAuth-callback surface (exp:// → §5.7).
    // Real signature: FnMut(Vec<String>) — NO cx.
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

        // ---- Auth + sync globals (§5.7 / §5.8) -----------------------------
        // The AuthStore hydrates remembered accounts + tokens (0600-file
        // store); its unauthorized handler is wired into the sync manager so a
        // hard 401 deletes the dead token BEFORE the store's delta drain
        // routes the UI to login (EXP-1 #13b).
        let data_dir = api::default_data_dir();
        let auth = api::AuthStore::load(data_dir.clone());
        let store = sync::Store::open(cx, Some(auth.unauthorized_handler_fn()));
        cx.set_global(store);
        cx.set_global(ui::AuthContext {
            auth,
            client: std::sync::Arc::new(api::AuthClient::new()),
            data_dir,
        });

        // Remote-steer subsystem (§08): the single steer tokio runtime, the
        // own-row Electric kill-switch, and the remote-`start_session` inbox.
        // MUST run before the session bootstrap connects an account (which
        // dials the per-account control socket). A no-op-friendly install:
        // when the relay is unconfigured the whole subsystem stays silent.
        ui::steer_wiring::install(cx);

        // Session bootstrap: the EXP_DEV_SERVER/EXP_DEV_TOKEN dev override
        // (headless verification, dev-only) or a warm-start resume of the
        // persisted account — else the workspace boots to the login surface.
        ui::bootstrap_session(cx);

        // Foreground drain for the OAuth-callback URLs (on_open_urls has no
        // cx): `exp://oauth-return#token=…` → the §5.7 token adoption in
        // `ui::handle_open_urls` (parse locally, validate, sign in, sync).
        cx.spawn(async move |cx| {
            while let Ok(urls) = url_rx.recv_async().await {
                cx.update(|cx| ui::handle_open_urls(urls, cx));
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
