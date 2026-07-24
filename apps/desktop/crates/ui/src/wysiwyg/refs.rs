//! EXP-261: host side of the vendored editor's reference pills — a Send+Sync
//! snapshot of the team's member emails and issue identifiers, scanned with
//! the same token contract the block editor uses (`mentions.ts` /
//! `issue-refs.ts` parity). Only RESOLVED tokens decorate; unknown
//! identifiers stay plain text (cross-client contract).

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use gpui::App;
use gpui_markdown_editor::{ReferenceDecorator, ReferenceKind, ReferenceSpan};
use sync::Store;

use crate::markdown::{scan_issue_refs, scan_mentions};

/// Snapshot the wrapper refreshes on the UI thread (init + every change).
#[derive(Default)]
pub(crate) struct SharedRefState {
    /// Lowercased member emails of the scoped team.
    pub member_emails: Mutex<HashSet<String>>,
    /// Uppercased issue identifiers of the scoped team.
    pub issue_identifiers: Mutex<HashSet<String>>,
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
                    });
                }
            }
        }
        if let Ok(issues) = self.state.issue_identifiers.lock() {
            for range in scan_issue_refs(text) {
                let identifier = text[range.start + 1..range.end].to_uppercase();
                if issues.contains(&identifier) {
                    spans.push(ReferenceSpan {
                        range,
                        kind: ReferenceKind::IssueRef,
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

    let identifiers: HashSet<String> = collections
        .issues_in_team(team_id, cx)
        .iter()
        .map(|issue| issue.identifier.to_uppercase())
        .collect();
    if let Ok(mut slot) = state.issue_identifiers.lock() {
        *slot = identifiers;
    }
}
