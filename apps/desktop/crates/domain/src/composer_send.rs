//! EXP-1537: what a SEND leaves in THE composer ×4. The field owns its text:
//! it stays editable while a send is in flight, so a send that lands clears
//! the text only when the field still holds exactly what was sent, and takes
//! out only the attachments that went out; a failed send leaves the field as
//! it is now. Locked by `packages/domain-contract/fixtures/composer-send.json`
//! (its `$comment` holds the rules); mirrors TS `src/composer-send.ts`, iOS
//! `ComposerSend.swift`, Android `ComposerSend.kt`.

/// A composer's content: its text and its attachments' ids (a client's own
/// key per picked file).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerContent {
    pub text: String,
    pub attachments: Vec<String>,
}

/// How the send settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// The server took it.
    Sent,
    /// An upload or the send refused (a toast says so).
    Failed,
}

/// The composer once a send settles: `sent` = the field when Send was pressed
/// (untrimmed) and the attachments that went out, `current` = the field and
/// its attachments now.
pub fn after_send(outcome: SendOutcome, sent: &ComposerContent, current: &ComposerContent) -> ComposerContent {
    if outcome == SendOutcome::Failed {
        return current.clone();
    }
    ComposerContent {
        text: text_after_send(&sent.text, &current.text),
        attachments: current
            .attachments
            .iter()
            .filter(|id| !sent.attachments.contains(id))
            .cloned()
            .collect(),
    }
}

/// The text half alone (a composer without attachments).
pub fn text_after_send(sent: &str, current: &str) -> String {
    if current == sent {
        String::new()
    } else {
        current.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const COMPOSER_SEND_JSON: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/composer-send.json");

    #[derive(Deserialize)]
    struct Content {
        text: String,
        attachments: Vec<String>,
    }

    impl From<Content> for ComposerContent {
        fn from(content: Content) -> Self {
            Self { text: content.text, attachments: content.attachments }
        }
    }

    #[derive(Deserialize)]
    struct Input {
        outcome: String,
        sent: Content,
        current: Content,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: Input,
        expected: Content,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[test]
    fn replays_every_fixture_case() {
        let fixture: Fixture = serde_json::from_str(COMPOSER_SEND_JSON).expect("composer-send.json parses");
        assert!(!fixture.cases.is_empty());
        for case in fixture.cases {
            let outcome = match case.input.outcome.as_str() {
                "sent" => SendOutcome::Sent,
                "failed" => SendOutcome::Failed,
                other => panic!("{}: unknown outcome {other}", case.name),
            };
            let sent = ComposerContent::from(case.input.sent);
            let current = ComposerContent::from(case.input.current);
            let expected = ComposerContent::from(case.expected);
            assert_eq!(after_send(outcome, &sent, &current), expected, "{}", case.name);
            if sent.attachments.is_empty() && current.attachments.is_empty() && outcome == SendOutcome::Sent {
                assert_eq!(text_after_send(&sent.text, &current.text), expected.text, "{} (text)", case.name);
            }
        }
    }
}
