//! Exponential Desktop's diagnostics (#LOGS-2): crashes (panics), errors,
//! performance transactions and structured logs through the Sentry SDK, and
//! the compositor's frame-time + main-loop hang spans. eDE's own small
//! module, mirroring the IDE's conventions (eDE is a separate Cargo
//! workspace).
//!
//! OFF unless the Exponential server says otherwise. Nothing is baked in:
//! the signed-in account's server (the IDE's shared `accounts.json`) answers
//! `GET <server>/api/monitoring/config?platform=ede`; the last answer is
//! cached on disk ([`config`]) so a launch initialises before the network,
//! and every envelope goes THROUGH that server (`POST <server>/api/monitoring`,
//! [`transport::Tunnel`]), never straight to the collector. No account, a
//! `{"enabled": false}` answer (self-hosters and dev by default),
//! `EXP_DIAGNOSTICS=0` or `[diagnostics] send = false` in `compositor.toml`
//! = off. `EXP_SENTRY_DSN` is a local development override (sent straight
//! to that DSN). `EXP_FRAME_STATS=1` prints a frame-time summary to stderr
//! with or without Sentry ([`stats`]).
//!
//! Native crashes (#ERR-12): a long-running binary calls
//! [`watch_native_crashes`]; a monitor process (the same binary, see
//! [`run_crash_monitor_if_requested`]) writes a walked report of every
//! signal/abort crash, and the next launch with a client sends it with the
//! crashed release (deleted unsent when diagnostics are off).
//!
//! Privacy: `send_default_pii` off, the hostname dropped, the user is the
//! account's pseudonymous id alone, and [`scrub`] filters credentials out
//! of every event, breadcrumb and log. A binary the person's content
//! passes through (the shell, the sealed surfaces) sends no line's text at
//! all below an error ([`Trail::Places`], #ERR-29).

pub mod config;
mod limit;
pub mod scrub;
pub mod stats;
pub mod transport;

use std::time::Duration;

use tracing::Level;
use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    fmt::writer::BoxMakeWriter,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

pub use stats::{record_frame, LoopWatch, HANG, LONG_FRAME, TICK};

/// `exponential-ede@<version>`: one release name for every eDE binary.
pub const RELEASE: &str = concat!("exponential-ede@", env!("CARGO_PKG_VERSION"));

/// First thing in `main`: a crash-monitor process runs and exits here.
pub use crash_monitor::run_monitor_if_requested as run_crash_monitor_if_requested;

/// `$XDG_CACHE_HOME/exponential-desktop/crashes`.
fn crashes_dir() -> std::path::PathBuf {
    config::cache_path().with_file_name("crashes")
}

fn diagnostics_consent() -> bool {
    config::diagnostics_on(std::env::var("EXP_DIAGNOSTICS").ok().as_deref(), std::fs::read_to_string(config::compositor_toml()).ok().as_deref())
}

/// #ERR-12: report this process's native crashes (a long-running binary
/// only: one monitor process each). Nothing when diagnostics are off.
pub fn watch_native_crashes(component: &'static str) {
    if diagnostics_consent() {
        crash_monitor::install(crash_monitor::Setup { dir: crashes_dir(), release: RELEASE.to_string(), component: component.to_string() });
    }
}

/// Keeps the Sentry client alive; dropping it flushes what is queued.
pub struct Guard(Option<sentry::ClientInitGuard>);

impl Guard {
    /// Whether this process reports to Sentry.
    pub fn enabled(&self) -> bool {
        self.0.is_some()
    }
}

/// Where the human-readable log goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Stdout,
    Stderr,
}

/// What of a binary's log lines a report carries (#ERR-29). Panics, native
/// crashes and `error!` lines are events either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trail {
    /// Every INFO+ line with its fields as a breadcrumb, WARN+ also as a
    /// log, `ede` spans as transactions: a binary whose lines are written
    /// to be sent (the compositor, the session, the services, the portal,
    /// the shell's supervisor).
    Lines,
    /// Only WHERE a line was logged (its target, level and `file:line`),
    /// never its text or fields, and no logs or transactions: a binary the
    /// person's content passes through (the shell, the sealed surfaces),
    /// whose lines name chats, notifications, accounts and paths. Its
    /// `error!` lines are the events, so they carry nothing personal.
    Places,
}

/// [`Trail::Places`]: one breadcrumb per INFO+ line, saying where.
struct Places<F>(F);

