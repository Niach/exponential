import Foundation

/// A board entry mirrored from the app's local DB into the app-group
/// container so the Share Extension can populate its board picker without
/// opening the (per-account, non-shared) GRDB database.
public struct MirroredBoard: Codable, Sendable, Identifiable {
    public let accountId: String
    public let accountName: String
    public let teamId: String
    public let teamName: String
    public let boardId: String
    public let boardName: String
    public let prefix: String

    public var id: String { accountId + ":" + boardId }

    public init(
        accountId: String,
        accountName: String,
        teamId: String,
        teamName: String,
        boardId: String,
        boardName: String,
        prefix: String
    ) {
        self.accountId = accountId
        self.accountName = accountName
        self.teamId = teamId
        self.teamName = teamName
        self.boardId = boardId
        self.boardName = boardName
        self.prefix = prefix
    }
}

/// The most recently opened/created board, used as the picker's default.
public struct LastUsedBoard: Codable, Sendable {
    public let accountId: String
    public let boardId: String

    public init(accountId: String, boardId: String) {
        self.accountId = accountId
        self.boardId = boardId
    }
}

/// Reads/writes the board picker mirror in the shared app-group defaults. The
/// app writes (whenever boards sync / a board is opened); the extension
/// reads. All keys live under [SharedAppGroup.suiteName].
public enum SharedBoardMirror {
    private static let boardsKey = "picker_boards_v1"
    private static let lastUsedKey = "picker_last_used_board_v1"

    // MARK: - App writes

    public static func write(boards: [MirroredBoard]) {
        guard let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(boards)
        else { return }
        defaults.set(data, forKey: boardsKey)
    }

    public static func writeLastUsed(accountId: String, boardId: String) {
        guard !accountId.isEmpty, !boardId.isEmpty,
              let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(LastUsedBoard(accountId: accountId, boardId: boardId))
        else { return }
        defaults.set(data, forKey: lastUsedKey)
    }

    // MARK: - Teardown

    /// Drop everything mirrored for one account (sign-out / remove-server /
    /// delete-account). The mirror is only ever rewritten by a *remaining*
    /// signed-in account's board observation, so without this an account torn
    /// down from Settings would leave its team/board/display names in the
    /// app-group container forever (and the extension would keep offering
    /// boards it can no longer reach).
    public static func remove(accountId: String) {
        guard !accountId.isEmpty, let defaults = SharedAppGroup.defaults else { return }
        let kept = readBoards().filter { $0.accountId != accountId }
        if kept.isEmpty {
            defaults.removeObject(forKey: boardsKey)
        } else if let data = try? JSONEncoder().encode(kept) {
            defaults.set(data, forKey: boardsKey)
        }
        if readLastUsed()?.accountId == accountId {
            defaults.removeObject(forKey: lastUsedKey)
        }
    }

    /// Forget the picker default once its account is no longer signed in.
    public static func pruneLastUsed(signedInAccountIds: Set<String>) {
        guard let defaults = SharedAppGroup.defaults,
              let last = readLastUsed(),
              !signedInAccountIds.contains(last.accountId)
        else { return }
        defaults.removeObject(forKey: lastUsedKey)
    }

    // MARK: - Extension reads

    public static func readBoards() -> [MirroredBoard] {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: boardsKey),
              let list = try? JSONDecoder().decode([MirroredBoard].self, from: data)
        else { return [] }
        return list
    }

    public static func readLastUsed() -> LastUsedBoard? {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: lastUsedKey),
              let value = try? JSONDecoder().decode(LastUsedBoard.self, from: data)
        else { return nil }
        return value
    }
}
