import Foundation

/// EXP-484: which rate-limit window this phone shows for an agent's usage bar.
///
/// Deliberately LOCAL and per-client — the desktop keeps its own in
/// `coding::Settings`, the web its own in localStorage. It is a viewing
/// preference, not machine state, so it never rides the device wire.
/// An unremembered agent falls back to the fullest window
/// (`AgentUsagePresentation.selectWindow`).
enum AgentUsageWindowPrefs {
    private static func key(for agent: String) -> String { "agentUsageWindow.\(agent)" }

    /// The remembered window key, or nil when this phone has never picked one
    /// for [agent] (or picked one the machine no longer reports).
    static func read(agent: String) -> String? {
        let stored = UserDefaults.standard.string(forKey: key(for: agent))
        guard let stored, !stored.isEmpty else { return nil }
        return stored
    }

    static func remember(agent: String, key windowKey: String) {
        UserDefaults.standard.set(windowKey, forKey: key(for: agent))
    }
}
