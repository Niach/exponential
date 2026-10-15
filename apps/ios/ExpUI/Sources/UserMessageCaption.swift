import Foundation

/// The caption under a person's message bubble (web `userMessageCaption`,
/// ×4): `<name> · HH:mm`, then `queued` until the device took the message
/// (EXP-1317). A missing part drops with its separator. The time is the
/// 24-hour clock `GlassTimeRow.formatted` draws — the agent channel's row
/// time — never a relative `5m`.
public enum UserMessageCaption {
    /// The queued word, byte-identical across the clients.
    public static let queuedWord = "queued"
    public static let separator = " · "

    @MainActor
    public static func text(name: String?, at: Date?, queued: Bool) -> String {
        let trimmed = name?.trimmingCharacters(in: .whitespacesAndNewlines)
        return [
            trimmed.flatMap { $0.isEmpty ? nil : $0 },
            at.map { GlassTimeRow.formatted($0) },
            queued ? queuedWord : nil,
        ]
        .compactMap { $0 }
        .joined(separator: separator)
    }
}
