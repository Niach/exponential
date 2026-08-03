//! Client-version header (EXP-104) — the desktop tags every request to the
//! instance server with `x-client-version: desktop/<version>` so the server
//! can gate stale builds behind an HTTP 426 min-version response. Shared here
//! because both `api` (auth + tRPC) and `sync` (shape long-polls) send it, and
//! both already depend on `domain`.

/// The header every request to the instance server carries.
pub const CLIENT_VERSION_HEADER: &str = "x-client-version";

/// EXP-403: the `exponential` CLI shares `api`/`sync` with the desktop but
/// must identify as its OWN platform (`cli/<version>`, its own
/// `CLIENT_MIN_VERSION_CLI` gate). Set ONCE at process start (before any
/// request) by the CLI's main; the desktop never calls this and keeps the
/// `desktop/<version>` default.
static IDENTITY_OVERRIDE: std::sync::OnceLock<(String, String)> = std::sync::OnceLock::new();

/// Override the `<platform>/<version>` identity this process sends. Later
/// calls are ignored (first write wins).
pub fn set_client_identity(platform: &str, version: &str) {
    let _ = IDENTITY_OVERRIDE.set((platform.to_string(), version.to_string()));
}

/// The version this binary was compiled at. Release CI injects the real tag
/// version via `EXP_DESKTOP_VERSION`; the `CARGO_PKG_VERSION` fallback
/// resolves to the shared team version (every crate inherits
/// `version.team = true`, so it reads the same everywhere this compiles
/// into). Shared by the header below, the update check (`ui::update`) and the
/// About screen (EXP-262).
pub fn current_version() -> &'static str {
    option_env!("EXP_DESKTOP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The header value: `desktop/<compiled version>` — or the process identity
/// installed via [`set_client_identity`] (the CLI's `cli/<version>`).
pub fn client_version_header_value() -> String {
    match IDENTITY_OVERRIDE.get() {
        Some((platform, version)) => format!("{platform}/{version}"),
        None => format!("desktop/{}", current_version()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_value_is_desktop_prefixed() {
        let value = client_version_header_value();
        assert!(value.starts_with("desktop/"), "unexpected: {value}");
        // A version, not an empty tag.
        assert!(value.len() > "desktop/".len());
    }
}
