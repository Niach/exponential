import ExpCore
import ExpUI
import GRDB
import SwiftUI

// The assignee-picker vocabulary and the team-member lookups the create and
// detail surfaces share.
//
// The `PickerSheet` this file was named for is gone (EXP-603): the stock
// `NavigationStack`+`List` sheet was the last non-glass picker in the app, and
// `GlassPickerSheet` (ExpUI/GlassSheet.swift) took its call sites over with the
// same generic signature. The filename stays — Tuist globs it, and these
// helpers have no better home.

// MARK: - Assignee picker helper

/// Row model for the assignee picker. Wraps an optional user so we
/// can render the "Unassigned" sentinel as a first-class option with a
/// stable identifier (`"__unassigned"`).
struct AssigneeOption: Identifiable, Hashable {
    let id: String
    let userId: String?
    let displayName: String

    static let unassigned = AssigneeOption(
        id: "__unassigned",
        userId: nil,
        displayName: "Unassigned"
    )
}

/// User ids of a team's members, from the synced `team_members` store.
///
/// When this returns exactly one id the team is solo: both create + detail
/// surfaces skip the assignee picker and auto-assign that sole member (EXP-50).
func humanTeamMemberIds(teamId: String, db: Database) throws -> [String] {
    let members = try TeamMemberEntity
        .filter(Column("team_id") == teamId)
        .fetchAll(db)
    return members.map(\.userId)
}

/// The team's member users — the assignee-picker / @-mention vocabulary.
/// The users store is account-wide (cross-team author display), so scoping
/// happens here via the synced team_members rows (EXP-487).
func teamMemberUsers(teamId: String, db: Database) throws -> [UserEntity] {
    let memberIds = try humanTeamMemberIds(teamId: teamId, db: db)
    return try UserEntity.filter(memberIds.contains(Column("id"))).fetchAll(db)
}

// Assignable members.
func assigneeOptions(users: [UserEntity]) -> [AssigneeOption] {
    var options: [AssigneeOption] = [.unassigned]
    for user in users {
        options.append(
            AssigneeOption(
                id: user.id,
                userId: user.id,
                // memberDisplayName falls back to the email for a blank name
                // (name-less Apple logins), so the option never renders empty.
                displayName: memberDisplayName(user, id: user.id)
            )
        )
    }
    return options
}
