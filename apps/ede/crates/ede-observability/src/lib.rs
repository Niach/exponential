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
    let user_id = plan.user_id.clone();
    let guard = sentry::init(options(&plan));
    sentry::configure_scope(|scope| {
        scope.set_tag("component", component);
        if let Some(id) = user_id {
            scope.set_user(Some(sentry::User { id: Some(id), ..Default::default() }));
        }
    });
    stats::init(guard.is_enabled());
    if guard.is_enabled() {
        // #ERR-12: the last launches' native crashes, each with its release.
        // Deleted once flushed, off the startup path.
        let hub = sentry::Hub::current();
        let _ = std::thread::Builder::new().name("ede-crash-reports".into()).spawn(move || {
            let sent = crash_monitor::send_pending(
                &crashes_dir(),
                |event| capture_as_reported(&hub, event),
                || hub.client().is_some_and(|client| client.flush(Some(Duration::from_secs(10)))),
            );
            if sent > 0 {
                tracing::info!(sent, "native crash reports sent");
            }
        });
    }
    tracing::info!(environment = %plan.environment, "diagnostics on");
    Guard(Some(guard))
}

/// A report another launch left (a native crash, of whichever binary)
/// keeps the tags it was written with: this process's `component` is not
/// the crashed one's (#ERR-29: seven binaries send each other's).
fn capture_as_reported(hub: &sentry::Hub, event: sentry::protocol::Event<'static>) {
    hub.with_scope(
        |scope| scope.remove_tag("component"),
        || {
            hub.capture_event(event);
        },
    );
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

    /// #ERR-29: a crash report another binary left is sent under ITS
    /// component, not the sender's (a scope's tags win over an event's).
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
        let hub = sentry::Hub::new(Some(Arc::new(sentry::Client::from(options))), Arc::new(sentry::Scope::default()));
        hub.configure_scope(|scope| scope.set_tag("component", "shell"));
        let mut tags = sentry::protocol::Map::new();
        tags.insert("component".to_owned(), "sealed-lock".to_owned());
        capture_as_reported(&hub, sentry::protocol::Event { message: Some("SIGSEGV".into()), tags, ..Default::default() });
        hub.capture_message("the shell's own", sentry::Level::Error);
        let components: Vec<Option<String>> = sent.lock().unwrap().iter().filter_map(|envelope| envelope.event()).map(|event| event.tags.get("component").cloned()).collect();
        assert_eq!(components, [Some("sealed-lock".to_owned()), Some("shell".to_owned())]);
    }
}
