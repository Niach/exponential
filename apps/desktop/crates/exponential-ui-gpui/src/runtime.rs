//! VAPP-91: the gpui host runtime, the Rust mirror of the TS reference
//! (`packages/exponential-ui/src/host/runtime.ts`). An [`ExponentialHost`]
//! entity owns the transport, the core's [`HostRouter`], ONE
//! [`SurfaceView`] entity per surface, the source subscriptions, the
//! function registry and the policy. The router's ops are performed here;
//! the painter's interactions come back through [`host_plugin`] (actions →
//! A2UI client messages, function calls → the policy gate, urls → the URL
//! policy, media → the media rules).
//!
//! Everything a transport or a source emits from another thread lands on
//! the gpui main thread through one channel (the host's pump task).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use exponential_ui::host::{
    action_message, client_capabilities, combine_decisions, decide_function, decide_url, error_message, media_request, package_policy, parse_source,
    FunctionDecision, FunctionPolicy, HostRouter, MediaOptions, MediaRequest, PackageIssue, ParsedSource, UrlPolicy, FUNCTION_DENIED, FUNCTION_NOT_FOUND,
    RENDER_FAILED, UNSUPPORTED_CATALOG, VALIDATION_FAILED,
};
use exponential_ui::measure::TextStyle;
use exponential_ui::surface::PlacedNode;
use exponential_ui::theme::{try_load_theme, Mode, ResolvedTheme, ThemeOptions};
use exponential_ui::themes::{builtin_refs, builtin_theme, default_theme, BUILTIN_THEME_IDS};
use exponential_ui::{ExtensionDef, FlatComponent};
use gpui::{App, AppContext as _, Context, Entity, Task, WeakEntity};
use serde_json::Value;

use crate::extension::ExtensionPainter;
use crate::host::{ActionEvent, FunctionCallEvent, HostPlugin, InputEvent, NoHost, PaintError};
use crate::transport::{Transport, TransportEvent, TransportSink, TransportStatus};
use crate::view::{SurfaceView, SurfaceViewOptions};

/// A host function call (the painter's [`FunctionCallEvent`]).
pub type FunctionCallInfo = FunctionCallEvent;

/// A host function: `(args, call, cx) → Task<Result<value, message>>`. A
/// synchronous one returns `Task::ready(..)` ([`sync_function`]); an async
/// one `cx.spawn(..)`s.
pub type HostFunction = Rc<dyn Fn(Value, &FunctionCallInfo, &mut App) -> Task<Result<Value, String>>>;

/// A synchronous [`HostFunction`].
pub fn sync_function(f: impl Fn(Value, &FunctionCallInfo, &mut App) -> Result<Value, String> + 'static) -> HostFunction {
    Rc::new(move |args, call, cx| Task::ready(f(args, call, cx)))
}

/// What a source resolver emits: a new value for the bound path. Callable
/// from any thread; emits after the binding is cancelled are dropped.
pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;
/// Stops a source subscription.
pub type Cancel = Box<dyn FnOnce()>;
/// One per source scheme (`exp:issues?board=…` → `exp`).
pub type SourceResolver = Box<dyn Fn(ParsedSource, Emit) -> Cancel>;
/// The consent hook for `ask` decisions: resolves `true` to run the call.
pub type ConsentHook = Rc<dyn Fn(&FunctionCallInfo, &mut App) -> Task<bool>>;
/// Opens an allowed (absolute) url.
pub type OpenUrlHandler = Rc<dyn Fn(&str, &mut App)>;
/// Observes a JSON value the host sends or an op it performs (`on_send`, `on_op`).
pub type ValueObserver = Rc<dyn Fn(&Value)>;
/// Observes every problem the host meets (`on_issue`).
pub type IssueObserver = Rc<dyn Fn(&HostIssue)>;

/// One problem the host met; [`ExponentialHost::issues`] keeps the latest
/// [`MAX_ISSUES`] (the TS `HostIssue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostIssue {
    /// A host error code (`TEMPLATE_NOT_FOUND`…) or [`PACKAGE_INVALID`].
    pub code: String,
    pub message: String,
    pub surface_id: Option<String>,
    /// A JSON pointer (into the package or the message).
    pub path: Option<String>,
    pub package_id: Option<String>,
}

/// The issue code of a package that failed validation.
pub const PACKAGE_INVALID: &str = "PACKAGE_INVALID";
/// How many issues [`ExponentialHost::issues`] keeps (oldest dropped).
pub const MAX_ISSUES: usize = 100;

