//! `exponential-runner`: the gpui half of the desktop runner. It reads the
//! bundle, boots the one Engine instance (`eengine_runner::runner`) and
//! paints its four surfaces with eui-gpui's `ExponentialHost`, layered
//! `app`, `consent`, `permissions`, `toast`; nothing else is painted. The
//! window's events drive lifecycle.json. No updater, no logger: diagnostics
//! are compiled out of the release build (`log`'s `release_max_level_off`).

#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use eengine::{FormFactor, RunnerFailure};
use eengine_runner::bundle::{find_bundle, read_bundle, Platform};
use eengine_runner::events;
use eengine_runner::paint::{self, Ctx, Icons, Inbound, RunnerView};
use eengine_runner::runner::{self, Providers, Runner};
use eengine_runner::{assets, storage, vault::RunnerVault};
use eui_gpui::runtime::ExponentialHost;
use gpui::{prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use serde_json::Value;

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Macos => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

fn main() {
    let exe = std::env::current_exe().unwrap_or_default();
    let platform = Platform::current();
    let bundle = read_bundle(find_bundle(platform, &exe).as_deref());
    // Linux and Windows: one running instance per app id. A later start
    // hands it its links (or asks it to the front) and exits 0.
    #[cfg(not(target_os = "macos"))]
    let primary = bundle.as_ref().ok().and_then(|b| {
        use eengine_runner::single::{claim, Claim, Endpoint, Message};
        let endpoint = Endpoint::for_app(&b.config.id)?;
        let mut messages: Vec<Message> = events::link_args(&b.config.id, std::env::args()).into_iter().map(Message::Open).collect();
        if messages.is_empty() {
            messages.push(Message::Activate);
        }
        match claim(&endpoint, &messages[0]) {
            Ok(Claim::First(primary)) => Some(primary),
            Ok(Claim::Forwarded) => {
                for message in &messages[1..] {
                    let _ = claim(&endpoint, message);
                }
                std::process::exit(0)
            }
            // Nowhere to listen: this start runs on its own.
            Err(_) => None,
        }
    });
    let (tx, rx) = flume::unbounded::<Inbound>();
    #[cfg(not(target_os = "macos"))]
    let _serving = primary.map(|primary| {
        let tx = tx.clone();
        primary.serve(move |message| {
            let _ = tx.send(match message {
                eengine_runner::single::Message::Open(link) => Inbound::Link(link),
                eengine_runner::single::Message::Activate => Inbound::Activate,
            });
        })
    });
    let application = gpui_platform::application().with_assets(assets::Assets);
    {
        // macOS hands a URL-scheme launch its link as an event.
        let tx = tx.clone();
        application.on_open_urls(move |urls| {
            for url in urls {
                let _ = tx.send(Inbound::Link(url));
            }
        });
    }
    application.run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(assets::fonts());
        let theme = paint::theme_of(bundle.as_ref().ok().map(|b| &b.file));
        let fallback_title = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();

        // The Engine instance, or the failure painted in place of `app`.
        let booted: Result<Runner, (RunnerFailure, String)> = match bundle {
            Err(u) => Err((u.kind, u.config.map(|c| c.name).unwrap_or(fallback_title))),
            Ok(b) => {
                let dir = storage::app_dir(&storage::platform_base().unwrap_or_else(std::env::temp_dir), &b.config.id);
                let _ = std::fs::create_dir_all(&dir);
                match eengine::SqliteRecordStore::open(storage::records_path(&dir)) {
                    Err(_) => Err((RunnerFailure::Invalid, b.config.name.clone())),
                    Ok(store) => {
                        let on_tap: eengine_desktop::OnTap = {
                            let tx = tx.clone();
                            Arc::new(move |tap: Value| {
                                let _ = tx.send(Inbound::Tap(tap));
                            })
                        };
                        let mut desktop = eengine_desktop::DesktopConfig::new(b.config.name.clone(), on_tap);
                        desktop.app_id = Some(b.config.id.clone());
                        let providers = Providers {
                            host_kind: "exported".into(),
                            platform: platform_name(platform).into(),
                            locale: paint::locale(),
                            storage: Arc::new(store),
                            blobs: Arc::new(eengine::FsBlobStore::new(storage::files_dir(&dir))),
                            vault: Arc::new(RunnerVault::platform(&b.config.id, &b.config.name, &dir)),
                            capabilities: eengine_desktop::desktop_capabilities(desktop),
                            identity: None,
                            synced: None,
                        };
                        let name = b.config.name.clone();
                        Runner::boot(b.config, &b.file, providers, FormFactor::Wide).map_err(|kind| (kind, name))
                    }
                }
            }
        };

        // The genUI host: the app's functions and `records:` sources route to the instance.
        let ctx_slot: Rc<RefCell<Option<Rc<Ctx>>>> = Rc::new(RefCell::new(None));
        let plugin = Rc::new(Icons { files: booted.as_ref().ok().map(Runner::app_files) });
        let options = paint::host_options(theme.clone(), plugin, booted.as_ref().ok(), tx.clone(), ctx_slot.clone());
        let host = cx.new(|cx| ExponentialHost::new(options, cx));
        let title = match &booted {
            Ok(r) => r.config().name.clone(),
            Err((_, title)) => title.clone(),
        };
        let app_id = booted.as_ref().ok().map(|r| r.config().id.clone());
        let ctx = Rc::new(Ctx::new(host.clone(), tx.clone()));
        *ctx_slot.borrow_mut() = Some(ctx.clone());

        let store = booted.as_ref().ok().map(|r| {
            let tx = tx.clone();
            r.storage().subscribe(r.app_id(), Box::new(move |_| {
                let _ = tx.send(Inbound::StoreChanged);
            }))
        });
        let first = match booted {
            Ok(mut r) => {
                let paints = r.launch(events::links_in_args(&r.config().id, std::env::args()));
                *ctx.runner.borrow_mut() = Some(r);
                paints
            }
            Err((kind, _)) => runner::failure_paints(kind),
        };
        ctx.apply(first, cx);

        // The pump: every inbound event on the main thread, in order.
        {
            let ctx = ctx.clone();
            cx.spawn(async move |cx| {
                while let Ok(event) = rx.recv_async().await {
                    let _ = cx.update(|cx| ctx.handle(event, cx));
                }
            })
            .detach();
        }
        {
            let ctx = ctx.clone();
            cx.on_app_quit(move |cx| {
                let paints = ctx.runner.borrow_mut().as_mut().map(Runner::terminate).unwrap_or_default();
                ctx.apply(paints, cx);
                async {}
            })
            .detach();
        }
        cx.on_window_closed(|cx, _| cx.quit()).detach();

        let bounds = Bounds::centered(None, size(px(1024.0), px(720.0)), cx);
        let window = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some(title.into()), ..Default::default() }),
            app_id,
            ..Default::default()
        };
        let window = cx.open_window(window, |window, cx| {
            cx.new(|cx| {
                let mut subs = vec![cx.observe(&host, |_, _, cx| cx.notify())];
                let tx = ctx.tx.clone();
                subs.push(cx.observe_window_activation(window, move |_, window, _| {
                    let _ = tx.send(Inbound::Activation(window.is_window_active()));
                }));
                RunnerView::new(ctx.clone(), theme, store, subs)
            })
        })
        .expect("a window");
        ctx.window.set(Some(window.into()));
        cx.activate(true);
    });
}
