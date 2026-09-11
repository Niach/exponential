import Foundation
import GRDB

/// EXP-778: one pinned row, RESOLVED against the local store. The `pins`
/// shape is per-user and never team/trash-scoped, so a row may name an issue
/// on a trashed board, a session that has not synced yet or an action of a
/// team the user has since left — `PinQueries.resolved` drops those, and a
/// pin renders ONLY when its target row is here.
public enum PinnedItem: Identifiable, Sendable {
    case issue(pin: PinEntity, issue: IssueEntity)
    /// The session's issue rides along for the row's identifier/title/PR
    /// state (nil for a batch or action run).
    case session(pin: PinEntity, session: CodingSessionEntity, issue: IssueEntity?)
    case action(pin: PinEntity, action: ActionEntity)

    public var pin: PinEntity {
        switch self {
        case let .issue(pin, _), let .session(pin, _, _), let .action(pin, _): pin
        }
    }

    public var id: String { pin.id }
}

public enum PinQueries {
    /// The active team's pins in display order (`sort_order` ascending,
    /// `created_at` as the tiebreak), each joined to its target; rows whose
    /// target is missing locally are omitted.
    public static func resolved(db: Database, teamId: String) throws -> [PinnedItem] {
        let pins = try PinEntity
            .filter(Column("team_id") == teamId)
            .order(Column("sort_order").asc, Column("created_at").asc)
            .fetchAll(db)
        var items: [PinnedItem] = []
        items.reserveCapacity(pins.count)
        for pin in pins {
            switch pin.kind {
            case DomainContract.pinKindIssue:
                guard let issueId = pin.issueId,
                      let issue = try IssueEntity.fetchOne(db, key: issueId) else { continue }
                items.append(.issue(pin: pin, issue: issue))
            case DomainContract.pinKindSession:
                guard let sessionId = pin.sessionId,
                      let session = try CodingSessionEntity.fetchOne(db, key: sessionId) else { continue }
                let issue = try session.issueId.flatMap { try IssueEntity.fetchOne(db, key: $0) }
                items.append(.session(pin: pin, session: session, issue: issue))
            case DomainContract.pinKindAction:
                guard let actionId = pin.actionId,
                      let action = try ActionEntity.fetchOne(db, key: actionId) else { continue }
                items.append(.action(pin: pin, action: action))
            default:
                // An unknown kind from a newer server: hide, never crash.
                continue
            }
        }
        return items
    }

    /// The pin row for one target, if the caller has pinned it.
    public static func pin(db: Database, kind: String, targetId: String) throws -> PinEntity? {
        guard let column = targetColumn(kind: kind) else { return nil }
        return try PinEntity
            .filter(Column("kind") == kind)
            .filter(Column(column) == targetId)
            .fetchOne(db)
    }

    /// Which `pins` column a kind writes its target into.
    public static func targetColumn(kind: String) -> String? {
        switch kind {
        case DomainContract.pinKindIssue: "issue_id"
        case DomainContract.pinKindSession: "session_id"
        case DomainContract.pinKindAction: "action_id"
        default: nil
        }
    }
}