/// A package passed in [`HostOptions::packages`] that failed validation
/// (the TS `PackageError`): [`HostOptions::validate_packages`] returns it;
/// [`ExponentialHost::new`] reports it as issues instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageError {
    pub package_id: String,
    pub issues: Vec<PackageIssue>,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let issues: Vec<String> = self.issues.iter().map(|i| format!("{} {}", if i.path.is_empty() { "/" } else { i.path.as_str() }, i.message)).collect();
        write!(f, "exponential-ui: package {} is unusable: {}", self.package_id, issues.join("; "))
    }
}

impl std::error::Error for PackageError {}

fn package_id(pkg: &Value) -> Option<String> {
    pkg.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()).map(str::to_string)
}

/// A `createSurface.theme` (a built-in id or a theme JSON) → the resolved
/// theme, or why it is unusable (the TS `resolveSurfaceTheme`).
pub fn resolve_surface_theme(input: &Value) -> Result<Arc<ResolvedTheme>, Vec<String>> {
    if let Value::String(id) = input {
        return builtin_theme(id).ok_or_else(|| vec![format!("unknown built-in theme \"{id}\"; known: {}", BUILTIN_THEME_IDS.join("|"))]);
    }
    let refs = builtin_refs();
    try_load_theme(input, &ThemeOptions::core(&refs))
        .map(Arc::new)
        .map_err(|issues| issues.into_iter().map(|i| if i.path.is_empty() { i.message } else { format!("{}: {}", i.path, i.message) }).collect())
}

/// The host's policy (`catalog/host.json` functions / urls / media).
#[derive(Clone, Default)]
pub struct HostPolicy {
    /// The declarative gate; `ask` decisions go to `on_function_call`.
    pub functions: Option<FunctionPolicy>,
    /// The consent hook. Unset = `ask` is denied.
    pub on_function_call: Option<ConsentHook>,
    pub urls: Option<UrlPolicy>,
    /// Opens an allowed url (default: `cx.open_url`).
    pub open_url: Option<OpenUrlHandler>,
    pub media: Option<MediaOptions>,
}

/// How an [`ExponentialHost`] is created.
pub struct HostOptions {
    pub transport: Option<Box<dyn Transport>>,
    pub functions: HashMap<String, HostFunction>,
    pub sources: HashMap<String, SourceResolver>,
    /// Extension catalogs (`exponential_ui::extension::parse_extension`).
    pub extensions: Vec<ExtensionDef>,
    /// Their native painters, keyed on the extension kind.
    pub painters: Vec<(String, Rc<dyn ExtensionPainter>)>,
    /// Declarative vapp packages to install.
    pub packages: Vec<Value>,
    pub policy: HostPolicy,
    /// Every surface's theme (`None` = geometry mode) and mode.
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    /// Round 2 §3: every surface's number / date formatter (`None` = the
    /// core's English one).
    pub formatter: Option<Arc<dyn exponential_ui::format::Formatter>>,
    /// The rest of the painter callbacks (icons, inputs, markdown, fonts…);
    /// actions, function calls, urls and media route through the host.
    pub plugin: Rc<dyn HostPlugin>,
    /// Every client message that leaves (after the transport got it).
    pub on_send: Option<ValueObserver>,
    /// Every op the host performs (tests, logging).
    pub on_op: Option<ValueObserver>,
    /// Every problem the host meets (a package that failed validation, an
    /// error it answered a message with, an invalid surface theme). A
    /// transport-less host has no server to tell: this is where they land.
    pub on_issue: Option<IssueObserver>,
}

impl HostOptions {
    /// Validate [`HostOptions::packages`] against these options' catalogs
    /// (what [`ExponentialHost::new`] would report and skip).
    pub fn validate_packages(&self) -> Result<(), PackageError> {
        let ids: Vec<&str> = self.extensions.iter().map(|e| e.id.as_str()).collect();
        let mut router = HostRouter::new(&ids);
        for pkg in &self.packages {
            let issues = router.install_package(pkg);
            if !issues.is_empty() {
                return Err(PackageError { package_id: package_id(pkg).unwrap_or_else(|| "?".into()), issues });
            }
        }
        Ok(())
    }
}

impl Default for HostOptions {
    fn default() -> Self {
        HostOptions {
            transport: None,
            functions: HashMap::new(),
            sources: HashMap::new(),
            extensions: Vec::new(),
            painters: Vec::new(),
            packages: Vec::new(),
            policy: HostPolicy::default(),
            theme: Some(default_theme()),
            mode: Mode::Dark,
            formatter: None,
            plugin: Rc::new(NoHost),
            on_send: None,
            on_op: None,
            on_issue: None,
        }
    }
}

