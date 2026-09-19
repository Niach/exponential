import Foundation

/// EXP-965: what ONE nested row draws in the gutters to its left.
public struct TreeGuide: Equatable, Sendable {
    /// The gutter level carrying the elbow (`depth - 1`); nil at depth 0.
    public let elbowAt: Int?
    /// A later sibling follows — the elbow's vertical runs the full height.
    public let tee: Bool
    /// Ancestor gutter levels whose subtree continues after this row: a
    /// straight full-height line each.
    public let passThrough: [Int]

    public init(elbowAt: Int? = nil, tee: Bool = false, passThrough: [Int] = []) {
        self.elbowAt = elbowAt
        self.tee = tee
        self.passThrough = passThrough
    }

    /// The row's nesting depth, as the guide describes it.
    public var depth: Int { elbowAt.map { $0 + 1 } ?? 0 }

    /// Nothing to draw (a root row).
    public var isEmpty: Bool { elbowAt == nil && passThrough.isEmpty }
}

/// EXP-965: the CONNECTOR every nested list draws — the Agent page's Running
/// band and its Recent sheet, the Reviews stack, the work header's overlay.
/// A row used to be nested by leading padding alone, so three levels of runs
/// read as three arbitrary left margins; the guide lines say which row hangs
/// off which.
///
/// ONE pure rule, mirrored ×4 (web, desktop, Android): for a row at depth
/// `d ≥ 1` the PARENT level's gutter (the `indentPerLevel` band between
/// `base + indent * (d - 1)` and `base + indent * d`) carries an elbow —
/// down from the row's top edge to its vertical centre, a rounded turn, then
/// a stub to the gutter's right edge. A row with a later sibling continues
/// the vertical to the bottom edge (a tee). Every ANCESTOR level whose
/// subtree carries on after this row carries a straight full-height line, so
/// a deep child stays visually attached to every level above it. A root row
/// (and anything folded out of the list) draws nothing.
public enum TreeGuides {

    /// 14 pt per level — the ×4 measure, and the ONE place it lives on iOS
    /// (the session lists, the Reviews stack and the PR-graph overlay each
    /// used to type their own).
    public static let indentPerLevel: CGFloat = 14

    /// The rounded turn at the elbow.
    public static let elbowRadius: CGFloat = 5

    /// The guides for a list of VISIBLE rows, in order, keyed only by their
    /// depths — the tree's shape is already in them (a child follows its
    /// parent directly, `SessionTree`), so nothing else is needed.
    public static func compute(depths: [Int]) -> [TreeGuide] {
        depths.indices.map { index in
            let depth = depths[index]
            guard depth >= 1 else { return TreeGuide() }
            let passThrough = (0..<(depth - 1)).filter { level in
                continues(depths, after: index, at: level + 1)
            }
            return TreeGuide(
                elbowAt: depth - 1,
                tee: continues(depths, after: index, at: depth),
                passThrough: passThrough
            )
        }
    }

    /// Does the subtree at `depth` carry on after `index` — i.e. is there a
    /// later row at exactly that depth before the list steps back out of it?
    private static func continues(_ depths: [Int], after index: Int, at depth: Int) -> Bool {
        var next = index + 1
        while next < depths.count {
            if depths[next] < depth { return false }
            if depths[next] == depth { return true }
            next += 1
        }
        return false
    }
}
