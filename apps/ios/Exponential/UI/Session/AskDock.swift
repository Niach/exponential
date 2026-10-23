import ExpCore

// N-12: the ask dock's own views are gone — a waiting question or plan is a
// DockCard of THE Dock (ExpUI `Dock.swift`) whose sheet holds the card
// (`ThreadDockSheets.swift`, the session view's `dockSheet`).

// MARK: - Plan options ×4

extension AskDock {
    /// The picked plan option for a key (nil = unknown).
    static func planOption(key: String?) -> PlanOption? {
        planOptions.first { $0.key == key }
    }
}
