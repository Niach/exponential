//! The action run's seed prompt (EXP-253): a small fixed preamble + the
//! action's markdown body VERBATIM. The body is the user-authored program —
//! the preamble only frames the execution context (team action, exponential
//! MCP tools available, report at the end) and never rewrites it.

/// Render the seed prompt for an action run: preamble + raw body.
pub fn render_action_prompt(name: &str, body: &str) -> String {
    format!(
        "You are running the team action \"{name}\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. When you finish, summarize what you did \
(and anything you deliberately skipped) as your final message.\n\n---\n\n{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_frames_the_body_verbatim() {
        let prompt = render_action_prompt("Code review", "# Review\nScan the repo.");
        assert!(prompt.contains("team action \"Code review\""));
        // The body rides verbatim after the divider — never rewritten.
        assert!(prompt.ends_with("---\n\n# Review\nScan the repo."));
        // The preamble asks for a closing summary.
        assert!(prompt.contains("summarize what you did"));
    }
}
