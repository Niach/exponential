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

use eui_core::host::{
    action_message_with, available_extensions, client_capabilities, combine_decisions, decide_function, decide_url, error_message, media_request, package_policy, parse_source, source_with_params,
    action_call_name, navigate_target, run_action, ActionContext, ActionRun, FunctionDecision, FunctionPolicy, HostRouter, MediaOptions, MediaRequest, PackageIssue, ParsedSource, RouteOptions, UrlPolicy, ACTION_FAILED, FUNCTION_DENIED,
    FUNCTION_NOT_FOUND, NAVIGATE_FUNCTION, RECORDS_BATCH_FUNCTION, RENDER_FAILED, UNSUPPORTED_CATALOG, VALIDATION_FAILED,
};
use eui_core::host::{CallEffect, CallHub, PressRoute, SettledCall, SharedCalls};
use eui_core::measure::TextStyle;
use eui_core::surface::PlacedNode;
use eui_core::theme::{try_load_theme, Mode, ResolvedTheme, ThemeOptions};
use eui_core::themes::{builtin_refs, builtin_theme, default_theme, BUILTIN_THEME_IDS};
use eui_core::{ExtensionDef, FlatComponent};
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
/// #EXP-1674: [`HostPlugin::text_links`] of a policy-built host.
pub type TextLinksHandler = Rc<dyn Fn(&str, &App) -> Option<Vec<eui_core::host::TextLink>>>;
/// #VAPP-258: [`HostPlugin::image_size`] of a policy-built host.
pub type ImageSizeHandler = Rc<dyn Fn(&str, &App) -> Option<(f32, f32)>>;
/// #VAPP-258: [`HostPlugin::on_image_press`] of a policy-built host (its
/// presence makes a Markdown block image pressable).
pub type ImagePressHandler = Rc<dyn Fn(&str, &mut gpui::Window, &mut App)>;
/// #VAPP-258: [`HostPlugin::attachment`] of a policy-built host.
pub type AttachmentHandler = Rc<dyn Fn(&str, &App) -> Option<crate::host::MarkdownAttachment>>;
/// VAPP-194: the team zone, minutes east of UTC at an instant (epoch ms).
pub type ZoneFn = Arc<dyn Fn(f64) -> i32 + Send + Sync>;
/// VAPP-194: a package action that landed nothing (the host's toast).
pub type ActionFailedHook = Rc<dyn Fn(&ActionFailure, &mut App)>;

/// VAPP-218 (U1): what [`HostOptions::navigate`] receives (the TS
/// `navigate` option's target): the surface, its package, the screen and
/// its string params, checked by `eui_core::host::navigate_target`.
#[derive(Debug, Clone, PartialEq)]
pub struct HostNavigate {
    pub surface_id: String,
    pub package_id: String,
    pub screen: String,
    pub params: serde_json::Map<String, Value>,
}

/// VAPP-218: the host's shell opening a package screen; `true` when it
/// navigated.
pub type NavigateHook = Rc<dyn Fn(&HostNavigate, &mut App) -> bool>;

/// VAPP-194: why a package action landed nothing (`onActionFailed`).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionFailure {
    pub surface_id: String,
    pub action: String,
    pub message: String,
}
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
    /// A `hostLinks` Text's segments (default: none).
    pub text_links: Option<TextLinksHandler>,
    /// #VAPP-258: a Markdown block image's natural size (default: none).
    pub image_size: Option<ImageSizeHandler>,
    /// #VAPP-258: a press on a Markdown block image (default: not pressable).
    pub on_image_press: Option<ImagePressHandler>,
    /// #VAPP-258: a one-link Markdown paragraph's attachment (default: none).
    pub attachment: Option<AttachmentHandler>,
    /// #VAPP-258 step 5: the Markdown render options (default: the host's).
    pub markdown_options: Option<crate::host::MarkdownOptions>,
    pub media: Option<MediaOptions>,
}

/// How an [`ExponentialHost`] is created.
pub struct HostOptions {
    pub transport: Option<Box<dyn Transport>>,
    /// Round 9 §6.5: the transport's far end reads renderer → agent messages
    /// (the MCP carrier, WebSocket, SSE + POST): a surface an agent created
    /// routes the functions the host lacks to it.
    pub agent: bool,
    pub functions: HashMap<String, HostFunction>,
    pub sources: HashMap<String, SourceResolver>,
    /// Extension catalogs (`eui_core::extension::parse_extension`).
    pub extensions: Vec<ExtensionDef>,
    /// Their native painters, keyed on the extension kind.
    pub painters: Vec<(String, Rc<dyn ExtensionPainter>)>,
    /// Declarative app packages to install.
    pub packages: Vec<Value>,
    pub policy: HostPolicy,
    /// Every surface's theme (`None` = geometry mode) and mode.
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    /// Round 2 §3: every surface's number / date formatter (`None` = the
    /// core's English one).
    pub formatter: Option<Arc<dyn eui_core::format::Formatter>>,
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
    /// VAPP-194: the member a package action runs as (its `me`).
    pub me: Option<String>,
    /// VAPP-194: the clock (epoch ms; `None` = the wall clock) and the team
    /// zone (`None` = UTC) a package action reads.
    pub now: Option<Rc<dyn Fn() -> f64>>,
    pub zone: Option<ZoneFn>,
    /// VAPP-194: a package action that landed nothing (a false require's
    /// message, a refused write or call): a failed action, so the toast.
    pub on_action_failed: Option<ActionFailedHook>,
    /// VAPP-200: every surface is read-only (the viewer's app access is
    /// `read`): its writing controls paint disabled.
    pub read_only: bool,
    /// VAPP-218 (U1): open one of the surface's package screens
    /// (`app.navigate`, a press or an action's call step, checked by
    /// `navigate_target`). `None` = the host draws no screens, so a
    /// navigate is refused.
    pub navigate: Option<NavigateHook>,
    /// #VAPP-252: every surface's modal layers hang from the window
    /// ([`SurfaceViewOptions::lift_layers`]). Default off.
    pub lift_layers: bool,
}

