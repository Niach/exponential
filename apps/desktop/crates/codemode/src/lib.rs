//! EXP-1507: code mode moved into eMCP (`emcp::codemode`). This crate keeps
//! the launcher's paths (`codemode::grant`, `codemode::worker`, …) and hands
//! emcp the contract's `expToolDisplay` rows (`domain`, an app crate emcp
//! never links), so the call log's subjects and composed names stay
//! Exponential's.

use std::sync::Arc;

pub use emcp::codemode::*;

/// The contract's tool rows, installed before the first run is admitted.
fn install_subjects() {
    use domain::contract::{EXP_TOOL_NAMES, EXP_TOOL_PREFIX, EXP_TOOL_SUBJECT_KEYS};
    emcp::codemode::set_subject_table(SubjectTable {
        prefix: EXP_TOOL_PREFIX,
        names: EXP_TOOL_NAMES,
        keys: EXP_TOOL_SUBJECT_KEYS,
    });
}

/// [`emcp::codemode::grant`], the contract's rows installed.
pub fn grant(session_id: &str, label: &str, upstreams: Vec<Upstream>, direct_only: Vec<String>) -> Result<Grant, String> {
    install_subjects();
    emcp::codemode::grant(session_id, label, upstreams, direct_only)
}

/// [`emcp::codemode::gateway_grant`], the contract's rows installed.
pub fn gateway_grant(
    session_id: &str,
    label: &str,
    upstream: Upstream,
    transform: Arc<dyn GatewayTransform>,
) -> Result<Grant, String> {
    install_subjects();
    emcp::codemode::gateway_grant(session_id, label, upstream, transform)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_contract_rows_name_exponential_tools_subjects() {
        super::install_subjects();
        let args = serde_json::json!({ "op": "get", "id": " EXP-12 " });
        assert_eq!(super::display_tool_name("exponential_issues", &args), "exponential_issues_get");
    }
}
