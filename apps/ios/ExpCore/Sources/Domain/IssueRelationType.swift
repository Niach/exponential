import Foundation

/// The four canonical relation types (`issue_relations.type`, EXP-736), the
/// hand-maintained mirror of `packages/domain-contract/contract.json`
/// `issueRelationType` — locked to the generated constants by
/// IssueRelationContractTests, exactly like `IssueStatus`.
///
/// Every type has ONE canonical direction, stored on the row: the row's
/// `issue_id` side is the FORWARD side and `related_issue_id` the INVERSE one.
/// `blocks` = the issue blocks the related one, `parent` = the issue is the
/// parent of the related one, `duplicate` = the issue IS the duplicate (the
/// related one is canonical, mirroring `issues.duplicate_of_id`), `related` is
/// symmetric and reads the same from both sides.
public enum IssueRelationType: String, CaseIterable, Codable, Identifiable, Sendable {
    case blocks
    case parent
    case duplicate
    case related

    public var id: String { rawValue }

    /// The label for ONE side of the row: `false` reads it from the row's own
    /// issue (forward), `true` from the related issue. Byte-identical to
    /// contract `forwardLabels` / `inverseLabels` on all four clients.
    public func label(inverse: Bool) -> String {
        switch self {
        case .blocks: inverse ? "blocked by" : "blocks"
        case .parent: inverse ? "sub-issue of" : "parent of"
        case .duplicate: inverse ? "duplicated by" : "duplicate of"
        case .related: "related to"
        }
    }

    /// Tolerant wire decoder — an unknown type (an older client meeting a
    /// newer server) yields nil and the row is simply not rendered.
    public static func from(_ wire: String?) -> IssueRelationType? {
        guard let wire else { return nil }
        return IssueRelationType(rawValue: wire)
    }
}

/// One entry of the "Add relation" picker: the wire type plus which side the
/// caller's issue ends up on. Order and wording are shared across the four
/// clients — Parent of, Sub-issue of, Blocking, Blocked by, Duplicate of,
/// Related to.
public struct RelationPick: Identifiable, Sendable, Equatable {
    public let type: IssueRelationType
    /// True = the caller's issue is the RELATED side of the stored row.
    public let inverse: Bool
    public let title: String

    public init(type: IssueRelationType, inverse: Bool, title: String) {
        self.type = type
        self.inverse = inverse
        self.title = title
    }

    public var id: String { "\(type.rawValue)-\(inverse)" }

    public static let all: [RelationPick] = [
        RelationPick(type: .parent, inverse: false, title: "Parent of"),
        RelationPick(type: .parent, inverse: true, title: "Sub-issue of"),
        RelationPick(type: .blocks, inverse: false, title: "Blocking"),
        RelationPick(type: .blocks, inverse: true, title: "Blocked by"),
        RelationPick(type: .duplicate, inverse: false, title: "Duplicate of"),
        RelationPick(type: .related, inverse: false, title: "Related to"),
    ]

    /// Render order for a relation row: its (type, side) position in `all`.
    /// A side with no picker entry (`duplicated by`) sorts after every pick.
    public static func order(type: IssueRelationType, inverse: Bool) -> Int {
        all.firstIndex { $0.type == type && $0.inverse == inverse } ?? all.count
    }
}

extension IssueRelationType {
    /// The activity phrase for a `relation_added` / `relation_removed` event,
    /// byte-identical across web, desktop, iOS and Android. `inverse` is the
    /// event payload's `direction`, so each issue's own row reads from its own
    /// side.
    public static func eventPhrase(
        type: IssueRelationType,
        inverse: Bool,
        identifier: String,
        removed: Bool
    ) -> String {
        if type == .related {
            return removed ? "removed related issue \(identifier)" : "added related issue \(identifier)"
        }
        let label = type.label(inverse: inverse)
        return removed ? "no longer \(label) \(identifier)" : "marked as \(label) \(identifier)"
    }
}
