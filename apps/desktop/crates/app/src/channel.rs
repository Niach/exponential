//! Build channel (production vs staging) — the desktop analog of iOS
//! `AppConstants.isStaging`. Selected at compile time by the `staging` cargo
//! feature so a staging build points at `next.exponential.at` (via
//! `ui`'s matching feature) and installs a DISTINCT `.desktop` app id, letting
//! a staging and a production build coexist on one machine.
//!
//! The `exp://` URL scheme is shared by both channels (the server always
//! deep-links to `exp://oauth-return`), so whichever channel was launched most
//! recently claims the handler (`desktop_integration` re-asserts the default
//! each launch).

/// Human-readable app name (window title / `.desktop` `Name=`).
#[cfg(not(feature = "staging"))]
pub const APP_NAME: &str = "Exponential";
#[cfg(feature = "staging")]
pub const APP_NAME: &str = "Exponential (staging)";

/// Reverse-DNS app id — the `.desktop` file basename. Distinct per channel so
/// both can be installed at once.
#[cfg(not(feature = "staging"))]
pub const APP_ID: &str = "at.exponential";
#[cfg(feature = "staging")]
pub const APP_ID: &str = "at.exponential.staging";
