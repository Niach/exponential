import Foundation

/// #EXP-1661: team skills on the phone (read + pick only), a 1:1 port of
/// `packages/domain-contract/src/skills.ts`, locked by `SkillsTests` against
/// `fixtures/skills.json`. Phones never create or edit a skill, so the form
/// validation stays on web + desktop.
public enum Skills {
    public static let title = "Skills"
    /// The pill a row shows while `enabledByDefault`.
    public static let defaultPill = "Default"
    /// The + submenu's empty line.
    public static let pickerEmpty = "No skills"
    /// The agent page row (`agentRow`).
    public static let agentRowLabel = "Skills"
    public static let agentRowNone = "None"

    /// #VAPP-213 (`builtin`): the built-in skills every team carries (read-only
    /// rows with `builtin` true), their app-editor delivery order, and the pill
    /// a built-in's row shows instead of Default.
    public enum Builtin {
        public static let names = ["exponential-apps", "exponential-genui", "exponential-motion"]
        public static let editorDelivery = ["exponential-apps", "exponential-genui", "exponential-motion"]
        public static let pill = "Built-in"
    }

    /// The skill page's fields (`page.fields`): label + placeholder.
    public enum Field {
        public static let name = (label: "Name", placeholder: "review-checklist")
        public static let description = (label: "Description", placeholder: "What the skill does and when to use it")
        public static let body = (label: "Instructions", placeholder: "The SKILL.md body, in markdown")
    }

    public enum TestId {
        public static let page = "settings-skills-page"
        public static let list = "settings-skills-list"
        public static let row = "settings-skills-row"
    }

    /// Settings › Skills and the + submenu order: by name, code-point order.
    public static func sortSkills(_ skills: [SkillEntity]) -> [SkillEntity] {
        skills.sorted { $0.name.unicodeScalars.lexicographicallyPrecedes($1.name.unicodeScalars) }
    }

    /// The launch pick's seed (the MCP servers' rule): the saved pick clamped
    /// to skills that still exist, else the enabled-by-default ones; list
    /// order either way.
    public static func preselectSkillIds(_ skills: [SkillEntity], saved: [String]?) -> [String] {
        let sorted = sortSkills(skills)
        if let saved {
            let picked = Set(saved)
            return sorted.filter { picked.contains($0.id) }.map(\.id)
        }
        return sorted.filter(\.enabledByDefault).map(\.id)
    }

    /// The list row's pill (`pillCases`): Built-in for a built-in (never
    /// Default), Default while a team skill is pre-selected, else none.
    public static func skillPill(builtin: Bool, enabledByDefault: Bool) -> String? {
        if builtin { return Builtin.pill }
        return enabledByDefault ? defaultPill : nil
    }

    /// Whether the skill page is editable (`editCases`): owners, never on a built-in.
    public static func canEditSkill(builtin: Bool, owner: Bool) -> Bool {
        owner && !builtin
    }

    /// A built-in's file path (`filePathCases`): relative, `/`-separated, no
    /// empty, `.` or `..` segment, no backslash, never SKILL.md itself.
    public static func isSkillFilePath(_ path: String) -> Bool {
        if path.isEmpty || path == "SKILL.md" || path.contains("\\") || path.contains("\u{0}") { return false }
        return path.split(separator: "/", omittingEmptySubsequences: false)
            .allSatisfy { $0 != "" && $0 != "." && $0 != ".." }
    }

    /// The + menu row's value: the number picked, none at zero.
    public static func skillMenuValue(_ count: Int) -> String? {
        count <= 0 ? nil : "\(count)"
    }

    /// The agent page row's value: the picked names as the one picker
    /// summary (`None` / `a` / `a, b` / `a, b +2`, web `pickerSummary`).
    public static func agentSkillsValue(_ names: [String]) -> String {
        if names.isEmpty { return agentRowNone }
        if names.count <= 2 { return names.joined(separator: ", ") }
        return names.prefix(2).joined(separator: ", ") + " +\(names.count - 2)"
    }
}
