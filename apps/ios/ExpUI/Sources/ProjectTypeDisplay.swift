import ExpCore
import SwiftUI

/// Create-flow quickstart template: pre-sets `isPublic`, the curated `icon`,
/// and whether the repository section starts visible. Replaces the old
/// type-first picker — a project is public-or-not + an icon, never a `type`
/// (the server derives the legacy `type` column from these). Mirrors the web
/// template cards (Dev / Task / Feedback board).
public struct ProjectTemplate: Identifiable, Sendable {
    /// Stable key for selection state (dev | tasks | feedback). NOT sent to the
    /// server — the create mutation carries `isPublic` + `icon`.
    public let id: String
    public let symbol: String
    public let label: String
    public let summary: String
    /// Public-board switch this template pre-sets.
    public let isPublic: Bool
    /// Curated glyph name (DomainContract.projectIconValues) this template
    /// pre-sets.
    public let icon: String
    /// Whether the repo picker section starts shown (repos are ALWAYS optional
    /// now — this only decides the initial disclosure).
    public let showsRepository: Bool
}

public enum ProjectTypeDisplay {
    /// SF Symbol for every curated icon name in DomainContract.projectIconValues.
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

    /// The three quickstart templates, in display order (Dev first — the default).
    public static let templates: [ProjectTemplate] = [
        ProjectTemplate(
            id: DomainContract.projectTypeDev,
            symbol: "chevron.left.forwardslash.chevron.right",
            label: "Dev board",
            summary: "Code with Claude, PRs and coding sessions. Connect a GitHub repo.",
            isPublic: false,
            icon: "code",
            showsRepository: true
        ),
        ProjectTemplate(
            id: DomainContract.projectTypeTasks,
            symbol: "square.grid.2x2",
            label: "Task board",
            summary: "Plain issue tracking. No repository required.",
            isPublic: false,
            icon: "square-kanban",
            showsRepository: false
        ),
        ProjectTemplate(
            id: DomainContract.projectTypeFeedback,
            symbol: "megaphone",
            label: "Feedback board",
            summary: "A public, read-only roadmap. Visitors can't sign in to write.",
            isPublic: true,
            icon: "megaphone",
            showsRepository: false
        ),
    ]

    /// SF Symbol for a stored curated icon name (nil/unknown → the type
    /// fallback via `typeSymbol`).
    public static func iconSymbol(for icon: String?) -> String? {
        guard let icon else { return nil }
        return iconSymbols[icon]
    }

    /// Fallback SF Symbol derived from the legacy `type` when a project carries
    /// no stored icon. Kept only for that fallback — never a behavior gate.
    public static func typeSymbol(for type: String) -> String {
        switch type {
        case DomainContract.projectTypeTasks: return "square.grid.2x2"
        case DomainContract.projectTypeFeedback: return "megaphone"
        default: return "chevron.left.forwardslash.chevron.right"
        }
    }

    /// The glyph for a project: its stored curated `icon`, else the
    /// type-derived fallback.
    public static func symbol(for project: ProjectEntity) -> String {
        iconSymbol(for: project.icon) ?? typeSymbol(for: project.type)
    }
}
