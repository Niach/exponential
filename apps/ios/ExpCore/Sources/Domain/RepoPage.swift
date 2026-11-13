import Foundation

/// #EXP-1616 THE REPO PAGE ×4 (packages/domain-contract/fixtures/
/// repo-page.json: its `$comment` is the spec; TS reference
/// `src/repo-page.ts`). Pure folds every host replays: the PR tree
/// (`prTree`), the branch list (`branches`), a repo row's checks rollup on
/// Settings › Repositories (`checksRollup`) and the route's tab (`tab`).
/// Replayed by `RepoPageTests`.
public enum RepoPage {
    // MARK: - Copy and tabs (fixture `copy` / `tabs`)

    public enum Tab: String, CaseIterable, Sendable {
        case pulls, runs, branches, settings

        public var label: String {
            switch self {
            case .pulls: "Pull requests"
            case .runs: "Runs"
            case .branches: "Branches"
            case .settings: "Settings"
            }
        }
    }

    /// `tabs`, in order.
    public static let tabs: [Tab] = [.pulls, .runs, .branches, .settings]

    public enum Copy {
        public enum Provider {
            public static let github = "GitHub"
            public static let ecode = "Exponential"

            /// The provider `Badge` (any provider but `ecode` reads GitHub).
            public static func label(_ provider: String) -> String {
                provider == "ecode" ? ecode : github
            }
        }

        public enum Add {
            public static let title = "Add repository"
            public static let github = "GitHub"
            public static let ecode = "Exponential"
            public static let name = "Name"
            public static let defaultBranch = "Default branch"
            public static let mirror = "Mirror to GitHub"
            public static let mirrorUrl = "Mirror URL"
            public static let cancel = "Cancel"
            public static let create = "Create"
        }

        public enum Pulls {
            public static let empty = "No open pull requests"
        }

        public enum Runs {
            public static let empty = "No runs yet"
        }

        public enum Branches {
            public static let empty = "No branches yet"
            public static let `default` = "Default"
        }

        public enum Settings {
            public static let provider = "Provider"
            public static let defaultBranch = "Default branch"
            public static let cloneUrl = "Clone URL"
            public static let backups = "Backups"
            public static let noBackups = "No backups yet"
            public static let restore = "Restore…"
            public static let restoreTitle = "Restore this backup?"
            public static let restoreConfirm = "Restore"
            public static let mirror = "Mirror"
            public static let mirrorUrl = "Mirror URL"
        }

        /// `copy.checks[state]`: the chip's label and its dot tone.
        public struct Check: Equatable, Sendable {
            public let label: String
            public let tone: Tone
        }

        public enum Tone: String, Sendable {
            case green, amber, red
        }

        public static func checks(_ state: String) -> Check? {
            switch state {
            case "success": Check(label: "Passing", tone: .green)
            case "pending": Check(label: "Running", tone: .amber)
            case "failure": Check(label: "Failing", tone: .red)
            default: nil
            }
        }
    }

    public enum TestIds {
        public static let page = "repo-page"
        public static let rowOpen = "repo-row-open"
    }

    /// The Exponential add form's default-branch choices (`main` first).
    public static let addBranches = ["main", "master", "develop"]

    /// A valid Exponential repo name: [A-Za-z0-9._-], not dot-led, no `.git`.
    public static func validName(_ name: String) -> Bool {
        guard !name.isEmpty, !name.hasPrefix("."), !name.hasSuffix(".git") else { return false }
        return name.unicodeScalars.allSatisfy { scalar in
            scalar.isASCII && (CharacterSet.alphanumerics.contains(scalar) || "._-".unicodeScalars.contains(scalar))
        }
    }

    // MARK: - Folds

    public struct Pr: Equatable, Sendable {
        public let id: String
        public let number: Int
        public let title: String?
        public let head: String
        public let base: String
        public let state: String
        public let checksState: String

        public init(id: String, number: Int, title: String?, head: String, base: String, state: String, checksState: String) {
            self.id = id
            self.number = number
            self.title = title
            self.head = head
            self.base = base
            self.state = state
            self.checksState = checksState
        }
    }

    public struct PrRow: Equatable, Sendable {
        public let id: String
        public let number: Int
        /// `#<number>`.
        public let identifier: String
        public let title: String
        public let depth: Int
        /// `<head> → <base>`.
        public let subtitle: String
        public let draft: Bool
        /// The rollup chip; nil = no checks (no chip).
        public let checks: String?

        public init(
            id: String, number: Int, identifier: String, title: String, depth: Int, subtitle: String, draft: Bool, checks: String?
        ) {
            self.id = id
            self.number = number
            self.identifier = identifier
            self.title = title
            self.depth = depth
            self.subtitle = subtitle
            self.draft = draft
            self.checks = checks
        }
    }

