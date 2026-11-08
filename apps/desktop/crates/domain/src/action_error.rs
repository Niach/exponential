//! The failed action's toast words (`feedback-states.json` `errors`): the
//! title `Couldn't <verb> <thing>` and the description, the server's message
//! when it reads like a sentence (web `actionErrorDescription`). The IDE and
//! Exponential Desktop's shell toast the same words.

/// `errors.titlePattern`.
pub const TITLE_PATTERN: &str = "Couldn't {verb} {thing}";

/// `Couldn't <verb> <thing>`, the failed action's toast title.
pub fn title(verb: &str, thing: &str) -> String {
    TITLE_PATTERN.replace("{verb}", verb).replace("{thing}", thing)
}

/// The toast's description for a message: trimmed, `None` when empty or a
/// JSON or list dump.
pub fn description_of(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() || trimmed.starts_with('[') || trimmed.starts_with('{') {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_follows_the_fixture_s_pattern() {
        let fx: serde_json::Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/feedback-states.json")).unwrap();
        assert_eq!(fx["errors"]["titlePattern"], TITLE_PATTERN);
        assert_eq!(title("send", "the message"), "Couldn't send the message");
    }

    #[test]
    fn a_description_is_a_sentence_never_a_dump() {
        assert_eq!(description_of("  The session ended. "), Some("The session ended.".into()));
        assert_eq!(description_of("{\"code\":1}"), None);
        assert_eq!(description_of("[1]"), None);
        assert_eq!(description_of("  "), None);
    }
}
