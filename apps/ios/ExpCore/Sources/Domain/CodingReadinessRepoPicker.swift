import Foundation

/// EXP-1121 — the "Ready to code?" checklist's inline repository picker: the
/// team's repositories ordered and tagged for ONE board. Pure; Android's
/// `CodingReadinessRepoPicker.kt` writes the same rule.
///
/// - Search: a case-insensitive substring of the full name (`owner/name`).
/// - Order: repos whose NAME (the part after `/`) equals the board's name or
///   slug (case-insensitive) first, then the rest — both halves keep the
///   API's original order (stable).
/// - Tag: a match reads `matches board`; otherwise the first OTHER board
///   using the repo reads `used by <Board>` (the picking board never counts);
///   else none.
public enum CodingReadinessRepoPicker {

    public struct BoardRef: Equatable, Sendable {
        public let id: String
        public let name: String

        public init(id: String, name: String) {
            self.id = id
            self.name = name
        }
    }

    public struct Repo: Equatable, Sendable {
        public let id: String
        public let fullName: String
        /// The boards this repo backs (`repositories.list().boards`).
        public let boards: [BoardRef]

        public init(id: String, fullName: String, boards: [BoardRef]) {
            self.id = id
            self.fullName = fullName
            self.boards = boards
        }
    }

    public struct Row: Equatable, Sendable {
        public let id: String
        public let fullName: String
        public let matchesBoard: Bool
        /// The trailing muted text, nil for none.
        public let tag: String?

        public init(id: String, fullName: String, matchesBoard: Bool, tag: String?) {
            self.id = id
            self.fullName = fullName
            self.matchesBoard = matchesBoard
            self.tag = tag
        }
    }

    /// Whether `fullName`'s repo half names the board.
    public static func matches(fullName: String, boardName: String, boardSlug: String) -> Bool {
        let name = (fullName.split(separator: "/").last.map(String.init) ?? fullName).lowercased()
        guard !name.isEmpty else { return false }
        return name == boardName.lowercased() || name == boardSlug.lowercased()
    }

    public static func rows(
        repos: [Repo],
        boardId: String,
        boardName: String,
        boardSlug: String,
        query: String
    ) -> [Row] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let filtered = needle.isEmpty
            ? repos
            : repos.filter { $0.fullName.lowercased().contains(needle) }
        let rows = filtered.map { repo -> Row in
            let isMatch = matches(fullName: repo.fullName, boardName: boardName, boardSlug: boardSlug)
            let tag: String? = isMatch
                ? CodingReadiness.Copy.pickerMatchesBoard
                : repo.boards.first { $0.id != boardId }.map { CodingReadiness.pickerUsedBy($0.name) }
            return Row(id: repo.id, fullName: repo.fullName, matchesBoard: isMatch, tag: tag)
        }
        return rows.filter(\.matchesBoard) + rows.filter { !$0.matchesBoard }
    }
}

public extension CodingReadinessRepoPicker.Repo {
    /// A `repositories.list` row.
    init(_ repo: TeamRepo) {
        self.init(
            id: repo.id,
            fullName: repo.fullName,
            boards: repo.boards.map { CodingReadinessRepoPicker.BoardRef(id: $0.id, name: $0.name) }
        )
    }
}