impl HostOptions {
    /// Validate [`HostOptions::packages`] against these options' catalogs
    /// (what [`ExponentialHost::new`] would report and skip).
    pub fn validate_packages(&self) -> Result<(), PackageError> {
        let ids: Vec<&str> = self.extensions.iter().map(|e| e.id.as_str()).collect();
        let mut router = HostRouter::with_extensions(&ids, Some(available_extensions(&self.extensions)));
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
            agent: false,
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
            me: None,
            now: None,
            zone: None,
            on_action_failed: None,
            read_only: false,
            navigate: None,
            lift_layers: false,
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

/// One press waiting for its action call's answer.
#[derive(Default)]
struct AgentWait {
    answer: Option<SettledCall>,
    waker: Option<std::task::Waker>,
}

struct AgentAnswer(Rc<RefCell<AgentWait>>);

impl std::future::Future for AgentAnswer {
    type Output = SettledCall;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<SettledCall> {
        let mut wait = self.0.borrow_mut();
        match wait.answer.take() {
            Some(answer) => std::task::Poll::Ready(answer),
            None => {
                wait.waker = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        }
    }
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
    /// Round 9 §6.5: the routed calls (shared with the surfaces).
    calls: SharedCalls,
    /// The press waiting for each action call to the agent.
    agent_waiters: HashMap<String, Rc<RefCell<AgentWait>>>,
    router: HostRouter,
    surfaces: Vec<SurfaceEntry>,
    /// Per surface, one entry per bound path (VAPP-122: a re-bind of a
    /// path cancels the earlier subscription at that path).
    subscriptions: HashMap<String, Vec<(String, Arc<AtomicBool>, Cancel)>>,
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
    /// `onPaintError` dedupe: surface \0 component (cleared per component
    /// when its props change, per surface on create/delete).
    paint_errors: std::collections::HashSet<String>,
    theme: Option<Arc<ResolvedTheme>>,
    mode: Mode,
    formatter: Option<Arc<dyn eui_core::format::Formatter>>,
    read_only: bool,
    lift_layers: bool,
    base: Rc<dyn HostPlugin>,
    on_send: Option<ValueObserver>,
    on_op: Option<ValueObserver>,
    on_issue: Option<IssueObserver>,
    me: Option<String>,
    now: Option<Rc<dyn Fn() -> f64>>,
    zone: Option<ZoneFn>,
    on_action_failed: Option<ActionFailedHook>,
    navigate: Option<NavigateHook>,
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
        let calls = CallHub::shared(options.agent);
        {
            let mut hub = calls.lock().unwrap_or_else(|e| e.into_inner());
            hub.set_registered(&options.functions.keys().collect::<Vec<_>>());
            hub.set_policy(options.policy.functions.clone());
        }
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
            // VAPP-161: a package's `extensions` are checked against their versions and components.
            router: HostRouter::with_extensions(&ids, Some(available_extensions(&options.extensions))),
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
            read_only: options.read_only,
            lift_layers: options.lift_layers,
            base: options.plugin,
            on_send: options.on_send,
            on_op: options.on_op,
            on_issue: options.on_issue,
            me: options.me,
            now: options.now,
            zone: options.zone,
            on_action_failed: options.on_action_failed,
            navigate: options.navigate,
            issues: Vec::new(),
            calls,
            agent_waiters: HashMap::new(),
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
        self.router.register_extension_with(&ext.id, available_extensions(std::slice::from_ref(&ext)).pop());
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
        self.sync_calls();
    }

    fn sync_calls(&mut self) {
        let names: Vec<&String> = self.functions.keys().collect();
        let mut hub = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        hub.set_registered(&names);
        hub.set_policy(self.policy.functions.clone());
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

    /// One agent message, UNTRUSTED (a transport delivers here; round 9 §2:
    /// `applyTemplate` / `bindDataModel` answer INVALID_MESSAGE). Returns the
    /// ops performed.
    pub fn receive(&mut self, message: &Value, cx: &mut Context<Self>) -> Vec<Value> {
        self.route(message, RouteOptions::default(), cx)
    }

    /// One message from HOST code (round 9 §2): the host-internal kinds pass.
    pub fn receive_trusted(&mut self, message: &Value, cx: &mut Context<Self>) -> Vec<Value> {
        self.route(message, RouteOptions::trusted(), cx)
    }

    /// Round 9 §1.4: a fresh id for a re-opened surface.
    pub fn mint_surface_id(&mut self, base: &str) -> String {
        self.router.mint_surface_id(base)
    }

    fn route(&mut self, message: &Value, options: RouteOptions, cx: &mut Context<Self>) -> Vec<Value> {
        // VAPP-206: `${today}`, `${now}`, `${month}` at this bind, in the team zone.
        let params = eui_core::host::host_params(self.now.as_ref().map(|f| f()), self.zone.as_deref());
        let ops = self.router.route(&eui_core::host::with_host_params(message, &params), options);
        for op in &ops {
            self.perform(op, cx);
        }
        let settled = self.hub().observe(&ops, &self.router);
        self.answer_presses(settled);
        self.flush_calls(cx);
        ops
    }

    fn hub(&self) -> std::sync::MutexGuard<'_, CallHub> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn answer_presses(&mut self, settled: Vec<SettledCall>) {
        for call in settled {
            if let Some(wait) = self.agent_waiters.remove(&call.function_call_id) {
                let mut wait = wait.borrow_mut();
                wait.answer = Some(call);
                if let Some(waker) = wait.waker.take() {
                    waker.wake();
                }
            }
        }
    }

    /// Round 9 §6.5: what the hub queued: messages out, reactive effects
    /// run, surfaces whose routed value settled bound again.
    fn flush_calls(&mut self, cx: &mut Context<Self>) {
        loop {
            let (outbox, effects, dirty) = {
                let mut hub = self.hub();
                (hub.take_outbox(), hub.take_effects(), hub.take_dirty())
            };
            if outbox.is_empty() && effects.is_empty() && dirty.is_empty() {
                return;
            }
            for message in outbox {
                self.send_in(message, cx);
            }
            for effect in effects {
                self.run_effect(effect, cx);
            }
            for sid in dirty {
                if let Some(view) = self.entry(&sid).map(|e| e.view.clone()) {
                    view.update(cx, |v, cx| v.rebind(cx));
                }
            }
        }
    }

    fn run_effect(&mut self, effect: CallEffect, cx: &mut Context<Self>) {
        match effect {
            CallEffect::Ask { ticket, surface_id, name, args, .. } => {
                let info = FunctionCallInfo { surface_id, component_id: String::new(), name, args: Value::Object(args) };
                let consent = self.policy.on_function_call.clone().map(|hook| hook(&info, cx));
                cx.spawn(async move |this, cx| {
                    let allowed = match consent {
                        Some(task) => task.await,
                        None => false,
                    };
                    let _ = this.update(cx, |host, cx| {
                        host.hub().consent(ticket, allowed);
                        host.flush_calls(cx);
                    });
                })
                .detach();
            }
            CallEffect::RunLocal { ticket, surface_id, name, args, .. } => {
                let info = FunctionCallInfo { surface_id, component_id: String::new(), name, args: Value::Object(args) };
                let task = self.functions.get(&info.name).cloned().map(|f| f(info.args.clone(), &info, cx));
                cx.spawn(async move |this, cx| {
                    let answer = match task {
                        Some(task) => task.await,
                        None => Ok(Value::Null),
                    };
                    let _ = this.update(cx, |host, cx| {
                        host.hub().settle_local(ticket, answer);
                        host.flush_calls(cx);
                    });
                })
                .detach();
            }
        }
    }

    /// Round 9 §5.2, §6.8: a layout left issues or failures on the surface.
    pub fn flush_surface(&mut self, surface_id: &str, cx: &mut Context<Self>) {
        if let Some(view) = self.entry(surface_id).map(|e| e.view.clone()) {
            let calls = self.calls.clone();
            view.update(cx, |v, _| v.flush_calls(&mut calls.lock().unwrap_or_else(|e| e.into_inner())));
        }
        self.flush_calls(cx);
    }

    /// [`Self::send`] with the §1.3 metadata of a `sendDataModel` surface.
    /// Outside any surface view's update (it reads the view).
    pub fn send_in(&mut self, message: Value, cx: &mut Context<Self>) {
        let hub = self.calls.clone();
        let metadata = {
            let surfaces = &self.surfaces;
            let data = |sid: &str| surfaces.iter().find(|s| s.id == sid).map(|s| s.view.read(cx).surface().data().clone());
            hub.lock().unwrap_or_else(|e| e.into_inner()).data_model_metadata(&message, &data)
        };
        match metadata {
            Some(metadata) => {
                if let Some(t) = self.transport.as_mut() {
                    t.send_with_metadata(&message, &metadata);
                }
                let transport = self.transport.take();
                self.send(message);
                self.transport = transport;
            }
            None => self.send(message),
        }
    }

    /// An action call to the agent: the policy, the consent, one new
    /// `callAgentFunction`, its answer or the timeout (§6.5: never shared).
    fn call_agent(&mut self, call: FunctionCallInfo, cx: &mut Context<Self>) -> Task<FunctionOutcome> {
        let args = call.args.as_object().cloned().unwrap_or_default();
        let route = self.hub().press(&call.surface_id, &call.name, None, &args, None);
        let consent = (route == PressRoute::Ask).then(|| self.policy.on_function_call.clone().map(|hook| hook(&call, cx)));
        self.flush_calls(cx);
        cx.spawn(async move |this, cx| {
            let route = match consent {
                None => route,
                Some(task) => {
                    let allowed = match task {
                        Some(task) => task.await,
                        None => false,
                    };
                    this.update(cx, |host, _| host.hub().press(&call.surface_id, &call.name, None, &args, Some(allowed))).unwrap_or(PressRoute::Deny)
                }
            };
            let id = match route {
                PressRoute::Agent { function_call_id } => function_call_id,
                PressRoute::Deny | PressRoute::Ask => {
                    let _ = this.update(cx, |host, cx| host.flush_calls(cx));
                    return FunctionOutcome::of(FunctionDecision::Deny);
                }
                PressRoute::NotFound => {
                    let _ = this.update(cx, |host, cx| host.flush_calls(cx));
                    return FunctionOutcome::of(FunctionDecision::NotFound);
                }
                PressRoute::Local => return run_function(this, call, cx).await,
            };
            let wait = Rc::new(RefCell::new(AgentWait::default()));
            let _ = this.update(cx, |host, cx| {
                host.agent_waiters.insert(id, wait.clone());
                host.flush_calls(cx);
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(eui_core::limits::AGENT_FUNCTION_TIMEOUT_MS + 50)).await;
                    let _ = this.update(cx, |host, cx| {
                        let settled = host.hub().expire();
                        host.answer_presses(settled);
                        host.flush_calls(cx);
                    });
                })
                .detach();
            });
            let answer = AgentAnswer(wait).await;
            FunctionOutcome { decision: FunctionDecision::Allow, result: answer.value, error: answer.error }
        })
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
                        let msg = error_message(VALIDATION_FAILED, &sid, &format!("createSurface.metadata.extensions.at_exponential_theme is unusable: {}", issues.join("; ")), Some("/createSurface/metadata/extensions/at_exponential_theme"));
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
                    // #EXP-1694: a date-time field's seed and its join read the host's
                    // zone (team-zone.json), not UTC.
                    formatter: self.formatter.clone().or_else(|| {
                        self.zone.clone().map(|zone| Arc::new(eui_core::format::ZonedEnglishFormatter::new(zone)) as Arc<dyn eui_core::format::Formatter>)
                    }),
                    lift_layers: self.lift_layers,
                    settings: (self.read_only || self.zone.is_some()).then(|| eui_core::surface::SurfaceSettings {
                        mode: match self.mode {
                            Mode::Light => eui_core::theme::ModeSetting::Light,
                            Mode::Dark => eui_core::theme::ModeSetting::Dark,
                        },
                        read_only: self.read_only,
                        // #EXP-1694: an empty date field's calendar opens on this
                        // month in the host's zone, as on iOS and Android.
                        today: self.zone.as_ref().and_then(|_| {
                            eui_core::host::host_params(self.now.as_ref().map(|f| f()), self.zone.as_deref()).get("today").and_then(|v| v.as_str()).map(str::to_string)
                        }),
                        ..Default::default()
                    }),
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
                let route = eui_core::host::route_handle(&self.calls, &sid);
                view.update(cx, |v, _| v.set_call_route(route));
                let package_id = self.router.package_id_of(&sid);
                self.surfaces.push(SurfaceEntry { id: sid, view, catalog_id, package_id, send_data_model: op["sendDataModel"] == Value::Bool(true), theme: surface_theme });
                cx.notify();
            }
            "components" => {
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
                let subs = self.subscriptions.entry(sid).or_default();
                let (same, kept): (Vec<_>, Vec<_>) = subs.drain(..).partition(|(p, _, _)| *p == path);
                *subs = kept;
                for (_, alive, cancel) in same {
                    alive.store(false, Ordering::SeqCst);
                    cancel();
                }
                let cancel = resolver(source, emit);
                subs.push((path, alive, cancel));
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
        for (_, alive, cancel) in self.subscriptions.remove(surface_id).unwrap_or_default() {
            alive.store(false, Ordering::SeqCst);
            cancel();
        }
    }

    // --- interactions out --------------------------------------------------

    /// A component's server event as the A2UI client action message, sent.
    /// `timestamp` `None` = now (ISO 8601, UTC).
    pub fn action(&mut self, event: &ActionEvent, timestamp: Option<&str>) -> Value {
        let message = self.action_message(event, timestamp);
        self.send(message.clone());
        message
    }

    fn action_message(&self, event: &ActionEvent, timestamp: Option<&str>) -> Value {
        let ts = timestamp.map(str::to_string).unwrap_or_else(now_iso);
        action_message_with(&event.surface_id, &event.component_id, &event.name, event.context.clone(), event.payload.clone(), &ts, event.user_message.as_deref())
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
    /// built-in other than `openUrl` and `bind` has no effect as an action;
    /// VAPP-194: `actions.<name>` runs the surface's package action
    /// ([`Self::run_package_action`]).
    pub fn call_function(&mut self, call: FunctionCallInfo, cx: &mut Context<Self>) -> Task<FunctionOutcome> {
        if call.name == "openUrl" {
            let url = call.args.get("url").and_then(Value::as_str).unwrap_or("").to_string();
            let opened = self.open_url(&url, cx);
            return Task::ready(FunctionOutcome::of(if opened { FunctionDecision::Allow } else { FunctionDecision::Deny }));
        }
        if call.name == "bind" {
            let bound = self.bind(&call.surface_id, &call.args, cx);
            return Task::ready(FunctionOutcome::of(if bound { FunctionDecision::Allow } else { FunctionDecision::Deny }));
        }
        if call.name == NAVIGATE_FUNCTION {
            return Task::ready(self.navigate(&call, cx));
        }
        if let Some(action) = action_call_name(&call.name).map(str::to_string) {
            return self.run_package_action(call, action, cx);
        }
        // Round 9 §6.5: allowed, nothing local: the surface's agent.
        if !self.functions.contains_key(&call.name) && !eui_core::host::is_builtin_function(&call.name) && self.hub().has_agent(&call.surface_id) {
            return self.call_agent(call, cx);
        }
        let gate = self.gate(&call, cx);
        cx.spawn(async move |this, cx| {
            let gate = gate.await;
            if gate.decision != FunctionDecision::Allow {
                return gate;
            }
            run_function(this, call, cx).await
        })
    }

    /// The policy gate and the consent hook: `Allow`, or the refusal (sent
    /// as FUNCTION_NOT_FOUND / FUNCTION_DENIED).
    fn gate(&mut self, call: &FunctionCallInfo, cx: &mut Context<Self>) -> Task<FunctionOutcome> {
        // VAPP-218: navigate is always allowed (checked when it runs).
        if call.name == NAVIGATE_FUNCTION {
            return Task::ready(FunctionOutcome::of(FunctionDecision::Allow));
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
                let consent = self.policy.on_function_call.clone().map(|hook| hook(call, cx));
                cx.spawn(async move |this, cx| {
                    let ok = match consent {
                        Some(task) => task.await,
                        None => false,
                    };
                    if !ok {
                        let _ = this.update(cx, |host, _| host.send(denied));
                        return FunctionOutcome::of(FunctionDecision::Deny);
                    }
                    FunctionOutcome::of(FunctionDecision::Allow)
                })
            }
            FunctionDecision::Allow => Task::ready(FunctionOutcome::of(FunctionDecision::Allow)),
        }
    }

    /// VAPP-218 (U1): set (or clear) the shell's navigate hook after the
    /// host exists ([`HostOptions::navigate`]).
    pub fn set_navigate(&mut self, navigate: Option<NavigateHook>) {
        self.navigate = navigate;
    }

    /// VAPP-218: `app.navigate {screen, params?}` → the host's shell, when
    /// the surface's package has that screen. A refusal = FUNCTION_DENIED.
    pub fn navigate(&mut self, call: &FunctionCallInfo, cx: &mut App) -> FunctionOutcome {
        let package_id = self.package_id_of(&call.surface_id).map(str::to_string);
        let pkg = package_id.as_deref().and_then(|id| self.router.package(id));
        let target = match pkg {
            Some(pkg) => navigate_target(pkg, &call.args),
            None => Err("navigate: the surface has no package".to_string()),
        };
        let refuse = |host: &mut Self, message: String| {
            host.send(error_message(FUNCTION_DENIED, &call.surface_id, &message, None));
            FunctionOutcome { decision: FunctionDecision::Deny, result: None, error: Some(message) }
        };
        let target = match target {
            Ok(target) => target,
            Err(message) => return refuse(self, message),
        };
        let Some(hook) = self.navigate.clone() else {
            return refuse(self, "navigate: this host has no screens".into());
        };
        let request = HostNavigate { surface_id: call.surface_id.clone(), package_id: package_id.unwrap_or_default(), screen: target.screen, params: target.params };
        if hook(&request, cx) {
            FunctionOutcome::of(FunctionDecision::Allow)
        } else {
            refuse(self, format!("navigate: {} was not opened", request.screen))
        }
    }

    /// VAPP-194: a package action landed nothing: ACTION_FAILED +
    /// `on_action_failed` (the toast); the outcome carries the message.
    fn action_failed(&mut self, surface_id: &str, action: &str, message: String, cx: &mut App) -> FunctionOutcome {
        self.send(error_message(ACTION_FAILED, surface_id, &message, None));
        if let Some(hook) = self.on_action_failed.clone() {
            hook(&ActionFailure { surface_id: surface_id.into(), action: action.into(), message: message.clone() }, cx);
        }
        FunctionOutcome { decision: FunctionDecision::Allow, result: None, error: Some(message) }
    }

    /// VAPP-194: `actions.<name>` = the surface's package action, all or
    /// nothing (the TS `runPackageAction`): the plan over one snapshot (the
    /// args = params, the surface data, `me`, the clock), every host call's
    /// gate and consent, then the record writes as ONE `records.batch`,
    /// then the sets, then the calls. A failure before the commit lands
    /// nothing. VAPP-271: a `call` step with a `result` runs when the plan
    /// reaches it (its gate first, before the commit; a refusal or a
    /// failure = the action's failure) and the action runs again over the
    /// SAME snapshot with its answer as `results.<name>`.
    pub fn run_package_action(&mut self, call: FunctionCallInfo, action: String, cx: &mut Context<Self>) -> Task<FunctionOutcome> {
        let view = self.entry(&call.surface_id).map(|s| s.view.clone());
        let pkg = self.package_id_of(&call.surface_id).and_then(|id| self.router.package(id)).cloned();
        let (Some(view), Some(pkg)) = (view, pkg) else {
            let out = self.action_failed(&call.surface_id, &action, format!("no action {action}"), cx);
            return Task::ready(out);
        };
        let data = view.read(cx).surface().data().clone();
        let now = self.now.as_ref().map(|f| f());
        let zone = self.zone.clone();
        let offset = move |ms: f64| zone.as_ref().map_or(0, |z| z(ms));
        let me = self.me.clone();
        let run = move |results: &serde_json::Map<String, Value>, action: &str, args: &Value| {
            let ctx = ActionContext { data: Some(&data), me: me.as_deref(), now, offset: Some(&offset), max_steps: None, results: Some(results) };
            run_action(&pkg, action, args, &ctx)
        };
        let mut results = serde_json::Map::new();
        let mut outcome = match run(&results, &action, &call.args) {
            Ok(outcome) => outcome,
            Err(message) => return Task::ready(self.action_failed(&call.surface_id, &action, message, cx)),
        };
        cx.spawn(async move |this, cx| {
            let fail = |message: String, cx: &mut gpui::AsyncApp| {
                this.update(cx, |host, cx| host.action_failed(&call.surface_id, &action, message.clone(), cx))
                    .unwrap_or(FunctionOutcome { decision: FunctionDecision::Allow, result: None, error: Some(message) })
            };
            let plan = loop {
                let pending = match outcome {
                    ActionRun::Plan(plan) => break plan,
                    ActionRun::Pending(pending) => pending,
                };
                let c = FunctionCallInfo { surface_id: call.surface_id.clone(), component_id: call.component_id.clone(), name: pending.name.clone(), args: Value::Object(pending.args) };
                let Ok(gate) = this.update(cx, |host, cx| host.gate(&c, cx)) else { return FunctionOutcome::of(FunctionDecision::Allow) };
                let gate = gate.await;
                if gate.decision != FunctionDecision::Allow {
                    return fail(gate.error.unwrap_or_else(|| format!("{} was not allowed", c.name)), cx);
                }
                let answer = run_function(this.clone(), c, cx).await;
                if let Some(error) = answer.error {
                    return fail(error, cx);
                }
                if answer.decision != FunctionDecision::Allow {
                    return fail(format!("{} was not allowed", pending.name), cx);
                }
                results.insert(pending.result, answer.result.unwrap_or(Value::Null));
                outcome = match run(&results, &action, &call.args) {
                    Ok(outcome) => outcome,
                    Err(message) => return fail(message, cx),
                };
            };
            let list = |key: &str| plan.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
            let (writes, sets) = (list("writes"), list("sets"));
            let calls: Vec<FunctionCallInfo> = list("calls")
                .into_iter()
                .map(|c| FunctionCallInfo { surface_id: call.surface_id.clone(), component_id: call.component_id.clone(), name: c["name"].as_str().unwrap_or("").to_string(), args: c["args"].clone() })
                .collect();
            for c in &calls {
                let Ok(gate) = this.update(cx, |host, cx| host.gate(c, cx)) else { return FunctionOutcome::of(FunctionDecision::Allow) };
                let gate = gate.await;
                if gate.decision != FunctionDecision::Allow {
                    return fail(gate.error.unwrap_or_else(|| format!("{} was not allowed", c.name)), cx);
                }
            }
            let mut committed = serde_json::json!({"ids": []});
            if !writes.is_empty() {
                let batch = FunctionCallInfo { surface_id: call.surface_id.clone(), component_id: call.component_id.clone(), name: RECORDS_BATCH_FUNCTION.into(), args: serde_json::json!({"writes": writes}) };
                let task = this.update(cx, |host, cx| host.functions.get(RECORDS_BATCH_FUNCTION).cloned().map(|f| f(batch.args.clone(), &batch, cx)));
                let Ok(Some(task)) = task else { return fail("this host keeps no records".into(), cx) };
                match task.await {
                    Ok(result) => committed = result,
                    Err(e) => return fail(e, cx),
                }
            }
            let _ = this.update(cx, |host, cx| {
                for s in &sets {
                    let path = s["path"].as_str().unwrap_or("").to_string();
                    if let Err(error) = view.update(cx, |v, cx| v.assign_data(&path, Some(s["value"].clone()), cx)) {
                        let at = if path.is_empty() { "/" } else { path.as_str() };
                        host.send(error_message(VALIDATION_FAILED, &call.surface_id, &error, Some(at)));
                    }
                }
            });
            for c in calls {
                let name = c.name.clone();
                let outcome = run_function(this.clone(), c, cx).await;
                let error = outcome.error.or_else(|| (outcome.decision != FunctionDecision::Allow).then(|| format!("{name} was not allowed")));
                if error.is_some() {
                    return FunctionOutcome { decision: FunctionDecision::Allow, result: Some(committed), error };
                }
            }
            FunctionOutcome { decision: FunctionDecision::Allow, result: Some(committed), error: None }
        })
    }

    /// VAPP-122: the `bind` action: re-follow `path` from `source` with
    /// `params` appended (`source_with_params`), through the router like a
    /// server's bindDataModel (the surface's package allowlist applies; a
    /// refusal is the usual SOURCE_DENIED / VALIDATION_FAILED error). True
    /// when the bind op ran.
    pub fn bind(&mut self, surface_id: &str, args: &Value, cx: &mut Context<Self>) -> bool {
        let path = args.get("path").and_then(Value::as_str).unwrap_or("");
        let source = args.get("source").and_then(Value::as_str).unwrap_or("");
        if source.is_empty() || self.entry(surface_id).is_none() {
            return false;
        }
        let params = args.get("params").filter(|p| p.is_object());
        let message = serde_json::json!({
            "version": eui_core::catalog::A2UI_VERSION,
            "bindDataModel": {"surfaceId": surface_id, "path": path, "source": source_with_params(source, params)},
        });
        self.receive_trusted(&message, cx).iter().any(|op| op["op"].as_str() == Some("bind"))
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
    /// `onPaintError` → ONE `RENDER_FAILED` per surface + component until
    /// its props change ([`Self::paint_retry`]) or the surface is recreated.
    pub fn paint_error(&mut self, error: &PaintError) {
        let key = format!("{}\0{}", error.surface_id, error.component_id);
        if !self.paint_errors.insert(key) {
            return;
        }
        let path = format!("/components/{}", error.component_id);
        self.send(error_message(RENDER_FAILED, &error.surface_id, &error.message, Some(&path)));
    }

    /// A failed component got new props: its next failure reports again.
    pub fn paint_retry(&mut self, surface_id: &str, component_id: &str) {
        self.paint_errors.remove(&format!("{surface_id}\0{component_id}"));
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
    // VAPP-205: an action's `call` step reaches the built-ins a press does.
    if call.name == "openUrl" || call.name == "bind" {
        let ran = this.update(cx, |host, cx| {
            if call.name == "bind" {
                host.bind(&call.surface_id, &call.args, cx)
            } else {
                let url = call.args.get("url").and_then(Value::as_str).unwrap_or("").to_string();
                host.open_url(&url, cx)
            }
        });
        return FunctionOutcome::of(if ran.unwrap_or(false) { FunctionDecision::Allow } else { FunctionDecision::Deny });
    }
    if call.name == NAVIGATE_FUNCTION {
        return this.update(cx, |host, cx| host.navigate(&call, cx)).unwrap_or(FunctionOutcome::of(FunctionDecision::Deny));
    }
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
            // §1.3: a `sendDataModel` message reads the view, which is being
            // updated while a press arrives: it leaves right after.
            if host.read(cx).sends_data_model(&event.surface_id) {
                let event = event.clone();
                cx.defer(move |cx| {
                    host.update(cx, |h, cx| {
                        let message = h.action_message(&event, None);
                        h.send_in(message, cx);
                    })
                });
            } else {
                host.update(cx, |h, _| {
                    h.action(event, None);
                });
            }
        }
        self.base.on_action(event, cx)
    }

    fn after_layout(&self, surface_id: &str, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, cx| h.flush_surface(surface_id, cx));
        }
    }

    fn on_function_call(&self, event: &FunctionCallEvent, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            // Deferred: a press arrives INSIDE the surface view's update, and a
            // package action reads that view's data (`run_package_action`):
            // calling it synchronously panicked gpui ("cannot read SurfaceView
            // while it is already being updated"; eApps dogfood 2026-11-03).
            let event = event.clone();
            cx.defer(move |cx| host.update(cx, |h, cx| h.call_function(event, cx)).detach());
        }
        self.base.on_function_call(event, cx)
    }

    fn on_function_call_then(&self, event: &FunctionCallEvent, then: Box<dyn FnOnce(&mut App)>, cx: &mut App) {
        let Some(host) = self.host.upgrade() else { return then(cx) };
        let call = event.clone();
        cx.defer(move |cx| {
            let task = host.update(cx, |h, cx| h.call_function(call, cx));
            cx.spawn(async move |cx| {
                let outcome = task.await;
                if outcome.error.is_none() && !matches!(outcome.decision, FunctionDecision::Deny | FunctionDecision::NotFound) {
                    let _ = cx.update(then);
                }
            })
            .detach();
        });
        self.base.on_function_call(event, cx)
    }

    fn url_policy(&self) -> Option<UrlPolicy> {
        Some(self.urls.borrow().clone())
    }

    fn media_options(&self) -> MediaOptions {
        self.media.borrow().clone()
    }

    /// The base plugin's request first (a host override: signed urls, its
    /// own headers), re-checked against the host's media schemes and hosts
    /// and given the host's rule headers for its url (the base's win per
    /// header); a base that answers `None` defers to the host's policy over
    /// `base.resolve_url(src)` (the default hook only knows the default
    /// schemes).
    fn media_allowed(&self, src: &str) -> bool {
        self.base.media_allowed(src)
    }

    /// The base's deny-only [`HostPlugin::media_allowed`] first (on the raw
    /// src: a `false` loads nothing, whatever the policy says).
    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        if !self.base.media_allowed(src) {
            return None;
        }
        let media = self.media.borrow();
        match self.base.media_request(src) {
            Some(req) => {
                let checked = media_request(&req.url, &MediaOptions { base_url: None, ..media.clone() })?;
                let mut headers = checked.headers;
                headers.extend(req.headers);
                Some(MediaRequest { url: checked.url, headers })
            }
            None => media_request(&self.base.resolve_url(src), &media),
        }
    }

    fn on_paint_error(&self, error: &PaintError, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, _| h.paint_error(error));
        }
        self.base.on_paint_error(error, cx)
    }

    fn on_paint_retry(&self, surface_id: &str, component_id: &str, cx: &mut App) {
        if let Some(host) = self.host.upgrade() {
            host.update(cx, |h, _| h.paint_retry(surface_id, component_id));
        }
        self.base.on_paint_retry(surface_id, component_id, cx)
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

    fn text_links(&self, text: &str, cx: &App) -> Option<Vec<eui_core::host::TextLink>> {
        let own = self.host.upgrade().and_then(|host| host.read(cx).policy.text_links.clone());
        match own {
            Some(links) => links(text, cx),
            None => self.base.text_links(text, cx),
        }
    }

    fn image_size(&self, src: &str, cx: &App) -> Option<(f32, f32)> {
        match self.host.upgrade().and_then(|host| host.read(cx).policy.image_size.clone()) {
            Some(size) => size(src, cx),
            None => self.base.image_size(src, cx),
        }
    }

    fn image_pressable(&self, cx: &App) -> bool {
        self.host.upgrade().is_some_and(|host| host.read(cx).policy.on_image_press.is_some()) || self.base.image_pressable(cx)
    }

    fn on_image_press(&self, src: &str, window: &mut gpui::Window, cx: &mut App) {
        match self.host.upgrade().and_then(|host| host.read(cx).policy.on_image_press.clone()) {
            Some(press) => press(src, window, cx),
            None => self.base.on_image_press(src, window, cx),
        }
    }

    fn attachment(&self, href: &str, cx: &App) -> Option<crate::host::MarkdownAttachment> {
        match self.host.upgrade().and_then(|host| host.read(cx).policy.attachment.clone()) {
            Some(attachment) => attachment(href, cx),
            None => self.base.attachment(href, cx),
        }
    }

    fn markdown_options(&self, cx: &App) -> crate::host::MarkdownOptions {
        match self.host.upgrade().and_then(|host| host.read(cx).policy.markdown_options) {
            Some(options) => options,
            None => self.base.markdown_options(cx),
        }
    }

    fn markdown_options_for(&self, text: &str, cx: &App) -> crate::host::MarkdownOptions {
        match self.host.upgrade().and_then(|host| host.read(cx).policy.markdown_options) {
            Some(options) => options,
            None => self.base.markdown_options_for(text, cx),
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

    fn app_files(&self) -> Option<std::sync::Arc<dyn crate::host::AppFiles>> {
        self.base.app_files()
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

    /// VAPP-103 rfix: the base plugin's media hook runs first again (a
    /// signed url), then the host's schemes/hosts re-check it and its rules
    /// add their headers; a base `None` defers to the host's policy.
    #[test]
    fn the_base_media_hook_runs_first_under_the_hosts_policy() {
        use super::*;
        struct Signing;
        impl HostPlugin for Signing {
            fn media_request(&self, src: &str) -> Option<MediaRequest> {
                if src.contains("deny") {
                    return None;
                }
                let url = if src.contains("evil") { "https://evil.example/x".to_string() } else { format!("https://files.example{src}?sig=1") };
                Some(MediaRequest { url, headers: [("x-sig".to_string(), "s".to_string())].into_iter().collect() })
            }
        }
        let rules = vec![eui_core::host::MediaRule { prefix: "https://files.example/".into(), headers: [("authorization".to_string(), "Bearer t".to_string())].into_iter().collect() }];
        let media = MediaOptions { hosts: Some(vec!["files.example".into()]), rules: Some(rules), schemes: Some(vec!["https".into(), "file".into()]), ..Default::default() };
        let plugin = host_plugin_with(WeakEntity::new_invalid(), Rc::new(Signing), Rc::new(RefCell::new(media)), Rc::new(RefCell::new(UrlPolicy::default())));
        let req = plugin.media_request("/a.png").expect("the override's url");
        assert_eq!(req.url, "https://files.example/a.png?sig=1");
        assert_eq!(req.headers.get("x-sig").map(String::as_str), Some("s"));
        assert_eq!(req.headers.get("authorization").map(String::as_str), Some("Bearer t"));
        assert!(plugin.media_request("/evil").is_none(), "the host's hosts list re-checks the override");
        assert_eq!(plugin.media_request("file:///tmp/deny.png").map(|r| r.url), Some("file:///tmp/deny.png".to_string()), "a base None defers to the host's policy");
        assert!(plugin.media_request("https://other.example/deny.png").is_none());
    }

    /// VAPP-157: a base's deny-only `media_allowed` sees the raw src and
    /// denies before the base hook or the host's policy run; it never widens.
    #[test]
    fn the_base_media_allowed_narrows_only() {
        use super::*;
        struct Narrow;
        impl HostPlugin for Narrow {
            fn media_allowed(&self, src: &str) -> bool {
                src.starts_with("/ok")
            }
        }
        let media = MediaOptions { base_url: Some("https://files.example".into()), ..Default::default() };
        let plugin = host_plugin_with(WeakEntity::new_invalid(), Rc::new(Narrow), Rc::new(RefCell::new(media)), Rc::new(RefCell::new(UrlPolicy::default())));
        assert_eq!(plugin.media_request("/ok.png").map(|r| r.url), Some("https://files.example/ok.png".to_string()));
        assert!(plugin.media_request("https://files.example/no.png").is_none());
        assert!(!plugin.media_allowed("/no.png"));
    }

    #[test]
    fn timestamps_match_javascript_to_iso_string() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_millis(1_791_460_800_123), "2026-10-08T12:00:00.123Z");
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