/// The breadcrumb of a line at `metadata`: nothing of the line itself.
fn place(metadata: &tracing::Metadata<'_>) -> sentry::Breadcrumb {
    let level = match *metadata.level() {
        Level::ERROR => sentry::Level::Error,
        Level::WARN => sentry::Level::Warning,
        _ => sentry::Level::Info,
    };
    // From the crate on: a path dependency's `file!()` is the build
    // machine's absolute path.
    fn within(file: &str) -> &str {
        file.rsplit_once("crates/").or_else(|| file.split_once("/registry/src/").and_then(|(_, rest)| rest.split_once('/'))).map_or(file, |(_, rest)| rest)
    }
    let at = match (metadata.file(), metadata.line()) {
        (Some(file), Some(line)) => format!("{}:{line}", within(file)),
        _ => metadata.name().to_owned(),
    };
    sentry::Breadcrumb { ty: "log".into(), category: Some(metadata.target().to_owned()), level, message: Some(at), ..Default::default() }
}

impl<S: tracing::Subscriber, F: Fn(sentry::Breadcrumb) + 'static> Layer<S> for Places<F> {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        (self.0)(place(event.metadata()));
    }
}

/// `log` records reach tracing through one callsite per level, named this
/// (tracing-log). A dependency's `log::error!` is no event of ours.
const LOG_BRIDGE: &str = "log event";

/// [`init_with`] sending every line ([`Trail::Lines`]).
pub fn init(component: &'static str, default_filter: &str, output: Output) -> Guard {
    init_with(component, default_filter, output, Trail::Lines)
}

/// The process's tracing subscriber (`RUST_LOG`, else `default_filter`, to
/// `output`) plus, when diagnostics are on and the server enabled them, the
/// Sentry client and its tracing layers, sending `trail` of the lines.
/// `component` tags every event (`compositor`, `session`, `services`,
/// `shell`, `sealed-<mode>`, `portal`, `ui-supervise`).
pub fn init_with(component: &'static str, default_filter: &str, output: Output, trail: Trail) -> Guard {
    limit::started();
    // The ring provider for every rustls user in the process (the tunnel's
    // reqwest has none of its own); a second install is a no-op.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let plan = config::resolve();

    let writer = match output {
        Output::Stdout => BoxMakeWriter::new(std::io::stdout),
        Output::Stderr => BoxMakeWriter::new(std::io::stderr),
    };
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());
    let fmt = tracing_subscriber::fmt::layer().with_writer(writer).with_filter(env_filter);
    // The person's content passes through this binary: where, never what.
    let places = (plan.is_some() && trail == Trail::Places).then(|| {
        use sentry::integrations::tracing::ErrorLayer;
        let errors = tracing_subscriber::filter::filter_fn(|metadata| *metadata.level() == Level::ERROR && metadata.name() != LOG_BRIDGE).with_max_level_hint(Level::ERROR);
        Places(|crumb| sentry::add_breadcrumb(crumb)).with_filter(LevelFilter::INFO).and_then(ErrorLayer::new().with_filter(errors))
    });
    let sentry_layers = (plan.is_some() && trail == Trail::Lines).then(|| {
        use sentry::integrations::tracing::{BreadcrumbLayer, ErrorLayer, LogLayer, SpanLayer};
        // Static filters only (callsite interest + a level hint), so the
        // renderer's trace!/debug! callsites stay disabled. Spans: ours only
        // (smithay's long-lived backend spans would be transactions that
        // never end). Logs: ours from INFO, everyone's from WARN.
        // Breadcrumbs from INFO (before the error layer, so an error carries
        // its own). Errors: every ERROR.
        SpanLayer::new()
            .with_filter(Targets::new().with_target("ede", Level::INFO))
            .and_then(BreadcrumbLayer::new().with_filter(LevelFilter::INFO))
            .and_then(ErrorLayer::new().with_filter(LevelFilter::ERROR))
            .and_then(LogLayer::new().with_filter(Targets::new().with_default(Level::WARN).with_target("ede", Level::INFO)))
    });
    tracing_subscriber::registry().with(fmt).with(sentry_layers).with(places).init();

    // The answer for the next launch (never this one: no waiting on the
    // network before the desktop starts).
    config::refresh_in_background();

    let Some(plan) = plan else {
        if !diagnostics_consent() {
            crash_monitor::discard_pending(&crashes_dir());
        }
        stats::init(false);
        return Guard(None);
    };
    let user = plan.user_id.clone().map(|id| sentry::User { id: Some(id), ..Default::default() });
    let guard = sentry::init(options(&plan));
    sentry::configure_scope(|scope| {
        scope.set_tag("component", component);
        if let Some(user) = user.clone() {
            scope.set_user(Some(user));
        }
    });
    stats::init(guard.is_enabled());
    if guard.is_enabled() {
        // #ERR-12: the last launches' native crashes, each with its release.
        // Deleted once flushed, off the startup path.
        let hub = reports_hub(sentry::Hub::current().client(), user);
        let _ = std::thread::Builder::new().name("ede-crash-reports".into()).spawn(move || {
            let sent = send_pending_reports(&hub, &crashes_dir());
            if sent > 0 {
                tracing::info!(sent, "native crash reports sent");
            }
        });
    }
    tracing::info!(environment = %plan.environment, "diagnostics on");
    Guard(Some(guard))
}

