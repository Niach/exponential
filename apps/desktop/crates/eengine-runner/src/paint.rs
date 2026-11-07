//! The gpui half of a runner, shared by `exponential-runner` (one bundle, one
//! window) and Exponential Desktop's shell (#VAPP-273: every app of the
//! desktop, its window and its widgets): the main-thread context that turns
//! [`Inbound`] events into the runner core's [`Paint`]s, the genUI host whose
//! functions and `records:` sources route into the instance, and the view
//! that layers `app`, `consent`, `permissions` and `toast`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use eengine::CallFrom;
use eengine::{CallResult, FormFactor, LifecycleEvent};
use eui_core::theme::{Mode, ResolvedTheme};
use eui_gpui::host::HostPlugin;
use eui_gpui::runtime::{Emit, ExponentialHost, HostFunction, HostOptions, SourceResolver};
use gpui::{div, prelude::*, AnyWindowHandle, App, Context, Entity, Hsla, SharedString, Window};
use serde_json::{json, Map, Value};

use crate::events;
use crate::runner::{self, Dispatch, LiveSurfaces, Paint, Runner};

pub type Reply = flume::Sender<Result<Value, String>>;

/// Everything that reaches the main thread, in order.
pub enum Inbound {
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
pub struct Ctx {
    pub runner: RefCell<Option<Runner>>,
    pub host: Entity<ExponentialHost>,
    pub surfaces: RefCell<LiveSurfaces>,
    replies: RefCell<HashMap<u64, Reply>>,
    bindings: RefCell<Vec<Binding>>,
    live: Cell<bool>,
    /// The sources stay live whatever the lifecycle says (a desktop widget
    /// shows the records while the window is closed, #VAPP-273).
    pub always_live: Cell<bool>,
    next: Cell<u64>,
    pub tx: flume::Sender<Inbound>,
    pub window: Cell<Option<AnyWindowHandle>>,
}

impl Ctx {
    pub fn new(host: Entity<ExponentialHost>, tx: flume::Sender<Inbound>) -> Self {
        Ctx {
            runner: RefCell::new(None),
            host,
            surfaces: RefCell::new(LiveSurfaces::default()),
            replies: RefCell::new(HashMap::new()),
            bindings: RefCell::new(Vec::new()),
            live: Cell::new(false),
            always_live: Cell::new(false),
            next: Cell::new(0),
            tx,
            window: Cell::new(None),
        }
    }

    pub fn ticket(&self) -> u64 {
        self.next.set(self.next.get() + 1);
        self.next.get()
    }

    fn reply(&self, ticket: u64, result: Result<Value, String>) {
        if let Some(reply) = self.replies.borrow_mut().remove(&ticket) {
            let _ = reply.send(result);
        }
    }

    pub fn apply(&self, paints: Vec<Paint>, cx: &mut App) {
        for paint in paints {
            match paint {
                Paint::Install(pkg) => {
                    self.host.update(cx, |h, _| h.install_package(&pkg));
                }
                Paint::Message(message) => self.receive(&message, cx),
                Paint::Live(live) => {
                    self.live.set(live);
                    if live || self.always_live.get() {
                        self.refresh();
                    }
                }
                Paint::Settle(ticket, result) => self.reply(ticket, result),
            }
        }
    }

    /// One genUI server message on its live surface id.
    pub fn receive(&self, message: &Value, cx: &mut App) {
        let (message, replaced) = self.host.update(cx, |h, _| self.surfaces.borrow_mut().rewrite(message, |base| h.mint_surface_id(base)));
        self.host.update(cx, |h, cx| {
            h.receive_trusted(&message, cx);
            if let Some(delete) = replaced {
                h.receive_trusted(&delete, cx);
            }
        });
    }

