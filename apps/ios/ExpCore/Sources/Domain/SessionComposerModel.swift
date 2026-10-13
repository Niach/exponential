import Foundation

public extension AgentSessionConfig {
    /// The session composer footer's model pill — the `model` config option's
    /// value, nil when the engine reported none or a blank (web
    /// `lib/agent-feed.ts`).
    var modelValue: String? {
        guard let value = options.first(where: { $0.id == "model" })?.value, !value.isEmpty else {
            return nil
        }
        return value
    }
}
