import ExpCore
import SwiftUI

public enum BoardTypeDisplay {
    /// Registry icon name for a stored curated `boards.icon` — the stored value
    /// IS the Lucide name (EXP-273 unified all four clients on one registry), so
    /// this only validates that we ship an asset for it. Unknown/newer names
    /// return nil so the caller falls back instead of drawing nothing.
    public static func iconName(for icon: String?) -> String? {
        guard let icon, AppIcons.allNames.contains(icon) else { return nil }
        return icon
    }

    /// The glyph for a board: its stored curated `icon`, else a fallback
    /// derived from its shape — repo-backed boards read as the code glyph,
    /// everything else as the kanban grid.
    public static func iconName(for board: BoardEntity) -> String {
        if let name = iconName(for: board.icon) { return name }
        if board.repositoryId != nil { return "code" }
        return "square-kanban"
    }
}