    /// The store changed: every bound `records:` source re-resolves.
    pub fn refresh(&self) {
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

    pub fn handle(&self, event: Inbound, cx: &mut App) {
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
                if self.live.get() || self.always_live.get() {
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
    pub fn front(&self, cx: &mut App) {
        if let Some(window) = self.window.get() {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        cx.activate(true);
    }

    pub fn lifecycle(&self, event: impl FnOnce(&Runner) -> Option<LifecycleEvent>) -> Vec<Paint> {
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
pub struct Icons {
    pub files: Option<Arc<dyn eui_gpui::host::AppFiles>>,
}

impl HostPlugin for Icons {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::assets::icon_path(name)
    }

    fn app_files(&self) -> Option<Arc<dyn eui_gpui::host::AppFiles>> {
        self.files.clone()
    }
}

/// #VAPP-271: every runner paints dark, whatever the system appearance (the
/// web, iOS and Android runners do the same): one look on every platform.
pub const MODE: Mode = Mode::Dark;

/// `LANG=de_AT.UTF-8` → `de-AT`; the default `en-US`.
pub fn locale() -> String {
    ["LC_ALL", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(|v| v.split(['.', '@']).next().unwrap_or("").replace('_', "-"))
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .unwrap_or_else(|| "en-US".into())
}

/// The package's theme (a built-in id or a theme JSON), else Exponential's.
pub fn theme_of(file: Option<&Value>) -> Arc<ResolvedTheme> {
    file.map(eengine::package::package_of)
        .and_then(|pkg| pkg.get("theme"))
        .and_then(|t| eui_gpui::runtime::resolve_surface_theme(t).ok())
        .or_else(|| eui_core::themes::builtin_theme("exponential"))
        .expect("the exponential theme")
}

/// The genUI host options of one runner: the app's functions and `records:`
/// sources route to the instance through `tx` and the [`Ctx`] in `ctx_slot`
/// (filled right after the host exists); actions, failed actions and
/// navigation reach the main thread the same way.
pub fn host_options(theme: Arc<ResolvedTheme>, plugin: Rc<dyn HostPlugin>, booted: Option<&Runner>, tx: flume::Sender<Inbound>, ctx_slot: Rc<RefCell<Option<Rc<Ctx>>>>) -> HostOptions {
    let bindings_next = Rc::new(Cell::new(0u64));
    let mut options = HostOptions { theme: Some(theme), mode: MODE, plugin, lift_layers: true, ..Default::default() };
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
    if let Some(r) = booted {
        for name in runner::function_names(r.instance().package()) {
            let (tx, slot, key) = (tx.clone(), ctx_slot.clone(), name.clone());
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
    options
}

/// The window's content: `app` at the window's height, the overlays on top.
pub struct RunnerView {
    pub ctx: Rc<Ctx>,
    pub theme: Arc<ResolvedTheme>,
    pub mode: Mode,
    form: Option<FormFactor>,
    _store: Option<eengine::Subscription>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl RunnerView {
    pub fn new(ctx: Rc<Ctx>, theme: Arc<ResolvedTheme>, store: Option<eengine::Subscription>, subscriptions: Vec<gpui::Subscription>) -> Self {
        RunnerView { ctx, theme, mode: MODE, form: None, _store: store, _subscriptions: subscriptions }
    }
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

/// A widget's surface (#VAPP-273): one template of the package on its own
/// surface id, painted transparent over the desktop at its size class.
pub struct WidgetView {
    pub ctx: Rc<Ctx>,
    pub surface_id: String,
    _subscriptions: Vec<gpui::Subscription>,
}

impl WidgetView {
    pub fn new(ctx: Rc<Ctx>, surface_id: String, subscriptions: Vec<gpui::Subscription>) -> Self {
        WidgetView { ctx, surface_id, _subscriptions: subscriptions }
    }
}

impl Render for WidgetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host = self.ctx.host.read(cx);
        let surface = host.surface(self.ctx.surfaces.borrow().live(&self.surface_id));
        if let Some(surface) = &surface {
            let height = f32::from(window.viewport_size().height);
            surface.update(cx, |v, cx| {
                v.set_viewport_height(height, cx);
                v.set_fill_height(true, cx);
            });
        }
        div().size_full().children(surface)
    }
}
