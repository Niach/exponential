//! EXP-261: host side of the vendored editor's reference pills — a Send+Sync
//! snapshot of the team's member emails and issue identifiers, scanned with
//! the same token contract the block editor uses (`mentions.ts` /
//! `issue-refs.ts` parity). Only RESOLVED tokens decorate; unknown
//! identifiers stay plain text (cross-client contract).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::{App, SharedString};
use gpui_markdown_editor::{ReferenceDecorator, ReferenceKind, ReferenceSpan};
use sync::Store;

use crate::markdown::{scan_issue_refs, scan_mentions, truncate_chip_title};

/// Snapshot the wrapper refreshes on the UI thread (init + every change).
#[derive(Default)]
pub(crate) struct SharedRefState {
    /// Lowercased member emails of the scoped team.
    pub member_emails: Mutex<HashSet<String>>,
    /// EXP-322: uppercased issue identifier → chip title, so a resolved
    /// `#IDENT` renders `#IDENT <title>` while editing like the web editor.
    pub issue_titles: Mutex<HashMap<String, SharedString>>,
}

pub(crate) struct WysiwygReferenceDecorator {
    pub state: Arc<SharedRefState>,
}

impl ReferenceDecorator for WysiwygReferenceDecorator {
    fn scan(&self, text: &str) -> Vec<ReferenceSpan> {
        let mut spans = Vec::new();
        if let Ok(members) = self.state.member_emails.lock() {
            for range in scan_mentions(text) {
                let email = text[range.start + 1..range.end].to_lowercase();
                if members.contains(&email) {
                    spans.push(ReferenceSpan {
                        range,
                        kind: ReferenceKind::Mention,
                        // Mentions keep their raw `@email` while editing (web
                        // parity — substituting the name under an active caret
                        // makes editing hazardous).
                        display_suffix: None,
                    });
                }
            }
        }
        if let Ok(issues) = self.state.issue_titles.lock() {
            for range in scan_issue_refs(text) {
                let identifier = text[range.start + 1..range.end].to_uppercase();
                if let Some(title) = issues.get(&identifier) {
                    spans.push(ReferenceSpan {
                        range,
                        kind: ReferenceKind::IssueRef,
                        display_suffix: (!title.is_empty()).then(|| title.clone()),
                    });
                }
            }
        }
        spans.sort_by_key(|span| span.range.start);
        spans
    }
}

/// Refresh the snapshot from the synced store (team-scoped). Cheap: two
/// collection scans over already-synced rows.
pub(crate) fn refresh_ref_state(state: &SharedRefState, team_id: &str, cx: &App) {
    let collections = Store::global(cx).collections();

    let members = collections.team_members.read(cx);
    let users = collections.users.read(cx);
    let emails: HashSet<String> = members
        .iter()
        .filter(|member| member.team_id == team_id)
        .filter_map(|member| users.get(&member.user_id))
        .filter_map(|user| user.email.as_ref().map(|email| email.to_lowercase()))
        .collect();
    if let Ok(mut slot) = state.member_emails.lock() {
        *slot = emails;
    }

    let titles: HashMap<String, SharedString> = collections
        .issues_in_team(team_id, cx)
        .iter()
        .map(|issue| {
            (
                issue.identifier.to_uppercase(),
                SharedString::from(truncate_chip_title(&issue.title)),
            )
        })
        .collect();
    if let Ok(mut slot) = state.issue_titles.lock() {
        *slot = titles;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decorator(members: &[&str], issues: &[(&str, &str)]) -> WysiwygReferenceDecorator {
        let state = SharedRefState::default();
        *state.member_emails.lock().unwrap() =
            members.iter().map(|email| email.to_lowercase()).collect();
        *state.issue_titles.lock().unwrap() = issues
            .iter()
            .map(|(id, title)| {
                (
                    id.to_uppercase(),
                    SharedString::from(truncate_chip_title(title)),
                )
            })
            .collect();
        WysiwygReferenceDecorator {
            state: Arc::new(state),
        }
    }

    #[test]
    fn a_resolved_issue_ref_carries_its_title_as_a_display_suffix() {
        let spans = decorator(&[], &[("EXP-42", "Fix login flow")]).scan("Fixes #EXP-42 today");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, ReferenceKind::IssueRef);
        assert_eq!(
            spans[0].display_suffix.as_deref(),
            Some("Fix login flow"),
            "the chip should read `#EXP-42 Fix login flow`"
        );
    }

    #[test]
    fn an_unresolved_identifier_stays_plain_text() {
        assert!(
            decorator(&[], &[("EXP-42", "Fix login flow")])
                .scan("Fixes #EXP-99 today")
                .is_empty()
        );
    }

    /// Mentions keep their raw `@email` while editing — web parity, because
    /// substituting the name under an active caret makes editing hazardous.
    #[test]
    fn a_resolved_mention_carries_no_display_suffix() {
        let spans = decorator(&["ada@example.com"], &[]).scan("Ping @ada@example.com now");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, ReferenceKind::Mention);
        assert_eq!(spans[0].display_suffix, None);
    }

    /// The 60-char cap is the cross-client chip contract (web's `chipTitle`).
    #[test]
    fn a_long_title_is_truncated_like_the_web_chip() {
        let long = "x".repeat(100);
        let spans = decorator(&[], &[("EXP-1", &long)]).scan("#EXP-1");
        let suffix = spans[0].display_suffix.as_deref().expect("title");
        assert_eq!(suffix.chars().count(), 60);
        assert!(suffix.ends_with('…'));
    }

    #[test]
    fn a_blank_title_leaves_the_bare_token() {
        let spans = decorator(&[], &[("EXP-1", "   ")]).scan("#EXP-1");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].display_suffix, None);
    }
}
