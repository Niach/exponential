import ExpCore

// N-12: the ask dock's own views are gone (ExpUI `Dock.swift` = THE Dock);
// S-3 + #EXP-1662: a waiting question stays inline, a plan is a tab.

// MARK: - Plan options ×4

extension AskDock {
    /// The picked plan option for a key (nil = unknown).
    static func planOption(key: String?) -> PlanOption? {
        planOptions.first { $0.key == key }
    }
}
