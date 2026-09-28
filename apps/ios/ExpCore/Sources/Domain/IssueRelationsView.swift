import Foundation

/// EXP-1097 — what the issue detail DRAWS for its relations, on every client:
/// the "Sub-issue of" parent line above the title, the Sub-issues section
/// (completion ring + `done/total`, rows, only a `+`), and ONE foldable band
/// per remaining relation side (Blocked by / Blocking / Duplicate of /
/// Duplicated by / Related) — which phones draw inside the properties sheet.
///
/// Pure and FIXTURE-LOCKED ×4 (`packages/domain-contract/fixtures/
/// issue-relations-view.json`): web `lib/issue-relations-view.ts`, desktop
/// `domain::relations_view`, Android `IssueRelationsView.kt` return
/// byte-identical output, copy included.
///
/// Storage stays canonical-direction: `parent` = issueId is the parent of
/// relatedIssueId, `blocks` = issueId blocks relatedIssueId, `duplicate` =
/// issueId duplicates relatedIssueId, `related` = symmetric. A row whose other
/// end is not synced is dropped; an unknown type folds into Related.
public enum IssueRelationsView {

    /// A band's side key — its fold state is stored per key.
    public enum BandKey: String, Equatable, Hashable, Sendable, CaseIterable {
        case blockedBy = "blocked_by"
        case blocking
        case duplicateOf = "duplicate_of"
        case duplicatedBy = "duplicated_by"
        case related
    }

    public struct Issue: Equatable, Sendable {
        public let id: String
        public let identifier: String
        public let title: String
        /// The dual-written ANCHOR enum (`issues.status`).
        public let status: String

