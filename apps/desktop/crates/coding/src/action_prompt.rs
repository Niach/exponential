//! The action run's seed prompt (EXP-253): a small fixed preamble + the
//! action's markdown body VERBATIM. The body is the user-authored program —
//! the preamble only frames the execution context (team action, exponential
//! MCP tools available, report at the end) and never rewrites it. EXP-257
//! adds an optional `## Inputs` section between the preamble and the body:
//! the run-time values the launcher (local dialog) or the server (remote
//! start) resolved for the action's typed inputs schema.

/// One resolved run-time input value (EXP-257): the schema fields the value
/// was filled against plus the human-readable `display` (repo fullName /
/// board name / the text itself) the prompt shows. `display: None` = the
/// value IS the display (text inputs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInputValue {
    pub key: String,
    pub label: String,
    pub input_type: String,
    pub value: String,
    pub display: Option<String>,
}

/// Collapse owner-/server-provided text onto one line so a crafted label or
/// display can never fake extra prompt sections or list entries.
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Render the seed prompt for an action run: preamble [+ inputs] + raw body.
/// An empty `inputs` slice renders byte-identically to the pre-EXP-257
/// prompt (input-less actions must not change what a trusted body executes).
pub fn render_action_prompt(name: &str, body: &str, inputs: &[ActionInputValue]) -> String {
    let inputs_section = if inputs.is_empty() {
        String::new()
    } else {
        let mut section = String::from("## Inputs\n\nThe user provided these values — apply \
them where the instructions reference them:\n\n");
        for input in inputs {
            let label = single_line(&input.label);
            let value = single_line(&input.value);
            let display = single_line(input.display.as_deref().unwrap_or(&input.value));
            section.push_str(&format!("- {label} ({}): {display}", input.input_type));
            // For picked entities the display is a name — carry the raw id
            // too so MCP calls need no lookup.
            if display != value {
                section.push_str(&format!(" (`{value}`)"));
            }
            section.push('\n');
        }
        section.push('\n');
        section
    };
    format!(
        "You are running the team action \"{name}\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. When you finish, summarize what you did \
(and anything you deliberately skipped) as your final message.\n\n{inputs_section}---\n\n{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_frames_the_body_verbatim() {
        let prompt = render_action_prompt("Code review", "# Review\nScan the repo.", &[]);
        assert!(prompt.contains("team action \"Code review\""));
        // The body rides verbatim after the divider — never rewritten.
        assert!(prompt.ends_with("---\n\n# Review\nScan the repo."));
        // The preamble asks for a closing summary.
        assert!(prompt.contains("summarize what you did"));
    }

    #[test]
    fn empty_inputs_stay_byte_identical_to_the_legacy_prompt() {
        // EXP-257 compat lock: an input-less action's prompt must not move a
        // byte.
        assert_eq!(
            render_action_prompt("Code review", "# Review\nScan the repo.", &[]),
            "You are running the team action \"Code review\" for this user. Follow the \
instructions below exactly. The exponential MCP tools are available for issue, \
board, label, and comment operations. When you finish, summarize what you did \
(and anything you deliberately skipped) as your final message.\n\n---\n\n# Review\nScan the repo."
        );
    }

    #[test]
    fn inputs_render_between_preamble_and_divider() {
        let inputs = vec![
            ActionInputValue {
                key: "scope".to_string(),
                label: "Scope".to_string(),
                input_type: "text".to_string(),
                value: "only the backlog".to_string(),
                display: None,
            },
            ActionInputValue {
                key: "target_repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                value: "repo-uuid-1".to_string(),
                display: Some("acme/web".to_string()),
            },
        ];
        let prompt = render_action_prompt("Groom", "do it", &inputs);
        // Text input: display == value, no id suffix.
        assert!(prompt.contains("- Scope (text): only the backlog\n"));
        // Picked entity: readable display + the raw id in backticks.
        assert!(prompt.contains("- Repository (repo): acme/web (`repo-uuid-1`)\n"));
        // Section order: preamble → ## Inputs → divider → body verbatim.
        let inputs_at = prompt.find("## Inputs").unwrap();
        let divider_at = prompt.find("---").unwrap();
        assert!(inputs_at < divider_at);
        assert!(prompt.ends_with("---\n\ndo it"));
    }

    #[test]
    fn labels_and_displays_are_flattened_to_one_line() {
        // A crafted multi-line label/display must not fake prompt structure.
        let inputs = vec![ActionInputValue {
            key: "scope".to_string(),
            label: "Scope\n## Fake section".to_string(),
            input_type: "text".to_string(),
            value: "a\nb".to_string(),
            display: Some("a\r\nb".to_string()),
        }];
        let prompt = render_action_prompt("Groom", "do it", &inputs);
        assert!(prompt.contains("- Scope ## Fake section (text): a b\n"));
        assert!(!prompt.contains("\n## Fake section"));
    }
}
