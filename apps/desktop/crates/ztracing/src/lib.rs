//! Inert replacement for Zed's `ztracing` profiling shim (EXP-222).
//!
//! The upstream crate is GPL-3.0-or-later — statically linking it is incompatible
//! with distributing the desktop binary under ESTL-1.0 — so the workspace `[patch]`
//! table swaps in this crate. It mirrors the API surface upstream exposes when the
//! `ztracing` cfg is OFF (the only configuration this workspace ever builds): a
//! pass-through `#[instrument]`, a unit `Span`, token-eating span/event macros, and
//! an inert `init`. Our graph only consumes `instrument` today (via `sum_tree`); the
//! rest is provided so gpui rev bumps don't require shim changes.

pub use tracing::{Level, field};
pub use ztracing_macro::instrument;

pub struct Span;

impl Span {
    pub fn current() -> Self {
        Span
    }

    pub fn enter(&self) {}

    pub fn record<F, V>(&self, _field: F, _value: V) {}
}

pub fn init() {}

/// Swallows its arguments and evaluates to a unit [`Span`].
#[macro_export]
macro_rules! __noop_span {
    ($($tokens:tt)*) => {
        $crate::Span
    };
}

pub use __noop_span as debug_span;
pub use __noop_span as error_span;
pub use __noop_span as event;
pub use __noop_span as info_span;
pub use __noop_span as span;
pub use __noop_span as trace_span;
pub use __noop_span as warn_span;
