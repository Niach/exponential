import ExpCore
import ExpUI
import GRDB
import SwiftUI

// The team-member lookups the create and detail surfaces share.
//
// The `PickerSheet` this file was named for is gone (EXP-603): the stock
// `NavigationStack`+`List` sheet was the last non-glass picker in the app, and
// `GlassPickerSheet` (ExpUI/GlassSheet.swift) took its call sites over with the
// same generic signature. EXP-1021 then retired the `AssigneeOption` row model
// that lived here too: the shared `AssigneePicker` owns the "Unassigned"
// sentinel now, in one place for all four clients. The filename stays — Tuist
// globs it, and these lookups have no better home.

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
    // Display-name order, at the source (EXP-1021 review r2) — the shared
    // assignee picker renders the rows it is handed.
    return membersByDisplayName(
        try UserEntity.filter(memberIds.contains(Column("id"))).fetchAll(db)
    )
}
