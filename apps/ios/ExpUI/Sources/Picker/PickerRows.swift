import ExpCore
import SwiftUI

// EXP-1021 — the ONE bridge from a synced row to a typed picker's row model.
//
// The typed pickers (EXP-1029) own their own row structs so the contract is
// the same on all four clients; these initialisers are the iOS seam between
// them and the app's entities. They live here rather than at the call sites
// because a picker that reads its board glyph one way on the issue sheet and
// another way in Support is exactly the drift EXP-1021 ends: the board's
// glyph, the status's resolved colour and the member's display name are
// decided ONCE, here.

public extension BoardPickerBoard {
    /// A board row: its curated glyph (else the shape fallback) in its colour.
    init(_ board: BoardEntity) {
        self.init(
            id: board.id,
            name: board.name,
            icon: BoardTypeDisplay.iconName(for: board),
            colorHex: board.color
        )
    }
}

public extension StatusPickerStatus {
    /// A status row (EXP-314): the resolved glyph in the resolved colour —
    /// a builtin's design token, a custom row's hex.
    init(_ status: ResolvedIssueStatus) {
        self.init(
            id: status.id,
            name: status.name,
            category: status.category.rawValue,
            icon: status.iconName,
            color: status.color
        )
    }
}

public extension PriorityPickerOption {
    /// A priority row, in the contract's own display order (the caller maps
    /// `IssuePriority.displayOrder`).
    init(_ priority: IssuePriority) {
        self.init(
            value: priority.id,
            label: priority.label,
            icon: priority.iconName,
            color: priority.color
        )
    }
}

public extension LabelPickerLabel {
    /// A label row: name + its own colour, which the picker draws as the dot.
    init(_ label: LabelEntity) {
        self.init(id: label.id, name: label.name, colorHex: label.color)
    }
}

public extension AssigneePickerMember {
    /// A member row. The display name falls back to the email (and to the
    /// pseudonym for an unsynced row) exactly as it does everywhere else.
    init(_ user: UserEntity) {
        self.init(
            id: user.id,
            name: memberDisplayName(user, id: user.id),
            email: user.email,
            image: user.image
        )
    }
}

public extension IssuePickerIssue {
    /// An issue row: the STATUS glyph in its colour, then `IDENT Title`,
    /// matched on either half. A row whose identifier has not been stamped
    /// yet reads as its title alone. The glyph comes off the builtin ANCHOR
    /// (`issues.status`) — the same resolution the linker's rows have always
    /// used, and the one every list can do without the team's status rows.
    init(_ issue: IssueEntity) {
        let status = IssueStatus.from(issue.status)
        self.init(
            id: issue.id,
            identifier: issue.identifier ?? "",
            title: issue.title,
            icon: status.iconName,
            color: status.color
        )
    }

    /// The same row against the issue's RESOLVED team status (EXP-314), for a
    /// caller that has already resolved it — a custom status keeps its own
    /// name's glyph and colour rather than its anchor's.
    init(_ issue: IssueEntity, status: ResolvedIssueStatus) {
        self.init(
            id: issue.id,
            identifier: issue.identifier ?? "",
            title: issue.title,
            icon: status.iconName,
            color: status.color
        )
    }
}
