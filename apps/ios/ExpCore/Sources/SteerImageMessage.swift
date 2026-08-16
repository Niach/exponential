import Foundation

/// EXP-511: steer messages carry attached images as markdown embeds. The host
/// device localizes each embed to a file path before the agent sees it, and the
/// activity echo restores the embed, so this exact shape is load-bearing across
/// web (`lib/steer-image-message.ts`), iOS and Android (`SteerImageMessage.kt`)
/// — keep the three builders byte-identical.
public enum SteerImageMessage {
    /// How many images one steer message may carry (MAX_STEER_IMAGES).
    public static let maxImages = 4

    public static func build(text: String, attachmentIds: [String]) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if attachmentIds.isEmpty { return trimmed }
        let embeds = attachmentIds
            .map { "![image](/api/attachments/\($0))" }
            .joined(separator: "\n")
        if trimmed.isEmpty { return embeds }
        return "\(trimmed)\n\n\(embeds)"
    }
}
