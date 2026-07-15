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
            id: "dev",
            symbol: "chevron.left.forwardslash.chevron.right",
            label: "Dev board",
            summary: "Code with Claude, PRs and coding sessions. Connect a GitHub repo.",
            isPublic: false,
            icon: "code",
            showsRepository: true
        ),
        ProjectTemplate(
            id: "tasks",
            symbol: "square.grid.2x2",
            label: "Task board",
            summary: "Plain issue tracking. No repository required.",
            isPublic: false,
            icon: "square-kanban",
            showsRepository: false
        ),
        ProjectTemplate(
            id: "feedback",
            symbol: "megaphone",
            label: "Feedback board",
            summary: "A public, read-only roadmap. Visitors can't sign in to write.",
            isPublic: true,
            icon: "megaphone",
            showsRepository: false
        ),
    ]

    /// SF Symbol for a stored curated icon name (nil/unknown → the caller's
    /// fallback in `symbol(for:)`).
    public static func iconSymbol(for icon: String?) -> String? {
        guard let icon else { return nil }
        return iconSymbols[icon]
    }

    /// The glyph for a project: its stored curated `icon`, else a fallback
    /// derived from its shape — public boards read as a megaphone, repo-backed
    /// projects as the code glyph, everything else as the task-board grid.
    public static func symbol(for project: ProjectEntity) -> String {
        if let symbol = iconSymbol(for: project.icon) { return symbol }
        if project.isPublic { return "megaphone" }
        if project.repositoryId != nil { return "chevron.left.forwardslash.chevron.right" }
        return "square.grid.2x2"
    }
}
