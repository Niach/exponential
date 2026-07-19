import ExpCore
import SwiftUI

public enum BoardTypeDisplay {
    /// SF Symbol for every curated icon name in DomainContract.boardIconValues.
    /// Mirrors the web lucide glyphs; every one of the 16 names maps.
    public static let iconSymbols: [String: String] = [
        "code": "chevron.left.forwardslash.chevron.right",
        "square-kanban": "square.grid.2x2",
        "megaphone": "megaphone",
        "bug": "ladybug",
        "rocket": "paperplane",
        "book-open": "book",
        "globe": "globe",
        "heart": "heart",
        "star": "star",
        "zap": "bolt",
        "wrench": "wrench.and.screwdriver",
        "shield": "shield",
        "package": "shippingbox",
        "terminal": "terminal",
        "lightbulb": "lightbulb",
        "message-circle": "message",
    ]

    /// SF Symbol for a stored curated icon name (nil/unknown → the caller's
    /// fallback in `symbol(for:)`).
    public static func iconSymbol(for icon: String?) -> String? {
        guard let icon else { return nil }
        return iconSymbols[icon]
    }

    /// The glyph for a board: its stored curated `icon`, else a fallback
    /// derived from its shape — repo-backed boards read as the code glyph,
    /// everything else as the kanban grid.
    public static func symbol(for board: BoardEntity) -> String {
        if let symbol = iconSymbol(for: board.icon) { return symbol }
        if board.repositoryId != nil { return "chevron.left.forwardslash.chevron.right" }
        return "square.grid.2x2"
    }
}
