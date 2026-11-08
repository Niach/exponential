import Foundation

/// EXP-741: one issue's comments nested by parent — the top-level cards in
/// their input order, and each card's replies (also in input order) keyed by
/// the parent id. Mirrors web `lib/comment-replies.ts`, the desktop
/// `nest_comments` and Android `nestComments`.
public struct CommentReplies: Sendable {
    public var topLevel: [CommentEntity]
    public var repliesByParent: [String: [CommentEntity]]

    public init(topLevel: [CommentEntity] = [], repliesByParent: [String: [CommentEntity]] = [:]) {
        self.topLevel = topLevel
        self.repliesByParent = repliesByParent
    }

    /// Every row — the "Activity (N)" count.
    public var count: Int {
        topLevel.count + repliesByParent.values.reduce(0) { $0 + $1.count }
    }
}

/// Replies are ONE level deep by construction (`comments.create` re-parents a
/// reply-to-a-reply onto the root), so a row is a reply exactly when
/// `parentId` is set. A reply whose parent is NOT in the list (still syncing,
/// or gone from a partial snapshot) surfaces as a top-level card rather than
/// disappearing — the row is still real activity.
public func nestComments(_ comments: [CommentEntity]) -> CommentReplies {
    let ids = Set(comments.map(\.id))
    var nested = CommentReplies()
    for comment in comments {
        if let parentId = comment.parentId, parentId != comment.id, ids.contains(parentId) {
            nested.repliesByParent[parentId, default: []].append(comment)
        } else {
            nested.topLevel.append(comment)
        }
    }
    return nested
}
