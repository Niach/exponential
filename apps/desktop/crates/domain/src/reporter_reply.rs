//! SLOP-4 — a widget submission IS an issue; the conversation with its
//! reporter is COMMENTS. This is the copy set for reporter comments and the
//! "Reply to reporter" composer toggle, mirrored ×4 (web imports the json,
//! iOS `ReporterReply`, Android `ReporterReply`) and locked by the contract
//! fixture `packages/domain-contract/fixtures/reporter-reply.json`.
//!
//! The rules the fixture pins: a comment with source `reporter` has no
//! author — the card names the submission's reporter (else
//! [`ANONYMOUS_NAME`]) with [`REPORTER_CAPTION`] where "via MCP" sits and an
//! initials avatar; a member comment with audience `reporter` carries
//! [`TO_REPORTER_CAPTION`] in that slot; a missing author row with any other
//! source reads [`FORMER_MEMBER_NAME`]. The [`TOGGLE_LABEL`] pill sits in the
//! issue composer's leading row ONLY while the submission has a reporter
//! email, OFF by default, never on a reply composer; while ON the
//! placeholder is [`placeholder_on`] and the send carries `audience:
//! reporter`. The create result's `reporterEmailed` picks [`SENT_TOAST`]
//! (true) / [`NOT_SENT_TOAST`] (false); `null` = a team comment, no toast.

/// The composer pill.
pub const TOGGLE_LABEL: &str = "Reply to reporter";
/// The composer placeholder while the pill is ON, `{name}` = the reporter.
const PLACEHOLDER_ON: &str = "Reply to {name}… (emailed to them)";
/// A submission without a reporter name.
pub const ANONYMOUS_NAME: &str = "Anonymous visitor";
/// A non-reporter comment whose author row is gone.
pub const FORMER_MEMBER_NAME: &str = "Former member";
/// The caption after the time on a reporter's comment.
pub const REPORTER_CAPTION: &str = "reporter";
/// The caption after the time on a member comment emailed to the reporter.
pub const TO_REPORTER_CAPTION: &str = "to reporter";
/// `reporterEmailed: true`.
pub const SENT_TOAST: &str = "Reply emailed to the reporter.";
/// `reporterEmailed: false`.
pub const NOT_SENT_TOAST: &str =
    "Reply saved. No email was sent: this server has no mail transport.";
/// The inbox row label of a `reporter_reply` notification.
pub const NOTIFICATION_LABEL: &str = "Reporter replied";

/// [`PLACEHOLDER_ON`] with the reporter's name filled in.
pub fn placeholder_on(name: &str) -> String {
    PLACEHOLDER_ON.replace("{name}", name)
}

/// The reporter's display name: the submission's `reporterName` when it has
/// one, else [`ANONYMOUS_NAME`].
pub fn reporter_display_name(reporter_name: Option<&str>) -> &str {
    reporter_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(ANONYMOUS_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/reporter-reply.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn reporter_reply_copy_matches_the_fixture() {
        let fixture = fixture();
        let copy = &fixture["copy"];
        assert_eq!(copy["toggleLabel"].as_str(), Some(TOGGLE_LABEL));
        assert_eq!(copy["placeholderOn"].as_str(), Some(PLACEHOLDER_ON));
        assert_eq!(copy["anonymousName"].as_str(), Some(ANONYMOUS_NAME));
        assert_eq!(copy["formerMemberName"].as_str(), Some(FORMER_MEMBER_NAME));
        assert_eq!(copy["reporterCaption"].as_str(), Some(REPORTER_CAPTION));
        assert_eq!(copy["toReporterCaption"].as_str(), Some(TO_REPORTER_CAPTION));
        assert_eq!(copy["sentToast"].as_str(), Some(SENT_TOAST));
        assert_eq!(copy["notSentToast"].as_str(), Some(NOT_SENT_TOAST));
        assert_eq!(copy["notificationLabel"].as_str(), Some(NOTIFICATION_LABEL));
        assert_eq!(copy.as_object().unwrap().len(), 9, "a new string needs a test");
    }

    #[test]
    fn placeholder_fills_the_name_and_falls_back_to_anonymous() {
        assert_eq!(placeholder_on("Ada"), "Reply to Ada… (emailed to them)");
        assert_eq!(reporter_display_name(Some("Ada")), "Ada");
        assert_eq!(reporter_display_name(Some("  ")), ANONYMOUS_NAME);
        assert_eq!(reporter_display_name(None), ANONYMOUS_NAME);
        assert_eq!(
            placeholder_on(reporter_display_name(None)),
            "Reply to Anonymous visitor… (emailed to them)"
        );
    }
}