    public struct BranchRow: Equatable, Sendable {
        public let name: String
        /// The first 7 characters of the sha.
        public let sha: String
        public let isDefault: Bool
        /// The live PR whose head this branch is, nil = none.
        public let prNumber: Int?

        public init(name: String, sha: String, isDefault: Bool, prNumber: Int?) {
            self.name = name
            self.sha = sha
            self.isDefault = isDefault
            self.prNumber = prNumber
        }
    }

    private static func live(_ pr: Pr) -> Bool { pr.state == "open" || pr.state == "draft" }

    /// A stable sort by number (ties keep their input order, like JS).
    private static func byNumber(_ prs: [Pr], descending: Bool) -> [Pr] {
        prs.enumerated().sorted { a, b in
            if a.element.number != b.element.number {
                return descending ? a.element.number > b.element.number : a.element.number < b.element.number
            }
            return a.offset < b.offset
        }.map(\.element)
    }

    /// Code-point order (Rust's `str` order).
    static func codePointLess(_ a: String, _ b: String) -> Bool {
        a.unicodeScalars.map(\.value).lexicographicallyPrecedes(b.unicodeScalars.map(\.value))
    }

    /// The live PRs as a tree: a PR's parent is the live PR whose head is its
    /// base (lowest number when two share a head). Roots newest first;
    /// children oldest first, depth-first; a loop starts at its newest member.
    public static func prTree(_ prs: [Pr]) -> [PrRow] {
        let live = prs.filter(live)
        var byHead: [String: Pr] = [:]
        for pr in byNumber(live, descending: false) where byHead[pr.head] == nil {
            byHead[pr.head] = pr
        }
        var children: [String: [Pr]] = [:]
        var roots: [Pr] = []
        for pr in live {
            if let parent = byHead[pr.base], parent.id != pr.id {
                children[parent.id, default: []].append(pr)
            } else {
                roots.append(pr)
            }
        }
        var rows: [PrRow] = []
        var seen = Set<String>()
        func visit(_ pr: Pr, _ depth: Int) {
            guard seen.insert(pr.id).inserted else { return }
            rows.append(PrRow(
                id: pr.id,
                number: pr.number,
                identifier: "#\(pr.number)",
                title: pr.title ?? "",
                depth: depth,
                subtitle: "\(pr.head) → \(pr.base)",
                draft: pr.state == "draft",
                checks: pr.checksState == "none" ? nil : pr.checksState
            ))
            for child in byNumber(children[pr.id] ?? [], descending: false) {
                visit(child, depth + 1)
            }
        }
        for root in byNumber(roots, descending: true) { visit(root, 0) }
        // A loop of live PRs (no root): its newest member starts it at depth 0.
        for pr in byNumber(live, descending: true) { visit(pr, 0) }
        return rows
    }

    /// The default branch first, then by name (code points).
    public static func branches(_ branches: [(name: String, sha: String)], defaultBranch: String, prs: [Pr]) -> [BranchRow] {
        let sorted = branches.enumerated().sorted { a, b in
            let x = a.element.name, y = b.element.name
            if x == defaultBranch || y == defaultBranch {
                if x == defaultBranch && y != defaultBranch { return true }
                if y == defaultBranch && x != defaultBranch { return false }
                return a.offset < b.offset
            }
            if x != y { return codePointLess(x, y) }
            return a.offset < b.offset
        }.map(\.element)
        return sorted.map { branch in
            BranchRow(
                name: branch.name,
                sha: String(branch.sha.prefix(7)),
                isDefault: branch.name == defaultBranch,
                prNumber: byNumber(prs.filter { live($0) && $0.head == branch.name }, descending: false).first?.number
            )
        }
    }

    /// A repo row's checks chip over its live PRs: failure (any) · pending
    /// (any) · success (all of the ones with checks) · nil = none has checks.
    public static func checksRollup(_ prs: [Pr]) -> String? {
        let states = prs.filter { live($0) && $0.checksState != "none" }.map(\.checksState)
        if states.isEmpty { return nil }
        if states.contains("failure") { return "failure" }
        if states.contains("pending") { return "pending" }
        return "success"
    }

    /// The tab a route's `tab` param opens (unknown = pulls).
    public static func tab(_ param: String?) -> Tab {
        param.flatMap(Tab.init(rawValue:)) ?? .pulls
    }
}

extension RepoPage.Pr {
    public init(_ entity: PullRequestEntity) {
        self.init(
            id: entity.id,
            number: entity.number,
            title: entity.title,
            head: entity.head,
            base: entity.base,
            state: entity.state,
            checksState: entity.checksState
        )
    }
}
