//! #LOGS-2: errors, performance and lag diagnostics for the desktop IDE and
//! the `exponential` CLI daemon, sent to the team's GlitchTip THROUGH the
//! server the app is connected to (`POST <server>/api/monitoring`).
//!
//! Off unless the server says on (`GET /api/monitoring/config`, cached in
//! `{data_dir}/diagnostics.json`) AND the person's "Send diagnostics"
//! switch is on (`EXP_DIAGNOSTICS=0` opts a process out). `EXP_SENTRY_DSN`
//! overrides the server's DSN for local testing; the envelopes still go to
//! the tunnel. Errors are always sent, transactions at
//! `SENTRY_TRACES_SAMPLE_RATE` (default 0.1). The user is the pseudonymous
//! Exponential user id, nothing else ([`scrub`]).
//!
//! `EXP_FRAME_STATS=1` turns on the lag instrumentation ([`lag`]) with a
//! stderr summary, with or without Sentry.
//!
//! Never a dependency of `crates/eagent` or the Engine runner.

pub mod config;
pub mod lag;
pub mod probe;
pub mod scrub;
mod transport;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub use transport::HttpClient;

use config::RemoteConfig;

/// Which binary reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    Desktop,
    Cli,
}

impl Product {
    fn release_name(self) -> &'static str {
        match self {
            Product::Desktop => "exponential-desktop",
            Product::Cli => "exponential-cli",
        }
    }

    /// The `platform` the config endpoint takes.
    pub fn platform(self) -> &'static str {
        match self {
            Product::Desktop => "desktop",
            Product::Cli => "cli",
        }
    }
}

pub struct Setup {
    pub product: Product,
    pub version: String,
    pub data_dir: PathBuf,
    pub http: HttpClient,
}

