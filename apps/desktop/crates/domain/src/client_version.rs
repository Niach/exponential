//! Client-version header (EXP-104) — the desktop tags every request to the
//! instance server with `x-client-version: desktop/<version>` so the server
//! can gate stale builds behind an HTTP 426 min-version response. Shared here
//! because both `api` (auth + tRPC) and `sync` (shape long-polls) send it, and
//! both already depend on `domain`.

/// The header every request to the instance server carries.
pub const CLIENT_VERSION_HEADER: &str = "x-client-version";

/// The version this binary was compiled at. Release CI injects the real tag
/// version via `EXP_DESKTOP_VERSION`; the `CARGO_PKG_VERSION` fallback
/// resolves to the shared team version (every crate inherits
/// `version.team = true`, so it reads the same everywhere this compiles
/// into). Shared by the header below, the update check (`ui::update`) and the
/// About screen (EXP-262).
pub fn current_version() -> &'static str {
    option_env!("EXP_DESKTOP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The header value: `desktop/<compiled version>`.
pub fn client_version_header_value() -> String {
    format!("desktop/{}", current_version())
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
