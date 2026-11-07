//! `exponential-runner`: the gpui half of the desktop runner. It reads the
//! bundle, boots the one Engine instance (`eengine_runner::runner`) and
//! paints its four surfaces with eui-gpui's `ExponentialHost`, layered
//! `app`, `consent`, `permissions`, `toast`; nothing else is painted. The
//! window's events drive lifecycle.json. No updater, no logger: diagnostics
//! are compiled out of the release build (`log`'s `release_max_level_off`).

#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use eengine::CallFrom;
use eengine::{CallResult, FormFactor, LifecycleEvent, RunnerFailure};
use eengine_runner::bundle::{find_bundle, read_bundle, Platform};
use eengine_runner::events;
use eengine_runner::runner::{self, Dispatch, LiveSurfaces, Paint, Providers, Runner};
use eengine_runner::{assets, storage, vault::RunnerVault};
use eui_core::theme::{Mode, ResolvedTheme};
use eui_gpui::host::HostPlugin;
use eui_gpui::runtime::{Emit, ExponentialHost, HostFunction, HostOptions, SourceResolver};
use gpui::{div, prelude::*, px, size, AnyWindowHandle, App, Bounds, Context, Entity, Hsla, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions};
use serde_json::{json, Map, Value};

type Reply = flume::Sender<Result<Value, String>>;

/// Everything that reaches the main thread, in order.
enum Inbound {
    /// A client message the host sent (the shell's events are `action`s).
    Sent(Value),
    Call { ticket: u64, name: String, args: Value, reply: Reply },
    Navigate { screen: String, params: Map<String, Value> },
    /// A brokered call answered on its worker thread.
    Answered { ticket: u64, name: String, args: Value, result: CallResult },
    Link(String),
    Tap(Value),
    StoreChanged,
    Toast(String),
    Resize(f32),
    Activation(bool),
    /// A later start without a link (Linux, Windows): come to the front.
    Activate,
}

struct Binding {
    id: u64,
    uri: String,
    emit: Emit,
}

/// The main thread's state; no borrow is held while the host runs.
struct Ctx {
    runner: RefCell<Option<Runner>>,
    host: Entity<ExponentialHost>,
    surfaces: RefCell<LiveSurfaces>,
    replies: RefCell<HashMap<u64, Reply>>,
    bindings: RefCell<Vec<Binding>>,
    live: Cell<bool>,
    next: Cell<u64>,
    tx: flume::Sender<Inbound>,
    window: Cell<Option<AnyWindowHandle>>,
}

impl Ctx {
    fn ticket(&self) -> u64 {
        self.next.set(self.next.get() + 1);
        self.next.get()
    }

    fn reply(&self, ticket: u64, result: Result<Value, String>) {
        if let Some(reply) = self.replies.borrow_mut().remove(&ticket) {
            let _ = reply.send(result);
        }
    }

    fn apply(&self, paints: Vec<Paint>, cx: &mut App) {
        for paint in paints {
            match paint {
                Paint::Install(pkg) => {
                    self.host.update(cx, |h, _| h.install_package(&pkg));
                }
                Paint::Message(message) => {
                    let (message, replaced) = self.host.update(cx, |h, _| self.surfaces.borrow_mut().rewrite(&message, |base| h.mint_surface_id(base)));
                    self.host.update(cx, |h, cx| {
                        h.receive_trusted(&message, cx);
                        if let Some(delete) = replaced {
                            h.receive_trusted(&delete, cx);
                        }
                    });
                }
                Paint::Live(live) => {
                    self.live.set(live);
                    if live {
                        self.refresh();
                    }
                }
                Paint::Settle(ticket, result) => self.reply(ticket, result),
            }
        }
    }

    /// The store changed: every bound `records:` source re-resolves.
    fn refresh(&self) {
        let runner = self.runner.borrow();
        let Some(runner) = runner.as_ref() else { return };
        for b in self.bindings.borrow().iter() {
            (b.emit)(runner.records(&b.uri).unwrap_or(Value::Null));
        }
    }

    fn finish(&self, ticket: u64, name: &str, args: &Value, result: CallResult) -> Vec<Paint> {
        let (answer, paints) = match self.runner.borrow_mut().as_mut() {
            Some(r) => r.finish(ticket, name, args, result),
            None => return Vec::new(),
        };
        if let Some(answer) = answer {
            self.reply(ticket, answer);
        }
        paints
    }