/// What [`ExponentialHost::call_function`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionOutcome {
    pub decision: FunctionDecision,
    pub result: Option<Value>,
    pub error: Option<String>,
}

impl FunctionOutcome {
    fn of(decision: FunctionDecision) -> Self {
        FunctionOutcome { decision, result: None, error: None }
    }
}

/// Events the pump lands on the main thread.
enum Inbound {
    Transport { generation: u64, event: TransportEvent },
    Source { surface_id: String, alive: Arc<AtomicBool>, path: String, value: Value },
}

struct SurfaceEntry {
    id: String,
    view: Entity<SurfaceView>,
    catalog_id: String,
    package_id: Option<String>,
    send_data_model: bool,
    /// The server's `createSurface.theme`, resolved (wins over the host's
    /// theme, [`ExponentialHost::set_theme`] included).
    theme: Option<Arc<ResolvedTheme>>,
}

/// The host runtime entity. Create it with `cx.new(|cx|
/// ExponentialHost::new(options, cx))`, `connect`, then paint
/// `host.read(cx).surface(id)` (a [`SurfaceView`]) wherever you like.
pub struct ExponentialHost {
    router: HostRouter,
    surfaces: Vec<SurfaceEntry>,
    subscriptions: HashMap<String, Vec<(Arc<AtomicBool>, Cancel)>>,
    transport: Option<Box<dyn Transport>>,
    functions: HashMap<String, HostFunction>,
    sources: HashMap<String, SourceResolver>,
    extensions: Vec<ExtensionDef>,
    painters: Vec<(String, Rc<dyn ExtensionPainter>)>,
    policy: HostPolicy,
    /// Shared with every view's plugin adapter (it needs no `cx`).
    media: Rc<RefCell<MediaOptions>>,
    /// The effective URL policy (the media `baseUrl` when it names none),
    /// shared like `media`.
    urls: Rc<RefCell<UrlPolicy>>,
    /// `onPaintError` dedupe: surface \0 component \0 message.
    paint_errors: std::collections::HashSet<String>,
    theme: Option<Arc<ResolvedTheme>>,
    mode: Mode,
    formatter: Option<Arc<dyn exponential_ui::format::Formatter>>,
    base: Rc<dyn HostPlugin>,
    on_send: Option<ValueObserver>,
    on_op: Option<ValueObserver>,
    on_issue: Option<IssueObserver>,
    issues: Vec<HostIssue>,
    status: TransportStatus,
    status_detail: Option<String>,
    unsupported_catalog: Option<String>,
    generation: u64,
    tx: flume::Sender<Inbound>,
    this: WeakEntity<ExponentialHost>,
    _pump: Task<()>,
}