        public init(id: String, identifier: String, title: String, status: String) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.status = status
        }
    }

    public struct Relation: Equatable, Sendable {
        public let type: String
        public let issueId: String
        public let relatedIssueId: String

        public init(type: String, issueId: String, relatedIssueId: String) {
            self.type = type
            self.issueId = issueId
            self.relatedIssueId = relatedIssueId
        }
    }

    public struct Input: Equatable, Sendable {
        public let subjectId: String
        public let relations: [Relation]
        /// Every synced issue the relations may name (the team's issues).
        public let issues: [Issue]
        /// Bands the user folded/unfolded this session, overriding the default.
        public let toggled: Set<BandKey>
        /// Bands whose "Show N more" was pressed.
        public let showAll: Set<BandKey>

        public init(
            subjectId: String,
            relations: [Relation],
            issues: [Issue],
            toggled: Set<BandKey> = [],
            showAll: Set<BandKey> = []
        ) {
            self.subjectId = subjectId
            self.relations = relations
            self.issues = issues
            self.toggled = toggled
            self.showAll = showAll
        }
    }

    public struct Row: Equatable, Sendable, Identifiable {
        public let id: String
        public let identifier: String
        public let title: String
        public let status: String
        public let open: Bool

        public init(id: String, identifier: String, title: String, status: String, open: Bool) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.status = status
            self.open = open
        }
    }

    public struct Band: Equatable, Sendable, Identifiable {
        public let key: BandKey
        public let title: String
        public let count: Int
        public let openCount: Int
        public let expanded: Bool
        /// The rows drawn right now (none while folded).
        public let rows: [Row]
        /// "Show N more", nil when nothing is hidden.
        public let more: String?
        /// "Show less", only once "Show N more" was pressed and rows exceed the cap.
        public let less: String?

        public var id: String { key.rawValue }
    }

    public struct SubIssues: Equatable, Sendable {
        public let rows: [Row]
        public let done: Int
        public let total: Int
        /// `2/5`, nil when there are no sub-issues.
        public let progress: String?
    }

    public struct Model: Equatable, Sendable {
        /// The "Sub-issue of" line's parent, nil when the subject has none.
        public let parent: Row?
        public let subIssues: SubIssues
        public let bands: [Band]
        /// Total rows across the bands (the phone sheet's "Relations" count).
        public let relationCount: Int
    }

    // MARK: - Copy (byte-identical ×4)

    public enum Copy {
        public static let subIssues = "Sub-issues"
        public static let subIssueOf = "Sub-issue of"
        public static let addSubIssues = "Add sub-issues"
        public static let relations = "Relations"
        public static let add = "Add"
        public static let blockedBy = "Blocked by"
        public static let blocking = "Blocking"
        public static let duplicateOf = "Duplicate of"
        public static let duplicatedBy = "Duplicated by"
        public static let related = "Related"
        public static let showLess = "Show less"

        public static let table: [String: String] = [
            "subIssues": subIssues,
            "subIssueOf": subIssueOf,
            "addSubIssues": addSubIssues,
            "relations": relations,
            "add": add,
            "blockedBy": blockedBy,
            "blocking": blocking,
            "duplicateOf": duplicateOf,
            "duplicatedBy": duplicatedBy,
            "related": related,
            "showLess": showLess,
        ]
    }

    public static func showMore(_ count: Int) -> String { "Show \(count) more" }
    public static func progress(_ done: Int, _ total: Int) -> String { "\(done)/\(total)" }

    /// Rows a band shows before "Show N more".
    public static let bandCap = 3

    private static let closedAnchors: Set<String> = ["done", "cancelled", "duplicate"]

    private static let bandOrder: [BandKey] = [
        .blockedBy, .blocking, .duplicateOf, .duplicatedBy, .related,
    ]

    public static func title(_ key: BandKey) -> String {
        switch key {
        case .blockedBy: Copy.blockedBy
        case .blocking: Copy.blocking
        case .duplicateOf: Copy.duplicateOf
        case .duplicatedBy: Copy.duplicatedBy
        case .related: Copy.related
        }
    }

    /// The identifier's trailing number, for a natural `EXP-9 < EXP-10` order.
    /// A Double, like the JS `Number` it mirrors (no overflow on long runs).
    private static func identifierNumber(_ identifier: String) -> Double {
        let maxSafeInteger = 9_007_199_254_740_991.0
        var digits = ""
        for scalar in identifier.unicodeScalars.reversed() {
            guard ("0"..."9").contains(scalar) else { break }
            digits.unicodeScalars.insert(scalar, at: digits.unicodeScalars.startIndex)
        }
        guard !digits.isEmpty else { return maxSafeInteger }
        return Double(digits) ?? maxSafeInteger
    }

    /// Negative / zero / positive, like the JS comparator.
    private static func byIdentifier(_ a: Row, _ b: Row) -> Int {
        let na = identifierNumber(a.identifier)
        let nb = identifierNumber(b.identifier)
        if na != nb { return na < nb ? -1 : 1 }
        // JS string `<` = UTF-16 code-unit order.
        if a.identifier.utf16.elementsEqual(b.identifier.utf16) { return 0 }
        return a.identifier.utf16.lexicographicallyPrecedes(b.identifier.utf16) ? -1 : 1
    }

    /// Open rows first, each half in identifier order.
    private static func openFirst(_ a: Row, _ b: Row) -> Int {
        if a.open != b.open { return a.open ? -1 : 1 }
        return byIdentifier(a, b)
    }

    /// A STABLE sort over a three-way comparator (JS `Array.sort` is stable).
    private static func stableSorted(_ rows: [Row], _ compare: (Row, Row) -> Int) -> [Row] {
        rows.enumerated()
            .sorted { lhs, rhs in
                let order = compare(lhs.element, rhs.element)
                return order != 0 ? order < 0 : lhs.offset < rhs.offset
            }
            .map(\.element)
    }

    private static func row(_ issue: Issue) -> Row {
        Row(
            id: issue.id,
            identifier: issue.identifier,
            title: issue.title,
            status: issue.status,
            open: !closedAnchors.contains(issue.status)
        )
    }

    public static func build(_ input: Input) -> Model {
        var byId: [String: Issue] = [:]
        for issue in input.issues { byId[issue.id] = issue } // last wins, like `new Map`
        let subject = input.subjectId
        var parent: Row?
        var children: [Row] = []
        var buckets: [BandKey: [Row]] = [:]
        var seen = Set<String>()

        func push(_ key: BandKey, _ row: Row) {
            let dedupe = "\(key.rawValue):\(row.id)"
            guard seen.insert(dedupe).inserted else { return }
            buckets[key, default: []].append(row)
        }

        for relation in input.relations {
            let forward = relation.issueId == subject
            let inverse = relation.relatedIssueId == subject
            if forward == inverse { continue } // not about the subject (or a self-loop)
            guard let other = byId[forward ? relation.relatedIssueId : relation.issueId] else {
                continue
            }
            let row = row(other)
            switch relation.type {
            case "parent":
                if forward {
                    if seen.insert("child:\(row.id)").inserted {
                        children.append(row)
                    }
                } else if parent == nil || byIdentifier(row, parent!) < 0 {
                    parent = row
                }
            case "blocks":
                push(forward ? .blocking : .blockedBy, row)
            case "duplicate":
                push(forward ? .duplicateOf : .duplicatedBy, row)
            default:
                push(.related, row)
            }
        }

        children = stableSorted(children, byIdentifier)
        let done = children.filter { !$0.open }.count

        let bands: [Band] = bandOrder.compactMap { key in
            guard let bucket = buckets[key] else { return nil }
            let all = stableSorted(bucket, openFirst)
            let openCount = all.filter(\.open).count
            // Blockers are the actionable relation: they open by default while
            // one is still open. Duplicates and Related stay folded.
            let openByDefault = (key == .blockedBy || key == .blocking) && openCount > 0
            let expanded = input.toggled.contains(key) ? !openByDefault : openByDefault
            let everything = input.showAll.contains(key)
            let overflow = all.count > bandCap
            let rows: [Row] = !expanded
                ? []
                : (everything || !overflow ? all : Array(all.prefix(bandCap)))
            return Band(
                key: key,
                title: title(key),
                count: all.count,
                openCount: openCount,
                expanded: expanded,
                rows: rows,
                more: expanded && overflow && !everything ? showMore(all.count - bandCap) : nil,
                less: expanded && overflow && everything ? Copy.showLess : nil
            )
        }

        return Model(
            parent: parent,
            subIssues: SubIssues(
                rows: children,
                done: done,
                total: children.count,
                progress: children.isEmpty ? nil : progress(done, children.count)
            ),
            bands: bands,
            relationCount: bands.reduce(0) { $0 + $1.count }
        )
    }
}
