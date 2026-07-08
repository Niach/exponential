import ExpCore
import SwiftUI

/// Presentation metadata for a project `type` (dev | tasks | feedback):
/// SF Symbol, display label, and a one-line description. Single source of the
/// per-type iconography so the switcher row, create form, and settings agree.
/// Mirrors the web lucide mapping (Code2 / SquareKanban / Megaphone).
public struct ProjectTypeInfo: Identifiable, Sendable {
    public let type: String
    public let symbol: String
    public let label: String
    public let summary: String

    public var id: String { type }
}

public enum ProjectTypeDisplay {
    /// The three selectable types, in display order (Dev first — the default).
    public static let all: [ProjectTypeInfo] = [
        ProjectTypeInfo(
            type: DomainContract.projectTypeDev,
            symbol: "chevron.left.forwardslash.chevron.right",
            label: "Dev board",
            summary: "Code with Claude, PRs and coding sessions. Needs a GitHub repo."
        ),
        ProjectTypeInfo(
            type: DomainContract.projectTypeTasks,
            symbol: "square.grid.2x2",
            label: "Task board",
            summary: "Plain issue tracking. No repository required."
        ),
        ProjectTypeInfo(
            type: DomainContract.projectTypeFeedback,
            symbol: "megaphone",
            label: "Feedback board",
            summary: "A public, read-only roadmap. Visitors can't sign in to write."
        ),
    ]

    /// Info for a given type string, falling back to Dev for unknown values.
    public static func info(for type: String) -> ProjectTypeInfo {
        all.first { $0.type == type } ?? all[0]
    }

    /// SF Symbol name for a type string (Dev fallback).
    public static func symbol(for type: String) -> String {
        info(for: type).symbol
    }
}