impl ExponentialHost {
    /// A package in [`HostOptions::packages`] that fails validation is not
    /// installed: it is reported as PACKAGE_INVALID issues (`on_issue`,
    /// [`Self::issues`]), never a panic; [`HostOptions::validate_packages`]
    /// turns it into a [`PackageError`] before the host exists.
    pub fn new(options: HostOptions, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = flume::unbounded::<Inbound>();
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv_async().await {
                if this.update(cx, |host, cx| host.inbound(event, cx)).is_err() {
                    break;
                }
            }
        });
        let ids: Vec<String> = options.extensions.iter().map(|e| e.id.clone()).collect();
        let mut host = ExponentialHost {
            router: HostRouter::new(&ids),
            surfaces: Vec::new(),
            subscriptions: HashMap::new(),
            transport: options.transport,
            functions: options.functions,
            sources: options.sources.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect(),
            extensions: options.extensions,
            painters: options.painters,
            media: Rc::new(RefCell::new(options.policy.media.clone().unwrap_or_default())),
            urls: Rc::new(RefCell::new(effective_urls(&options.policy))),
            paint_errors: Default::default(),
            policy: options.policy,
            theme: options.theme,
            mode: options.mode,
            formatter: options.formatter,
            base: options.plugin,
            on_send: options.on_send,
            on_op: options.on_op,
            on_issue: options.on_issue,
            issues: Vec::new(),
            status: TransportStatus::Closed,
            status_detail: None,
            unsupported_catalog: None,
            generation: 0,
            tx,
            this: cx.entity().downgrade(),
            _pump: pump,
        };
        // VAPP-103: never a panic. A package that fails validation is not
        // installed and is reported (PACKAGE_INVALID issues, `on_issue`);
        // `HostOptions::validate_packages` is the hard check.
        for pkg in &options.packages {
            host.install_package(pkg);
        }
        host
    }

    // --- state ---------------------------------------------------------------

    pub fn router(&self) -> &HostRouter {
        &self.router
    }

    /// False for a local-only host (packages, direct `receive`): no
    /// `host_offline` state to show.
    pub fn has_transport(&self) -> bool {
        self.transport.is_some()
    }

    pub fn status(&self) -> TransportStatus {
        self.status
    }

    pub fn status_detail(&self) -> Option<&str> {
        self.status_detail.as_deref()
    }

    /// The last `UNSUPPORTED_CATALOG` id (the catalog-update banner).
    pub fn unsupported_catalog(&self) -> Option<&str> {
        self.unsupported_catalog.as_deref()
    }

    pub fn supported_catalog_ids(&self) -> Vec<String> {
        self.router.supported_catalog_ids()
    }

    pub fn client_capabilities(&self) -> Value {
        let ids: Vec<&str> = self.extensions.iter().map(|e| e.id.as_str()).collect();
        client_capabilities(&ids)
    }

    pub fn extensions(&self) -> &[ExtensionDef] {
        &self.extensions
    }

    pub fn policy(&self) -> &HostPolicy {
        &self.policy
    }

    /// The latest problems, oldest first (at most [`MAX_ISSUES`]).
    pub fn issues(&self) -> &[HostIssue] {
        &self.issues
    }

    fn report(&mut self, issue: HostIssue) {
        if let Some(f) = &self.on_issue {
            f(&issue);
        }
        self.issues.push(issue);
        if self.issues.len() > MAX_ISSUES {
            let over = self.issues.len() - MAX_ISSUES;
            self.issues.drain(..over);
        }
    }

    // --- registration ----------------------------------------------------------

    /// A new extension catalog (surfaces created after this see it).
    pub fn register_extension(&mut self, ext: ExtensionDef) {
        if self.extensions.iter().any(|e| e.id == ext.id) {
            return;
        }
        self.router.register_extension(&ext.id);
        self.extensions.push(ext);
    }

    /// The native painter of an extension kind, on every surface.
    pub fn register_painter(&mut self, kind: impl Into<String>, painter: Rc<dyn ExtensionPainter>, cx: &mut Context<Self>) {
        let kind = kind.into();
        for s in &self.surfaces {
            let (k, p) = (kind.clone(), painter.clone());
            s.view.update(cx, |v, cx| {
                v.register_painter_rc(k, p);
                cx.notify();
            });
        }
        self.painters.retain(|(k, _)| *k != kind);
        self.painters.push((kind, painter));
    }

    pub fn register_function(&mut self, name: impl Into<String>, f: HostFunction) {
        self.functions.insert(name.into(), f);
    }

    pub fn register_source(&mut self, scheme: &str, resolver: SourceResolver) {
        self.sources.insert(scheme.to_lowercase(), resolver);
    }

    /// Install a package; its validation issues are returned AND reported
    /// ([`Self::issues`], `on_issue`, code [`PACKAGE_INVALID`]). A package
    /// with issues is not installed.
    pub fn install_package(&mut self, pkg: &Value) -> Vec<PackageIssue> {
        let issues = self.router.install_package(pkg);
        let package_id = pkg.get("id").and_then(Value::as_str).map(str::to_string);
        for i in &issues {
            self.report(HostIssue { code: PACKAGE_INVALID.into(), message: i.message.clone(), surface_id: None, path: Some(i.path.clone()), package_id: package_id.clone() });
        }
        issues
    }

    pub fn set_policy(&mut self, policy: HostPolicy) {
        *self.media.borrow_mut() = policy.media.clone().unwrap_or_default();
        *self.urls.borrow_mut() = effective_urls(&policy);
        self.policy = policy;
    }

    /// Every surface's theme.
    pub fn set_theme(&mut self, theme: Option<Arc<ResolvedTheme>>, cx: &mut Context<Self>) {
        self.theme = theme.clone();
        for s in self.surfaces.iter().filter(|s| s.theme.is_none()) {
            let t = theme.clone();
            s.view.update(cx, |v, cx| v.set_theme(t, cx));
        }
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        for s in &self.surfaces {
            s.view.update(cx, |v, cx| v.set_mode(mode, cx));
        }
        cx.notify();
    }

    // --- surfaces --------------------------------------------------------------

    pub fn surface(&self, id: &str) -> Option<Entity<SurfaceView>> {
        self.entry(id).map(|s| s.view.clone())
    }

    /// In creation order.
    pub fn surface_ids(&self) -> Vec<String> {
        self.surfaces.iter().map(|s| s.id.clone()).collect()
    }

    pub fn catalog_id_of(&self, id: &str) -> Option<&str> {
        self.entry(id).map(|s| s.catalog_id.as_str())
    }

    /// The package whose template created the surface (its function policy).
    pub fn package_id_of(&self, id: &str) -> Option<&str> {
        self.entry(id).and_then(|s| s.package_id.as_deref())
    }

    pub fn sends_data_model(&self, id: &str) -> bool {
        self.entry(id).is_some_and(|s| s.send_data_model)
    }

    /// The surface's own theme (its `createSurface.theme`), if any.
    pub fn surface_theme(&self, id: &str) -> Option<Arc<ResolvedTheme>> {
        self.entry(id).and_then(|s| s.theme.clone())
    }

    fn entry(&self, id: &str) -> Option<&SurfaceEntry> {
        self.surfaces.iter().find(|s| s.id == id)
    }

    // --- transport -------------------------------------------------------------

    fn sink(&self) -> TransportSink {
        let tx = self.tx.clone();
        let generation = self.generation;
        TransportSink::new(move |event| {
            let _ = tx.send(Inbound::Transport { generation, event });
        })
    }

    /// Start the transport (no-op without one).
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let sink = self.sink();
        if let Some(t) = self.transport.as_mut() {
            t.start(sink);
            cx.notify();
        }
    }

    /// Close the transport and drop every surface.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        if let Some(t) = self.transport.as_mut() {
            t.close();
        }
        for id in self.surface_ids() {
            self.perform(&serde_json::json!({"op": "delete", "surfaceId": id}), cx);
        }
        self.status = TransportStatus::Closed;
        cx.notify();
    }

    fn inbound(&mut self, event: Inbound, cx: &mut Context<Self>) {
        match event {
            Inbound::Transport { generation, event } => {
                if generation != self.generation {
                    return;
                }
                match event {
                    TransportEvent::Message(m) => {
                        self.receive(&m, cx);
                    }
                    TransportEvent::Status(status, detail) => {
                        self.status = status;
                        self.status_detail = detail;
                        cx.notify();
                    }
                }
            }
            Inbound::Source { surface_id, alive, path, value } => {
                if !alive.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(view) = self.surface(&surface_id) {
                    if let Err(e) = view.update(cx, |v, cx| v.set_data(&path, Some(value), cx)) {
                        let at = if path.is_empty() { "/" } else { path.as_str() };
                        self.send(error_message(VALIDATION_FAILED, &surface_id, &e, Some(at)));
                    }
                }
            }
        }
    }

    /// One server message (a transport delivers here; tests and local hosts
    /// may call it directly). Returns the ops performed.
    pub fn receive(&mut self, message: &Value, cx: &mut Context<Self>) -> Vec<Value> {
        let ops = self.router.route(message);
        for op in &ops {
            self.perform(op, cx);
        }
        ops
    }

    /// A client message to the server. An `error` message is also
    /// reported ([`Self::issues`]), transport or not.
    pub fn send(&mut self, message: Value) {
        if let Some(t) = self.transport.as_mut() {
            t.send(&message);
        }
        if let Some(f) = &self.on_send {
            f(&message);
        }
        if let Some(e) = message.get("error") {
            let text = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
            self.report(HostIssue { code: text("code").unwrap_or_default(), message: text("message").unwrap_or_default(), surface_id: text("surfaceId").filter(|s| !s.is_empty()), path: text("path"), package_id: None });
        }
    }

    fn perform(&mut self, op: &Value, cx: &mut Context<Self>) {
        if let Some(f) = &self.on_op {
            f(op);
        }
        let sid = op["surfaceId"].as_str().unwrap_or("").to_string();
        match op["op"].as_str().unwrap_or("") {
            "create" => {
                self.unbind(&sid);
                self.forget_paint_errors(&sid);
                self.surfaces.retain(|s| s.id != sid);
                let catalog_id = op["catalogId"].as_str().unwrap_or("").to_string();
                let surface_theme = match op.get("theme").filter(|t| !t.is_null()).map(resolve_surface_theme) {
                    Some(Ok(theme)) => Some(theme),
                    Some(Err(issues)) => {
                        let msg = error_message(VALIDATION_FAILED, &sid, &format!("createSurface.theme is unusable: {}", issues.join("; ")), Some("/createSurface/theme"));
                        self.send(msg);
                        None
                    }
                    None => None,
                };
                let plugin = host_plugin_with(self.this.clone(), self.base.clone(), self.media.clone(), self.urls.clone());
                let options = SurfaceViewOptions {
                    surface_id: sid.clone(),
                    catalog_id: catalog_id.clone(),
                    theme: surface_theme.clone().or_else(|| self.theme.clone()),
                    mode: self.mode,
                    extensions: self.extensions.clone(),
                    host: plugin,
                    rounding: false,
                    formatter: self.formatter.clone(),
                    ..Default::default()
                };
                let painters = self.painters.clone();
                let view = cx.new(|cx| {
                    let mut v = SurfaceView::without_window(options, cx);
                    for (k, p) in painters {
                        v.register_painter_rc(k, p);
                    }
                    v
                });
                let package_id = self.router.package_id_of(&sid);
                self.surfaces.push(SurfaceEntry { id: sid, view, catalog_id, package_id, send_data_model: op["sendDataModel"] == Value::Bool(true), theme: surface_theme });
                cx.notify();
            }
            "components" => {
                self.forget_paint_errors(&sid);
                let Some(entry) = self.surfaces.iter().find(|s| s.id == sid) else { return };
                let incoming: Vec<FlatComponent> = match serde_json::from_value(op["components"].clone()) {
                    Ok(c) => c,
                    Err(e) => {
                        let msg = error_message(VALIDATION_FAILED, &sid, &format!("components: {e}"), None);
                        self.send(msg);
                        return;
                    }
                };
                // By id, reduced lazily (VAPP-103: a streamed surface costs linear).
                entry.view.update(cx, |v, cx| {
                    v.update_components(incoming, cx);
                });
            }
            "data" => {
                if let Some(view) = self.surface(&sid) {
                    let path = op["path"].as_str().unwrap_or("").to_string();
                    let value = op.get("value").cloned();
                    if let Err(e) = view.update(cx, |v, cx| v.set_data(&path, value, cx)) {
                        let at = if path.is_empty() { "/" } else { path.as_str() };
                        self.send(error_message(VALIDATION_FAILED, &sid, &e, Some(at)));
                    }
                }
            }
            "bind" => {
                let path = op["path"].as_str().unwrap_or("").to_string();
                let Some(source) = op["source"].as_str().and_then(parse_source) else { return };
                if self.entry(&sid).is_none() {
                    return;
                }
                let Some(resolver) = self.sources.get(&source.scheme) else {
                    let at = if path.is_empty() { "/" } else { path.as_str() };
                    let msg = error_message(VALIDATION_FAILED, &sid, &format!("no resolver for the source scheme {}", source.scheme), Some(at));
                    self.send(msg);
                    return;
                };
                let alive = Arc::new(AtomicBool::new(true));
                let emit: Emit = {
                    let (tx, alive, sid, path) = (self.tx.clone(), alive.clone(), sid.clone(), path.clone());
                    Arc::new(move |value| {
                        if alive.load(Ordering::SeqCst) {
                            let _ = tx.send(Inbound::Source { surface_id: sid.clone(), alive: alive.clone(), path: path.clone(), value });
                        }
                    })
                };
                let cancel = resolver(source, emit);
                self.subscriptions.entry(sid).or_default().push((alive, cancel));
            }
            "delete" => {
                self.forget_paint_errors(&sid);
                self.unbind(&sid);
                self.surfaces.retain(|s| s.id != sid);
                cx.notify();
            }
            "send" => {
                let message = op["message"].clone();
                if let Some(err) = message.get("error") {
                    if err["code"].as_str() == Some(UNSUPPORTED_CATALOG) {
                        let text = err["message"].as_str().unwrap_or("");
                        // "catalog <id> is not supported" → <id>.
                        self.unsupported_catalog = Some(text.strip_prefix("catalog ").and_then(|r| r.split_whitespace().next()).unwrap_or(text).to_string());
                        cx.notify();
                    }
                }
                self.send(message);
            }
            _ => {}
        }
    }

    fn unbind(&mut self, surface_id: &str) {
        for (alive, cancel) in self.subscriptions.remove(surface_id).unwrap_or_default() {
            alive.store(false, Ordering::SeqCst);
            cancel();
        }
    }

    // --- interactions out --------------------------------------------------

    /// A component's server event as the A2UI client action message, sent.
    /// `timestamp` `None` = now (ISO 8601, UTC).
    pub fn action(&mut self, event: &ActionEvent, timestamp: Option<&str>) -> Value {
        let ts = timestamp.map(str::to_string).unwrap_or_else(now_iso);
        let message = action_message(&event.surface_id, &event.component_id, &event.name, event.context.clone(), event.payload.clone(), &ts);
        self.send(message.clone());
        message
    }

    /// The policy decision for a call, before any consent hook: the host's
    /// policy, narrowed by the creating package's `functions`.
    pub fn decide(&self, surface_id: &str, name: &str) -> FunctionDecision {
        let registered = self.functions.contains_key(name);
        let mut decision = decide_function(self.policy.functions.as_ref(), name, registered);
        let pkg = self.package_id_of(surface_id).and_then(|id| self.router.package(id));
        if let Some(pkg) = pkg {
            let functions: Option<Vec<String>> = pkg.get("functions").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
            decision = combine_decisions(decision, decide_function(Some(&package_policy(functions.as_deref())), name, registered));
        }
        decision
    }

    /// An action `functionCall` to a host function: gate, consent, run. A
    /// built-in other than `openUrl` has no effect as an action.
    pub fn call_function(&mut self, call: FunctionCallInfo, cx: &mut Context<Self>) -> Task<FunctionOutcome> {
        if call.name == "openUrl" {
            let url = call.args.get("url").and_then(Value::as_str).unwrap_or("").to_string();
            let opened = self.open_url(&url, cx);
            return Task::ready(FunctionOutcome::of(if opened { FunctionDecision::Allow } else { FunctionDecision::Deny }));
        }
        let denied = error_message(FUNCTION_DENIED, &call.surface_id, &format!("{} was not allowed", call.name), None);
        match self.decide(&call.surface_id, &call.name) {
            FunctionDecision::NotFound => {
                self.send(error_message(FUNCTION_NOT_FOUND, &call.surface_id, &format!("no function {}", call.name), None));
                Task::ready(FunctionOutcome::of(FunctionDecision::NotFound))
            }
            FunctionDecision::Deny => {
                self.send(denied);
                Task::ready(FunctionOutcome::of(FunctionDecision::Deny))
            }
            FunctionDecision::Ask => {
                let consent = self.policy.on_function_call.clone().map(|hook| hook(&call, cx));
                cx.spawn(async move |this, cx| {
                    let ok = match consent {
                        Some(task) => task.await,
                        None => false,
                    };
                    if !ok {
                        let _ = this.update(cx, |host, _| host.send(denied));
                        return FunctionOutcome::of(FunctionDecision::Deny);
                    }
                    run_function(this, call, cx).await
                })
            }
            FunctionDecision::Allow => cx.spawn(async move |this, cx| run_function(this, call, cx).await),
        }
    }

    /// `openUrl` / `Link` through the URL policy (relative urls against the
    /// policy's or the media `baseUrl`). True when it opened.
    pub fn open_url(&mut self, url: &str, cx: &mut App) -> bool {
        let d = decide_url(Some(&self.urls.borrow()), url);
        let (true, Some(abs)) = (d.allowed, d.url) else { return false };
        match &self.policy.open_url {
            Some(open) => open(&abs, cx),
            None => cx.open_url(&abs),
        }
        true
    }

    /// The media loader's request (absolute url + the media rules' headers).
    pub fn media_request(&self, url: &str) -> Option<MediaRequest> {
        media_request(url, &self.media.borrow())
    }

    /// `onPaintError` (`catalog/host.json` paint): a component's painter
    /// failed. Forwarded ONCE per surface + component + message as an A2UI
    /// `RENDER_FAILED` error (and so a host issue).
    pub fn paint_error(&mut self, error: &PaintError) {
        let key = format!("{}\0{}\0{}", error.surface_id, error.component_id, error.message);
        if !self.paint_errors.insert(key) {
            return;
        }
        let path = format!("/components/{}", error.component_id);
        self.send(error_message(RENDER_FAILED, &error.surface_id, &error.message, Some(&path)));
    }

    fn forget_paint_errors(&mut self, surface_id: &str) {
        let prefix = format!("{surface_id}\0");
        self.paint_errors.retain(|k| !k.starts_with(&prefix));
    }

    /// The painter callbacks routed through this host, over `base`.
    pub fn plugin(&self, base: Rc<dyn HostPlugin>) -> Rc<dyn HostPlugin> {
        host_plugin_with(self.this.clone(), base, self.media.clone(), self.urls.clone())
    }
}