/// The reports another launch left, through `hub`; deleted once flushed.
/// #ERR-29: and only while its client is still open. A binary that stops
/// right after its start (the portal without a compositor, a shell that
/// cannot open its directory) closes the client under this thread: what it
/// captures then goes nowhere, and a flush of a closed client says "all
/// sent". Those reports wait for the next launch.
fn send_pending_reports(hub: &sentry::Hub, dir: &std::path::Path) -> usize {
    crash_monitor::send_pending(
        dir,
        |event| {
            hub.capture_event(event);
        },
        || hub.client().is_some_and(|client| client.flush(Some(Duration::from_secs(10))) && client.is_enabled()),
    )
}

/// #ERR-29: the hub the reports another launch left go through (a native
/// crash, of whichever binary): the client and the person's id, and none
/// of this process's tags, so a report keeps the `component` it was
/// written with (seven binaries send each other's). Its own hub: a scope
/// pushed on the process's would hide this process's tag from its own
/// events meanwhile.
fn reports_hub(client: Option<std::sync::Arc<sentry::Client>>, user: Option<sentry::User>) -> sentry::Hub {
    let mut scope = sentry::Scope::default();
    scope.set_user(user);
    sentry::Hub::new(client, std::sync::Arc::new(scope))
}

/// Sends what is queued (up to 2 s): before a `std::process::exit`, which
/// skips the [`Guard`]'s drop.
pub fn flush() {
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(Duration::from_secs(2)));
    }
}

