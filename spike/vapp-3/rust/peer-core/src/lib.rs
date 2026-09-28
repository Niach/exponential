//! VAPP-3 spike core: ONE Rust peer-link core (str0m data channels) for desktop/CLI/iOS/Android.
//!
//! Throwaway spike code. The public surface below is FROZEN for the spike (peer-ffi + peer-cli
//! consume it); the implementation behind it is Lane A's job (see spike/vapp-3/README.md).
//!
//! Wire shapes shared with the web page and the mobile hello apps live in [`signal`].

pub mod signal;
#[cfg(feature = "signing")]
pub mod signing;
pub mod link;
pub mod bench;
mod gather;
mod io;

pub use link::{BenchResult, IcePolicy, LinkState, PeerConfig, PeerError, PeerLink};

pub fn version() -> &'static str {
    concat!("peer-core ", env!("CARGO_PKG_VERSION"), " str0m 0.24 spike")
}

/// Install the str0m crypto provider selected by feature flags, once per process.
/// Safe to call repeatedly.
pub fn install_crypto() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        str0m::crypto::from_feature_flags().install_process_default();
    });
}