/// The URL policy every href passes: the host's `urls`, relative urls
/// against its `baseUrl`, else the media `baseUrl`.
fn effective_urls(policy: &HostPolicy) -> UrlPolicy {
    let mut urls = policy.urls.clone().unwrap_or_default();
    if urls.base_url.is_none() {
        urls.base_url = policy.media.as_ref().and_then(|m| m.base_url.clone());
    }
    urls
}

async fn run_function(this: WeakEntity<ExponentialHost>, call: FunctionCallInfo, cx: &mut gpui::AsyncApp) -> FunctionOutcome {
    let task = this.update(cx, |host, cx| host.functions.get(&call.name).cloned().map(|f| f(call.args.clone(), &call, cx)));
    match task {
        Ok(Some(task)) => match task.await {
            Ok(result) => FunctionOutcome { decision: FunctionDecision::Allow, result: Some(result), error: None },
            Err(e) => FunctionOutcome { decision: FunctionDecision::Allow, result: None, error: Some(e) },
        },
        _ => FunctionOutcome::of(FunctionDecision::Allow),
    }
}

/// The [`HostPlugin`] a host-fed [`SurfaceView`] gets: actions become A2UI
/// client messages on the host's transport (and still reach
/// `base.on_action`), host functions pass its policy gate, urls its URL
/// policy, media its media rules; everything else is `base`'s.
pub fn host_plugin(host: &Entity<ExponentialHost>, base: Rc<dyn HostPlugin>, cx: &App) -> Rc<dyn HostPlugin> {
    host.read(cx).plugin(base)
}