struct State {
    product: Product,
    version: String,
    data_dir: PathBuf,
    http: HttpClient,
    /// The server connected to and the config the client was built from.
    server: Option<String>,
    built_from: Option<(String, RemoteConfig)>,
    user_id: Option<String>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static SENTRY_ON: AtomicBool = AtomicBool::new(false);
/// A `log` record being forwarded (a send that logs must not recurse).
static FORWARDING: AtomicBool = AtomicBool::new(false);

fn state() -> Option<std::sync::MutexGuard<'static, State>> {
    STATE.get().map(|state| state.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(crate) fn sentry_enabled() -> bool {
    SENTRY_ON.load(Ordering::Relaxed)
}

/// `EXP_FRAME_STATS=1`.
pub fn frame_stats_requested() -> bool {
    std::env::var("EXP_FRAME_STATS").is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Once, first thing in `main`, on the thread that runs the UI: installs
/// the `tracing` subscriber (only when something listens) and starts the
/// client from the cached config, so early panics are caught before any
/// network.
pub fn init(setup: Setup) {
    // The main thread owns the process hub: a client bound later (the
    // config arrived, the switch came back on) reaches it.
    let _ = sentry::Hub::main();
    let frame_stats = frame_stats_requested();
    let file = config::load(&setup.data_dir);
    let server = file.last_server.clone();
    let _ = STATE.set(Mutex::new(State {
        product: setup.product,
        version: setup.version,
        data_dir: setup.data_dir,
        http: setup.http,
        server: server.clone(),
        built_from: None,
        user_id: None,
    }));
    install_subscriber(frame_stats);
    if frame_stats {
        lag::activate(true);
    }
    if let Some(server) = server {
        let cached = file.servers.get(&server).cloned();
        apply(cached.unwrap_or_default());
    }
}

/// Global `tracing` subscriber: the lag layer (it ignores every thread but
/// the UI thread) and Sentry's span + breadcrumb + error layer, both over
/// our own crates only. Both filters start OFF unless something listens
/// (`EXP_FRAME_STATS`): a process that ships nothing pays one relaxed load
/// per span callsite. [`set_levels`] opens them when a client comes up.
fn install_subscriber(frame_stats: bool) {
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt as _;
    use tracing_subscriber::{reload, Layer as _};

    let lag_level = if frame_stats { LevelFilter::TRACE } else { LevelFilter::OFF };
    let (lag_filter, lag_handle) = reload::Layer::new(our_targets(lag_level));
    let (sentry_filter, sentry_handle) = reload::Layer::new(our_targets(LevelFilter::OFF));
    use sentry::integrations::tracing::{BreadcrumbLayer, ErrorLayer, SpanLayer};
    let sentry_layer = SpanLayer::new()
        .and_then(BreadcrumbLayer::new())
        .and_then(ErrorLayer::new())
        .with_filter(sentry_filter);
    let subscriber = tracing_subscriber::registry()
        .with(lag::MainThreadLayer.with_filter(lag_filter))
        .with(sentry_layer);
    if tracing::subscriber::set_global_default(subscriber).is_ok() {
        let _ = LEVELS.set(Box::new(move |lag, sentry| {
            let _ = lag_handle.reload(our_targets(lag));
            let _ = sentry_handle.reload(our_targets(sentry));
        }));
    }
}

type SetLevels = Box<dyn Fn(tracing_subscriber::filter::LevelFilter, tracing_subscriber::filter::LevelFilter) + Send + Sync>;
static LEVELS: OnceLock<SetLevels> = OnceLock::new();

/// Open (or close) the two filters: Sentry's at INFO, the lag layer's at
/// TRACE whenever the lag data is wanted (stderr or Sentry).
fn set_levels(sentry_on: bool) {
    use tracing_subscriber::filter::LevelFilter;
    if let Some(set) = LEVELS.get() {
        let lag = if sentry_on || frame_stats_requested() { LevelFilter::TRACE } else { LevelFilter::OFF };
        set(lag, if sentry_on { LevelFilter::INFO } else { LevelFilter::OFF });
    }
}

fn our_targets(level: tracing_subscriber::filter::LevelFilter) -> tracing_subscriber::filter::Targets {
    OUR_TARGETS.iter().fold(tracing_subscriber::filter::Targets::new(), |targets, target| targets.with_target(*target, level))
}

/// The crates whose spans and events we read (module-path prefixes).
const OUR_TARGETS: &[&str] = &[
    "app", "exp_cli", "exp_desktop", "ui", "sync", "api", "coding", "steer", "engine", "eengine", "eengine_ffi", "eengine_desktop",
    "gpui_markdown_editor", "uicore_docs", "uicore_gpui", "uicore_data", "eui_gpui", "eui_core", "terminal", "observability",
];

/// The app (or daemon) is now talking to `server` as `user_id`: fetch the
/// server's monitoring config on a background thread, cache it and
/// (re)start the client when it changed.
pub fn connect(server: &str, user_id: Option<&str>) {
    let server = config::normalize_server(server);
    let (platform, data_dir, http) = {
        let Some(mut state) = state() else { return };
        state.server = Some(server.clone());
        state.user_id = user_id.map(str::to_string);
        (state.product.platform(), state.data_dir.clone(), state.http)
    };
    apply_user();
    // Init from the cache first (a server switch, a first launch with a
    // cache); the next launch starts from this server's cache.
    let file = config::load(&data_dir);
    if file.last_server.as_deref() != Some(server.as_str()) {
        config::update(&data_dir, |file| file.last_server = Some(server.clone()));
    }
    let cached = file.servers.get(&server).cloned();
    apply(cached.unwrap_or_default());
    std::thread::Builder::new()
        .name("exp-diagnostics-config".into())
        .spawn(move || {
            let url = config::config_url(&server, platform);
            let answer = http()
                .get(&url)
                .timeout(Duration::from_secs(15))
                .send()
                .ok()
                .filter(|response| response.status().is_success())
                .and_then(|response| response.text().ok())
                .and_then(|body| serde_json::from_str::<RemoteConfig>(&body).ok());
            let Some(answer) = answer else { return };
            config::update(&data_dir, |file| {
                file.last_server = Some(server.clone());
                file.servers.insert(server.clone(), answer.clone());
            });
            let current = state().and_then(|state| state.server.clone());
            if current.as_deref() == Some(server.as_str()) {
                apply(answer);
            }
        })
        .ok();
}

/// The "Send diagnostics" switch.
pub fn diagnostics_enabled() -> bool {
    match state() {
        Some(state) => config::load(&state.data_dir).send_diagnostics,
        None => true,
    }
}

/// Flip the switch: persisted locally, and the client starts or closes now.
pub fn set_diagnostics_enabled(enabled: bool) {
    let Some(data_dir) = state().map(|state| state.data_dir.clone()) else { return };
    config::update(&data_dir, |file| file.send_diagnostics = enabled);
    reapply();
}

fn reapply() {
    let Some((data_dir, server)) = state().map(|state| (state.data_dir.clone(), state.server.clone())) else { return };
    let Some(server) = server else { return };
    let cached = config::load(&data_dir).servers.get(&server).cloned().unwrap_or_default();
    // Force a rebuild: the switch changed, not the config.
    if let Some(mut state) = state() {
        state.built_from = None;
    }
    apply(cached);
}

/// Start, rebuild or close the client for `remote` from the current server.
fn apply(remote: RemoteConfig) {
    let Some(mut state) = state() else { return };
    let Some(server) = state.server.clone() else { return };
    let consent = config::load(&state.data_dir).send_diagnostics && !config::env_opt_out();
    let dsn_override = std::env::var("EXP_SENTRY_DSN").ok().filter(|dsn| !dsn.trim().is_empty());
    let dsn = dsn_override.or_else(|| remote.enabled.then(|| remote.dsn.clone()).flatten());
    let wanted = match (consent, dsn) {
        (true, Some(dsn)) => Some(RemoteConfig { dsn: Some(dsn), ..remote }),
        _ => None,
    };
    let key = wanted.clone().map(|config| (server.clone(), config));
    if state.built_from == key && (key.is_some() == sentry_enabled()) {
        return;
    }
    state.built_from = key;
    let hub = sentry::Hub::main();
    if let Some(client) = hub.client() {
        SENTRY_ON.store(false, Ordering::Relaxed);
        client.close(Some(Duration::from_secs(2)));
        hub.bind_client(None);
        set_levels(false);
    }
    let Some(config) = wanted else { return };
    let Ok(dsn) = config.dsn.as_deref().unwrap_or_default().parse::<sentry::types::Dsn>() else {
        log::warn!(target: "observability", "diagnostics: the configured DSN does not parse; staying off");
        return;
    };
    let environment = std::env::var("SENTRY_ENVIRONMENT")
        .ok()
        .or(config.environment.clone())
        .unwrap_or_else(|| config::derive_environment(Some(&server)).to_string());
    let mut options = sentry::ClientOptions::new();
    options.dsn = Some(dsn);
    options.release = Some(format!("{}@{}", state.product.release_name(), state.version).into());
    options.environment = Some(environment.into());
    options.event_sampling_strategy = sentry::EventSamplingStrategy::FixedRate(1.0);
    options.traces_sampling_strategy = sentry::TracesSamplingStrategy::FixedRate(config::traces_sample_rate(config.traces_sample_rate));
    options.send_default_pii = false;
    options.server_name = None;
    options.max_breadcrumbs = 100;
    options.before_send = Some(Arc::new(|event| Some(scrub::scrub_event(event))));
    options.before_breadcrumb = Some(Arc::new(|breadcrumb| Some(scrub::scrub_breadcrumb(breadcrumb))));
    options.before_send_log = Some(Arc::new(|log| Some(scrub::scrub_log(log))));
    options.transport = Some(transport::TunnelTransport::factory(config::tunnel_url(&server), state.http));
    let mut options = sentry::apply_defaults(options);
    options.server_name = None;
    let client = Arc::new(sentry::Client::from(options));
    hub.bind_client(Some(client));
    SENTRY_ON.store(true, Ordering::Relaxed);
    drop(state);
    apply_user();
    // The lag data feeds Sentry's hang and long-frame transactions too.
    set_levels(true);
    lag::activate(frame_stats_requested());
}

fn apply_user() {
    let user_id = state().and_then(|state| state.user_id.clone());
    sentry::Hub::main().configure_scope(|scope| {
        scope.set_user(user_id.map(|id| sentry::User { id: Some(id), ..Default::default() }));
    });
}

/// `coding::logging`'s tap: every `log` record the file logger writes.
/// Errors become events, warnings and infos breadcrumbs.
pub fn log_tap(record: &log::Record) {
    if !sentry_enabled() || record.level() > log::Level::Info {
        return;
    }
    if FORWARDING.swap(true, Ordering::Acquire) {
        return;
    }
    let message = record.args().to_string();
    let hub = sentry::Hub::main();
    if record.level() == log::Level::Error && record.target() != "coding::panic" {
        hub.capture_event(sentry::protocol::Event {
            level: sentry::Level::Error,
            logger: Some(record.target().to_string()),
            message: Some(message),
            ..Default::default()
        });
    } else {
        hub.add_breadcrumb(sentry::Breadcrumb {
            category: Some(record.target().to_string()),
            level: match record.level() {
                log::Level::Error => sentry::Level::Error,
                log::Level::Warn => sentry::Level::Warning,
                _ => sentry::Level::Info,
            },
            message: Some(message),
            ..Default::default()
        });
    }
    FORWARDING.store(false, Ordering::Release);
}

/// Before exit: what is queued gets out (bounded).
pub fn flush() {
    if let Some(client) = sentry::Hub::main().client() {
        client.flush(Some(Duration::from_secs(2)));
    }
}