    fn handle(&self, event: Inbound, cx: &mut App) {
        let paints = match event {
            Inbound::Sent(message) => {
                let Some(event) = events::runner_event(&message) else { return };
                let (paints, answer) = match self.runner.borrow_mut().as_mut() {
                    Some(r) => r.runner_event(event),
                    None => return,
                };
                if let Some((pending, answer)) = answer {
                    let broker = self.runner.borrow().as_ref().map(|r| r.instance().broker().clone());
                    if let Some(broker) = broker {
                        let tx = self.tx.clone();
                        std::thread::spawn(move || {
                            let result = runner::brokered(broker.answer(CallFrom::Ui, None, &pending.name, &pending.args, &pending.verdict, answer));
                            let _ = tx.send(Inbound::Answered { ticket: pending.ticket, name: pending.name, args: pending.args, result });
                        });
                    }
                }
                paints
            }
            Inbound::Call { ticket, name, args, reply } => {
                self.replies.borrow_mut().insert(ticket, reply);
                let dispatch = match self.runner.borrow_mut().as_mut() {
                    Some(r) => r.dispatch(&name, &args),
                    None => return self.reply(ticket, Err("no app".into())),
                };
                match dispatch {
                    Dispatch::Done(result) => self.finish(ticket, &name, &args, result),
                    Dispatch::Broker(broker) => {
                        // The natives block on their dialogs: never the main thread.
                        let tx = self.tx.clone();
                        std::thread::spawn(move || {
                            let result = runner::brokered(broker.call(CallFrom::Ui, None, &name, &args));
                            let _ = tx.send(Inbound::Answered { ticket, name, args, result });
                        });
                        Vec::new()
                    }
                }
            }
            Inbound::Answered { ticket, name, args, result } => self.finish(ticket, &name, &args, result),
            Inbound::Navigate { screen, params } => {
                let args = json!({"screen": screen, "params": params});
                let result = match self.runner.borrow_mut().as_mut() {
                    Some(r) => match r.dispatch("app_navigate", &args) {
                        Dispatch::Done(result) => result,
                        Dispatch::Broker(_) => return,
                    },
                    None => return,
                };
                self.finish(0, "app_navigate", &args, result)
            }
            Inbound::Link(link) => {
                self.front(cx);
                self.lifecycle(|r| events::deep_link(&r.config().id, &link))
            }
            Inbound::Activate => {
                self.front(cx);
                return;
            }
            Inbound::Tap(tap) => self.lifecycle(|_| events::notification_tap(&tap)),
            Inbound::Activation(active) => self.lifecycle(|_| Some(events::activation(active))),
            Inbound::StoreChanged => {
                if self.live.get() {
                    self.refresh();
                }
                return;
            }
            Inbound::Toast(message) => runner::toast_paints(&message),
            Inbound::Resize(width) => match self.runner.borrow_mut().as_mut() {
                Some(r) => r.resize(width),
                None => return,
            },
        };
        self.apply(paints, cx);
    }

    /// The window to the front (its activation drives `foreground`).
    fn front(&self, cx: &mut App) {
        if let Some(window) = self.window.get() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        cx.activate(true);
    }

    fn lifecycle(&self, event: impl FnOnce(&Runner) -> Option<LifecycleEvent>) -> Vec<Paint> {
        let mut runner = self.runner.borrow_mut();
        let Some(r) = runner.as_mut() else { return Vec::new() };
        match event(r) {
            Some(event) => r.lifecycle(&event),
            None => Vec::new(),
        }
    }
}

/// Icons by concept from the embedded set; an `app-file:` src from the
/// instance's BlobStore (VAPP-271); everything else the painter's.
struct Icons {
    files: Option<Arc<dyn eui_gpui::host::AppFiles>>,
}

impl HostPlugin for Icons {
    fn icon(&self, name: &str) -> Option<SharedString> {
        assets::icon_path(name)
    }

    fn app_files(&self) -> Option<Arc<dyn eui_gpui::host::AppFiles>> {
        self.files.clone()
    }
}

/// #VAPP-271: every runner paints dark, whatever the system appearance (the
/// web, iOS and Android runners do the same): one look on every platform.
const MODE: Mode = Mode::Dark;

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Macos => "macos",
        Platform::Windows => "windows",
        Platform::Linux => "linux",
    }
}

