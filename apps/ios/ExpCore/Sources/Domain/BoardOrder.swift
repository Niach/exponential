import Foundation

/// THE order of a team's boards (sidebar-tree.json `teamItems`,
/// issue-draft.json `boards`): by sortOrder (none = last), ties by id, never
/// the order they synced in.
public enum BoardOrder {
    public static func precedes(_ a: (sortOrder: Double?, id: String), _ b: (sortOrder: Double?, id: String)) -> Bool {
        let left = a.sortOrder ?? .infinity
        let right = b.sortOrder ?? .infinity
        return left != right ? left < right : a.id < b.id
    }

    public static func sorted(_ boards: [BoardEntity]) -> [BoardEntity] {
        boards.sorted { precedes(($0.sortOrder, $0.id), ($1.sortOrder, $1.id)) }
    }
}