fn options(plan: &config::Plan) -> sentry::ClientOptions {
    let target = plan.target.clone();
    let mut options = sentry::ClientOptions::new()
        .release(RELEASE)
        .environment(plan.environment.clone())
        .sample_rate(1.0)
        .traces_sample_rate(plan.traces_sample_rate)
        .send_default_pii(false)
        .enable_logs(true)
        .before_send(|event| {
            if !limit::allow_event(&event) {
                return None;
            }
            Some(scrub::event(event))
        })
        .before_breadcrumb(|crumb| Some(scrub::breadcrumb(crumb)))
        .before_send_log(|log| Some(scrub::log(log)))
        .transport(move |_: &sentry::ClientOptions| -> std::sync::Arc<dyn sentry::Transport> { std::sync::Arc::new(transport::Tunnel::new(target.clone())) });
    // Parsed once in `config` (the builder's `dsn()` panics on a bad one).
    options.dsn = Some(plan.dsn.clone());
    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// #ERR-29: a binary the person's content passes through sends where a
    /// line was logged, never its text or its fields.
    #[test]
    fn a_place_says_where_and_never_what() {
        let crumbs = Arc::new(Mutex::new(Vec::<sentry::Breadcrumb>::new()));
        let seen = crumbs.clone();
        let subscriber = tracing_subscriber::registry().with(Places(move |crumb| seen.lock().unwrap().push(crumb)).with_filter(LevelFilter::INFO));
        tracing::subscriber::with_default(subscriber, || {
            let (email, summary) = ("ada@example.com", "Lunch at 12?");
            tracing::info!(account = %email, "account sync");
            tracing::warn!("send the message: {summary}");
            tracing::debug!("below INFO: no breadcrumb");
        });
        let crumbs = crumbs.lock().unwrap();
        assert_eq!(crumbs.len(), 2, "{crumbs:?}");
        let sent = format!("{crumbs:?}");
        assert!(!sent.contains("ada@example.com") && !sent.contains("Lunch") && !sent.contains("account sync"), "{sent}");
        for crumb in crumbs.iter() {
            assert!(crumb.data.is_empty(), "no fields: {crumb:?}");
            assert_eq!(crumb.category.as_deref(), Some(module_path!()));
            let at = crumb.message.as_deref().unwrap();
            assert!(at.starts_with("ede-observability/src/lib.rs:") && at.ends_with(|c: char| c.is_ascii_digit()), "file:line, from the crate on: {at}");
        }
        assert_eq!((crumbs[0].level, crumbs[1].level), (sentry::Level::Info, sentry::Level::Warning));
    }

    /// #ERR-29: reports a closed client could not send stay for the next
    /// launch; an open one sends them and they go.
    #[test]
    fn a_report_waits_while_the_client_is_closed() {
        struct Collect(Arc<Mutex<Vec<sentry::Envelope>>>);
        impl sentry::Transport for Collect {
            fn send_envelope(&self, envelope: sentry::Envelope) {
                self.0.lock().unwrap().push(envelope);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let report = dir.path().join("6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11.json");
        std::fs::write(
            &report,
            format!(r#"{{"id":"6c8f0b2c-9c55-4c8a-9b0f-0f3c1f0e9a11","release":"exponential-ede@0.2.28","component":"portal","timestamp":{now},"os":"Linux","os_version":null,"arch":"x86_64","kind":"SIGSEGV","reason":"SIGSEGV / SEGV_MAPERR","address":0,"thread_id":7,"thread_name":null,"frames":[{{"address":4096,"module":0,"trust":"context"}}],"modules":[{{"code_file":"/usr/bin/x","base":0,"size":65536,"debug_id":"b0b1b2b3-b4b5-b6b7-b8b9-babbbcbdbebf","code_id":null,"debug_file":null,"main":true}}]}}"#),
        )
        .unwrap();
        let client = |sent: &Arc<Mutex<Vec<sentry::Envelope>>>| {
            let collected = sent.clone();
            let mut options = sentry::ClientOptions::new().transport(move |_: &sentry::ClientOptions| -> Arc<dyn sentry::Transport> { Arc::new(Collect(collected.clone())) });
            options.dsn = Some("https://publickey@glitchtip.example/9".parse().unwrap());
            Arc::new(sentry::Client::from(options))
        };
        // The binary stopped: its client closed before the reports went.
        let sent = Arc::new(Mutex::new(Vec::new()));
        let closed = client(&sent);
        closed.close(None);
        assert_eq!(send_pending_reports(&reports_hub(Some(closed), None), dir.path()), 1);
        assert!(sent.lock().unwrap().is_empty() && report.exists(), "nothing went, so the report waits");
        // The next launch sends it.
        let open = client(&sent);
        assert_eq!(send_pending_reports(&reports_hub(Some(open), None), dir.path()), 1);
        assert_eq!(sent.lock().unwrap().len(), 1);
        assert!(!report.exists(), "sent, then deleted");
    }

    /// #ERR-29: a crash report another binary left is sent under ITS
    /// component, with the person's id, while this process's own events
    /// keep this process's (a scope's tags win over an event's).
    #[test]
    fn a_crash_report_keeps_the_component_it_was_written_with() {
        struct Collect(Arc<Mutex<Vec<sentry::Envelope>>>);
        impl sentry::Transport for Collect {
            fn send_envelope(&self, envelope: sentry::Envelope) {
                self.0.lock().unwrap().push(envelope);
            }
        }
        let sent = Arc::new(Mutex::new(Vec::new()));
        let collected = sent.clone();
        let mut options = sentry::ClientOptions::new().transport(move |_: &sentry::ClientOptions| -> Arc<dyn sentry::Transport> { Arc::new(Collect(collected.clone())) });
        options.dsn = Some("https://publickey@glitchtip.example/9".parse().unwrap());
        let client = Arc::new(sentry::Client::from(options));
        let person = sentry::User { id: Some("u-1".into()), ..Default::default() };
        let process = sentry::Hub::new(Some(client.clone()), Arc::new(sentry::Scope::default()));
        process.configure_scope(|scope| {
            scope.set_tag("component", "shell");
            scope.set_user(Some(person.clone()));
        });
        let reports = reports_hub(Some(client), Some(person));
        let mut tags = sentry::protocol::Map::new();
        tags.insert("component".to_owned(), "sealed-lock".to_owned());
        reports.capture_event(sentry::protocol::Event { message: Some("SIGSEGV".into()), tags, ..Default::default() });
        process.capture_message("the shell's own", sentry::Level::Error);
        let events: Vec<(Option<String>, Option<String>)> = sent
            .lock()
            .unwrap()
            .iter()
            .filter_map(|envelope| envelope.event())
            .map(|event| (event.tags.get("component").cloned(), event.user.as_ref().and_then(|user| user.id.clone())))
            .collect();
        assert_eq!(events, [(Some("sealed-lock".to_owned()), Some("u-1".to_owned())), (Some("shell".to_owned()), Some("u-1".to_owned()))]);
    }
}