fn host_plugin_with(host: WeakEntity<ExponentialHost>, base: Rc<dyn HostPlugin>, media: Rc<RefCell<MediaOptions>>, urls: Rc<RefCell<UrlPolicy>>) -> Rc<dyn HostPlugin> {
    Rc::new(HostAdapter { host, base, media, urls })
}

struct HostAdapter {
    host: WeakEntity<ExponentialHost>,
    base: Rc<dyn HostPlugin>,
    media: Rc<RefCell<MediaOptions>>,
    urls: Rc<RefCell<UrlPolicy>>,
}

impl HostPlugin for HostAdapter {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        self.base.icon(name)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, _| {
                h.action(event, None);
            });
        }
        self.base.on_action(event, cx)
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, cx| h.call_function(event.clone(), cx)).detach();
        }
        self.base.on_function_call(event, cx)
    }

    fn url_policy(&self) -> Option<UrlPolicy> {
        Some(self.urls.borrow().clone())
    }

    fn media_options(&self) -> MediaOptions {
        self.media.borrow().clone()
    }

    /// The host's media policy + rules over `base.resolve_url(src)`.
    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        media_request(&self.base.resolve_url(src), &self.media.borrow())
    }

    fn on_paint_error(&self, error: &PaintError, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, _| h.paint_error(error));
        }
        self.base.on_paint_error(error, cx)
    }

    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        self.base.on_input(event, cx)
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, cx| {
                h.open_url(url, cx);
            });
        }
    }

    fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
        self.base.open_media_file(path, cx)
    }

    fn on_unknown(&self, node: &PlacedNode) {
        self.base.on_unknown(node)
    }

    fn resolve_url(&self, src: &str) -> String {
        self.base.resolve_url(src)
    }

    fn font_family(&self, family: &str) -> gpui::SharedString {
        self.base.font_family(family)
    }

    fn markdown(&self, text: &str, text_style: &TextStyle, width: f32, window: &mut gpui::Window, cx: &mut App) -> Option<gpui::AnyElement> {
        self.base.markdown(text, text_style, width, window, cx)
    }

    fn announce(&self, text: &str, live: &str, cx: &mut App) {
        self.base.announce(text, live, cx)
    }

    fn pick_files(&self, request: &crate::host::FilePickRequest, cx: &mut App) -> bool {
        self.base.pick_files(request, cx)
    }

    fn on_upload(&self, event: &crate::host::UploadEvent, cx: &mut App) {
        self.base.on_upload(event, cx)
    }

    fn scroll_surface(&self, x: f32, y: f32, cx: &mut App) {
        self.base.scroll_surface(x, y, cx)
    }
}

/// Now as ISO 8601 UTC with milliseconds (JavaScript's `toISOString`).
pub fn now_iso() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    iso_from_millis(d.as_millis() as i64)
}

/// Milliseconds since the epoch → `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso_from_millis(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, milli) = (rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}

#[cfg(test)]
mod tests {
    use super::iso_from_millis;

    #[test]
    fn timestamps_match_javascript_to_iso_string() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_millis(1_791_460_800_123), "2026-10-08T12:00:00.123Z");
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
