//! EXP-630 placeholder members — web `lib/placeholder-status.ts`'s twin.
//!
//! An invite by email puts the invitee on the roster AT ONCE as a PLACEHOLDER
//! member (a credential-less `users` row + a `team_members` row), so work can
//! be assigned and attributed before they ever sign in; the invite carries
//! [`TeamInvite::placeholder_user_id`]. This module is the ONE rule a member
//! list reads off the synced invites: which roster rows are "invited, not
//! joined", and whether their link still works.
//!
//! `pending` = the link still works, `expired` = it lapsed or was superseded
//! (the roster row stays until removed; "Resend invite" mints a fresh link, at
//! a corrected address if need be).

use std::collections::HashMap;

use crate::rows::TeamInvite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderStatus {
    Pending,
    Expired,
}

impl PlaceholderStatus {
    /// Web `PLACEHOLDER_LABELS` — the badge text, byte-identical ×4.
    pub fn label(self) -> &'static str {
        match self {
            PlaceholderStatus::Pending => "Invited",
            PlaceholderStatus::Expired => "Invite expired",
        }
    }
}

/// Placeholder member id → its badge, for every invite bound to a placeholder
/// member that has NOT been accepted. `now_ms` = epoch millis (the web's
/// `now`); a pending link outranks an expired one for the same member, in
/// either input order.
pub fn placeholder_statuses(
    invites: &[TeamInvite],
    now_ms: i64,
) -> HashMap<String, PlaceholderStatus> {
    let mut statuses: HashMap<String, PlaceholderStatus> = HashMap::new();
    for invite in invites {
        let Some(user_id) = placeholder_of(invite) else {
            continue;
        };
        if is_accepted(invite) {
            continue;
        }
        let status = if is_live(invite, now_ms) {
            PlaceholderStatus::Pending
        } else {
            PlaceholderStatus::Expired
        };
        if statuses.get(user_id) == Some(&PlaceholderStatus::Pending) {
            continue;
        }
        statuses.insert(user_id.to_string(), status);
    }
    statuses
}

/// Web `InviteControls`: a PENDING invite is unaccepted AND unexpired — an
/// expired placeholder invite is a badge on the member row above, not a live
/// link in the list.
pub fn invite_is_pending(invite: &TeamInvite, now_ms: i64) -> bool {
    !is_accepted(invite) && is_live(invite, now_ms)
}

fn placeholder_of(invite: &TeamInvite) -> Option<&str> {
    invite
        .placeholder_user_id
        .as_deref()
        .filter(|id| !id.is_empty())
}

fn is_accepted(invite: &TeamInvite) -> bool {
    invite
        .accepted_at
        .as_deref()
        .is_some_and(|at| !at.is_empty())
}

/// `expires_at > now`. A missing or unparseable expiry reads EXPIRED — the
/// web's `new Date(null) > now` is false too (fail-closed: a link nobody can
/// date is not one to present as live).
fn is_live(invite: &TeamInvite, now_ms: i64) -> bool {
    invite
        .expires_at
        .as_deref()
        .and_then(crate::activity_fold::parse_instant)
        .is_some_and(|expires_ms| expires_ms > now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Twin of `apps/web/src/lib/placeholder-status.test.ts`.
    const NOW_MS: i64 = 1_790_244_000_000; // 2026-09-24T10:00:00Z
    const LATER: &str = "2026-10-01T10:00:00Z";
    const EARLIER: &str = "2026-09-20T10:00:00Z";

    fn invite(
        id: &str,
        placeholder: Option<&str>,
        accepted_at: Option<&str>,
        expires_at: &str,
    ) -> TeamInvite {
        serde_json::from_value(json!({
            "id": id,
            "team_id": "t-1",
            "placeholder_user_id": placeholder,
            "accepted_at": accepted_at,
            "expires_at": expires_at,
        }))
        .unwrap()
    }

    #[test]
    fn badges_a_pending_invite_an_expired_one_and_nothing_once_accepted() {
        let statuses = placeholder_statuses(
            &[
                invite("i-1", Some("p-pending"), None, LATER),
                invite("i-2", Some("p-expired"), None, EARLIER),
                invite("i-3", Some("p-joined"), Some(EARLIER), LATER),
                invite("i-4", None, None, LATER),
            ],
            NOW_MS,
        );
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses.get("p-pending"), Some(&PlaceholderStatus::Pending));
        assert_eq!(statuses.get("p-expired"), Some(&PlaceholderStatus::Expired));
        assert_eq!(statuses.get("p-joined"), None);
    }

    #[test]
    fn a_fresh_link_outranks_the_expired_one_it_superseded_in_either_order() {
        let rows = [
            invite("i-old", Some("p"), None, EARLIER),
            invite("i-new", Some("p"), None, LATER),
        ];
        assert_eq!(
            placeholder_statuses(&rows, NOW_MS).get("p"),
            Some(&PlaceholderStatus::Pending)
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(
            placeholder_statuses(&reversed, NOW_MS).get("p"),
            Some(&PlaceholderStatus::Pending)
        );
    }

    #[test]
    fn labels_both_states() {
        assert_eq!(PlaceholderStatus::Pending.label(), "Invited");
        assert_eq!(PlaceholderStatus::Expired.label(), "Invite expired");
    }

    /// Electric delivers Postgres timestamptz text (`… …+00`), not only
    /// RFC 3339 — both forms have to date the same way.
    #[test]
    fn reads_both_wire_timestamp_forms_and_fails_closed_on_neither() {
        let pg = invite("i-1", Some("p"), None, "2026-10-01 10:00:00+00");
        assert!(invite_is_pending(&pg, NOW_MS));
        let unparseable = invite("i-2", Some("p"), None, "soon");
        assert!(!invite_is_pending(&unparseable, NOW_MS));
        assert_eq!(
            placeholder_statuses(&[unparseable], NOW_MS).get("p"),
            Some(&PlaceholderStatus::Expired)
        );
    }

    /// The pending LIST is the complement of the badge: an accepted invite is
    /// gone from it, an expired one is a member-row badge instead.
    #[test]
    fn pending_means_unaccepted_and_unexpired() {
        assert!(invite_is_pending(&invite("i-1", None, None, LATER), NOW_MS));
        assert!(!invite_is_pending(
            &invite("i-2", None, None, EARLIER),
            NOW_MS
        ));
        assert!(!invite_is_pending(
            &invite("i-3", None, Some(EARLIER), LATER),
            NOW_MS
        ));
    }
}
