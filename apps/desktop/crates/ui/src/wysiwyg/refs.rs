//! EXP-261: host side of the vendored editor's reference pills — a Send+Sync
//! snapshot of the team's member emails and issue identifiers, scanned with
//! the same token contract the block editor uses (`mentions.ts` /
//! `issue-refs.ts` parity). Only RESOLVED tokens decorate; unknown
//! identifiers stay plain text (cross-client contract).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui::{App, Hsla, SharedString};
use gpui_markdown_editor::{ChipIcon, ReferenceDecorator, ReferenceKind, ReferenceSpan};
use sync::Store;

use crate::markdown::{scan_issue_refs, scan_mentions, truncate_chip_title};

/// EXP-423: everything a resolved `#IDENT` chip renders — the truncated
/// title plus the pre-resolved status icon (SVG asset path + tint).
#[derive(Clone, Debug)]
pub(crate) struct IssueChipSnapshot {
    /// Already `truncate_chip_title`'d.
    pub title: SharedString,
    pub icon_path: SharedString,
    pub icon_color: Hsla,
}

/// Snapshot the wrapper refreshes on the UI thread (init + every change).
#[derive(Default)]
pub(crate) struct SharedRefState {
    /// Lowercased member emails of the scoped team.
    pub member_emails: Mutex<HashSet<String>>,
    /// EXP-322/423: uppercased issue identifier → chip snapshot, so a
    /// resolved `#IDENT` renders `[status icon] #IDENT <title>` while
    /// editing like the web editor.
    pub issues: Mutex<HashMap<String, IssueChipSnapshot>>,
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
                        icon: None,
                    });
                }
            }
        }
        if let Ok(issues) = self.state.issues.lock() {
            for range in scan_issue_refs(text) {
                let identifier = text[range.start + 1..range.end].to_uppercase();
                if let Some(snapshot) = issues.get(&identifier) {
                    spans.push(ReferenceSpan {
                        range,
                        kind: ReferenceKind::IssueRef,
                        display_suffix: (!snapshot.title.is_empty())
                            .then(|| snapshot.title.clone()),
                        icon: Some(ChipIcon {
                            svg_path: snapshot.icon_path.clone(),
                            color: snapshot.icon_color,
                        }),
                    });
                }
            }
        }
        spans.sort_by_key(|span| span.range.start);
        spans
    }
}

/// Refresh the snapshot from the synced store (team-scoped). Cheap: two
/// collection scans over already-synced rows (status resolution is a
/// per-issue read against the team's cached status rows).
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

    let issues: HashMap<String, IssueChipSnapshot> = collections
        .issues_in_team(team_id, cx)
        .iter()
        .map(|issue| {
            // EXP-423: resolve status → glyph + tint once per refresh, so
            // the Send+Sync decorator never touches the store.
            let status = crate::queries::resolve_issue_status(cx, issue);
            (
                issue.identifier.to_uppercase(),
                IssueChipSnapshot {
                    title: SharedString::from(truncate_chip_title(&issue.title)),
                    icon_path: crate::icons::glyph_svg_path(status.glyph),
                    icon_color: crate::icons::status_tint_color(&status.tint, cx),
                },
            )
        })
        .collect();
    if let Ok(mut slot) = state.issues.lock() {
        *slot = issues;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decorator(members: &[&str], issues: &[(&str, &str)]) -> WysiwygReferenceDecorator {
        let state = SharedRefState::default();
        *state.member_emails.lock().unwrap() =
            members.iter().map(|email| email.to_lowercase()).collect();
        *state.issues.lock().unwrap() = issues
            .iter()
            .map(|(id, title)| {
                (
                    id.to_uppercase(),
                    IssueChipSnapshot {
                        title: SharedString::from(truncate_chip_title(title)),
                        icon_path: SharedString::from("icons/circle.svg"),
                        icon_color: Hsla::default(),
                    },
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

    /// EXP-423: a resolved issue span carries the snapshot's status icon.
    #[test]
    fn a_resolved_issue_ref_carries_its_status_icon() {
        let spans = decorator(&[], &[("EXP-42", "Fix login flow")]).scan("Fixes #EXP-42 today");
        let icon = spans[0].icon.as_ref().expect("status icon");
        assert_eq!(icon.svg_path.as_ref(), "icons/circle.svg");
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
    /// EXP-423: they carry no icon either.
    #[test]
    fn a_resolved_mention_carries_no_display_suffix_and_no_icon() {
        let spans = decorator(&["ada@example.com"], &[]).scan("Ping @ada@example.com now");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, ReferenceKind::Mention);
        assert_eq!(spans[0].display_suffix, None);
        assert!(spans[0].icon.is_none());
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
    fn a_blank_title_leaves_the_bare_token_but_keeps_the_icon() {
        let spans = decorator(&[], &[("EXP-1", "   ")]).scan("#EXP-1");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].display_suffix, None);
        assert!(spans[0].icon.is_some());
    }
}
