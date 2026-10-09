import Foundation
import CoreGraphics

/// EXP-1248: THE list item's geometry, ×4 (web `@exp/ui` SessionRow + PrRow,
/// desktop `domain::list_item`, Android `ListItem.kt`), locked by
/// `packages/domain-contract/fixtures/list-item.json`. Anatomy: [tree guides]
/// [lead glyph at `base + indent·depth`][mono identifier][title][caption, big
/// only][trailing meta]. No fold chevron, no trailing chevron, no buttons:
/// marks and titles at one depth align exactly.
public enum ListItem {
    /// The lead's inset at depth 0 (a row's content padding).
    public static let base: CGFloat = 12
    /// Per nesting level — the tree-guide gutter (`TreeGuides.indentPerLevel`).
    public static let indent: CGFloat = TreeGuides.indentPerLevel
    /// The lead box (run mark / PR node box) — one indent square.
    public static let mark: CGFloat = 14
    /// Lead → text, text → trailing.
    public static let gap: CGFloat = 8
    /// The one-line SessionRow.
    public static let small: CGFloat = 32
    /// The two-line SessionRow (title + caption).
    public static let big: CGFloat = 52
    /// The PrRow, md+ / phones.
    public static let prRow: CGFloat = 36
    public static let prRowPhone: CGFloat = 40
    /// The PR node's ring diameter.
    public static let prNode: CGFloat = 12
    /// The stack rail's width.
    public static let rail: CGFloat = 1

    /// Where a row's lead box starts at `depth`.
    public static func leadX(depth: Int) -> CGFloat {
        base + indent * CGFloat(depth)
    }
}

/// EXP-1248: the PrRow's ring lead (fixture `prNodes`): an open PR = an
/// emerald ring, the current stack member = the ring with a filled centre,
/// the base branch = a muted ring.
public enum PrNodeState: String, Sendable, CaseIterable {
    case open
    case current
    case base

    public enum Ring: String, Sendable {
        case emerald
        case muted
    }

    public var ring: Ring { self == .base ? .muted : .emerald }
    public var filled: Bool { self == .current }
}
