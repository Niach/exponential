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
//! Privacy: `send_default_pii` off, the hostname dropped, the user is the
//! account's pseudonymous id alone, and [`scrub`] filters credentials out
//! of every event, breadcrumb and log.

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

/// The process's tracing subscriber (`RUST_LOG`, else `default_filter`, to
/// `output`) plus, when diagnostics are on and the server enabled them, the
/// Sentry client and its tracing layers. `component` tags every event
/// (`compositor`, `session`, `services`).
pub fn init(component: &'static str, default_filter: &str, output: Output) -> Guard {
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
    let sentry_layers = plan.is_some().then(|| {
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
    tracing_subscriber::registry().with(fmt).with(sentry_layers).init();

    // The answer for the next launch (never this one: no waiting on the
    // network before the desktop starts).
    config::refresh_in_background();

    let Some(plan) = plan else {
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
    tracing::info!(environment = %plan.environment, "diagnostics on");
    Guard(Some(guard))
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