/// `LANG=de_AT.UTF-8` → `de-AT`; the default `en-US`.
fn locale() -> String {
    ["LC_ALL", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(|v| v.split(['.', '@']).next().unwrap_or("").replace('_', "-"))
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .unwrap_or_else(|| "en-US".into())
}

/// The package's theme (a built-in id or a theme JSON), else Exponential's.
fn theme_of(file: Option<&Value>) -> Arc<ResolvedTheme> {
    file.map(eengine::package::package_of)
        .and_then(|pkg| pkg.get("theme"))
        .and_then(|t| eui_gpui::runtime::resolve_surface_theme(t).ok())
        .or_else(|| eui_core::themes::builtin_theme("exponential"))
        .expect("the exponential theme")
}

struct RunnerView {
    ctx: Rc<Ctx>,
    theme: Arc<ResolvedTheme>,
    mode: Mode,
    form: Option<FormFactor>,
    _store: Option<eengine::Subscription>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl Render for RunnerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(window.viewport_size().width);
        let form = events::form_factor(width);
        if self.form != Some(form) {
            self.form = Some(form);
            let _ = self.ctx.tx.send(Inbound::Resize(width));
        }
        let bg: Hsla = eui_gpui::paint::parts::theme_color(Some(&self.theme), self.mode, "background").unwrap_or_else(|| gpui::black());
        let host = self.ctx.host.read(cx);
        let surfaces = self.ctx.surfaces.borrow();
        let app = host.surface(surfaces.live(runner::APP));
        let overlays: Vec<_> = [runner::CONSENT, runner::PERMISSIONS, runner::TOAST].into_iter().filter_map(|id| host.surface(surfaces.live(id))).collect();
        drop(surfaces);
        // runner.json SURFACES: `app` is laid out at the window's height, so
        // the shell fills the window and the page scrolls inside it.
        if let Some(app) = &app {
            let height = f32::from(window.viewport_size().height);
            app.update(cx, |v, cx| {
                v.set_viewport_height(height, cx);
                v.set_fill_height(true, cx);
            });
        }
        div().size_full().relative().bg(bg).child(div().id(runner::APP).size_full().children(app))
            .children(overlays.into_iter().map(|s| div().absolute().top_0().left_0().w_full().child(s)))
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
        let theme = theme_of(bundle.as_ref().ok().map(|b| &b.file));
        let mode = MODE;
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
                            platform: platform_name(platform).into(),
                            locale: locale(),
                            storage: Arc::new(store),
                            blobs: Arc::new(eengine::FsBlobStore::new(storage::files_dir(&dir))),
                            vault: Arc::new(RunnerVault::platform(&b.config.id, &b.config.name, &dir)),
                            capabilities: eengine_desktop::desktop_capabilities(desktop),
                        };
                        let name = b.config.name.clone();
                        Runner::boot(b.config, &b.file, providers, FormFactor::Wide).map_err(|kind| (kind, name))
                    }
                }
            }
        };

        // The genUI host: the app's functions and `records:` sources route to the instance.
        let bindings_next = Rc::new(Cell::new(0u64));
        let ctx_slot: Rc<RefCell<Option<Rc<Ctx>>>> = Rc::new(RefCell::new(None));
        let mut options = HostOptions { theme: Some(theme.clone()), mode, plugin: Rc::new(Icons { files: booted.as_ref().ok().map(Runner::app_files) }), lift_layers: true, ..Default::default() };
        {
            let tx = tx.clone();
            options.on_send = Some(Rc::new(move |message: &Value| {
                if message.get("action").is_some() {
                    let _ = tx.send(Inbound::Sent(message.clone()));
                }
            }));
        }
        {
            let tx = tx.clone();
            options.on_action_failed = Some(Rc::new(move |failure: &eui_gpui::runtime::ActionFailure, _cx: &mut App| {
                let _ = tx.send(Inbound::Toast(failure.message.clone()));
            }));
        }
        {
            let tx = tx.clone();
            options.navigate = Some(Rc::new(move |target: &eui_gpui::runtime::HostNavigate, _cx: &mut App| {
                let _ = tx.send(Inbound::Navigate { screen: target.screen.clone(), params: target.params.clone() });
                true
            }));
        }
        if let Ok(r) = &booted {
            for name in runner::function_names(r.instance().package()) {
                // Round 9 §6.3: genUI calls the flat name; the Engine keeps its own.
                let (tx, slot, key) = (tx.clone(), ctx_slot.clone(), eui_core::upgrade09::rename09(&name));
                let f: HostFunction = Rc::new(move |args: Value, _call, cx: &mut App| {
                    let (reply, answer) = flume::bounded(1);
                    let ticket = slot.borrow().as_ref().map_or(0, |c| c.ticket());
                    let _ = tx.send(Inbound::Call { ticket, name: name.clone(), args, reply });
                    cx.background_executor().spawn(async move { answer.recv_async().await.unwrap_or_else(|_| Err("closed".into())) })
                });
                options.functions.insert(key, f);
            }
            let slot = ctx_slot.clone();
            let next = bindings_next.clone();
            let resolver: SourceResolver = Box::new(move |source, emit: Emit| {
                let Some(ctx) = slot.borrow().clone() else { return Box::new(|| {}) };
                let id = next.get() + 1;
                next.set(id);
                if let Some(value) = ctx.runner.borrow().as_ref().and_then(|r| r.records(&source.uri)) {
                    emit(value);
                }
                ctx.bindings.borrow_mut().push(Binding { id, uri: source.uri.clone(), emit });
                let weak = Rc::downgrade(&ctx);
                Box::new(move || {
                    if let Some(ctx) = weak.upgrade() {
                        ctx.bindings.borrow_mut().retain(|b| b.id != id);
                    }
                })
            });
            options.sources.insert("records".into(), resolver);
        }
        let host = cx.new(|cx| ExponentialHost::new(options, cx));
        let title = match &booted {
            Ok(r) => r.config().name.clone(),
            Err((_, title)) => title.clone(),
        };
        let app_id = booted.as_ref().ok().map(|r| r.config().id.clone());
        let ctx = Rc::new(Ctx {
            runner: RefCell::new(None),
            host: host.clone(),
            surfaces: RefCell::new(LiveSurfaces::default()),
            replies: RefCell::new(HashMap::new()),
            bindings: RefCell::new(Vec::new()),
            live: Cell::new(false),
            next: Cell::new(0),
            tx: tx.clone(),
            window: Cell::new(None),
        });
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
                RunnerView { ctx: ctx.clone(), theme, mode, form: None, _store: store, _subscriptions: subs }
            })
        })
        .expect("a window");
        ctx.window.set(Some(window.into()));
        cx.activate(true);
    });
}
