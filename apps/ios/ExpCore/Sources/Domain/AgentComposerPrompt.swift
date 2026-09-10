import Foundation

/// EXP-825: the composer's free text on the wire. With a subject picked it is
/// OPTIONAL additional instructions (the desktop renders it as an
/// "Additional instructions from the requester" section); with none it IS
/// the chat prompt, and for the Chat and Create action builtins the server
/// requires it. Images ride it as the steer embed format
/// (`SteerImageMessage.build`, byte-identical ×4): prose, blank line, one
/// `![image](/api/attachments/<id>)` line per upload, `[Image #k]` markers in
/// the prose. The contract caps both (`startPrompt`).
public enum AgentComposerPrompt {
    /// The contract's `startPrompt.maxLength` (16384, UTF-16 code units on
    /// the web — `count` here counts characters, which can only be FEWER).
    public static let maxLength = DomainContract.startPromptMaxLength
    /// The contract's `startPrompt.maxImages` — the same four the steer
    /// composer carries.
    public static let maxImages = DomainContract.startPromptMaxImages

    /// The `prompt` to send: nil when there is nothing to say (blank text and
    /// no images), so the wire omits the key and the server sees NO prompt
    /// rather than an empty one.
    public static func build(text: String, attachmentIds: [String]) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty, attachmentIds.isEmpty { return nil }
        return SteerImageMessage.build(text: trimmed, attachmentIds: attachmentIds)
    }

    /// Whether the draft (before its embeds) fits the contract cap.
    public static func withinLimit(_ text: String) -> Bool {
        text.count <= maxLength
    }

    /// What the composer is about to start.
    public enum Subject: Equatable, Sendable {
        /// No subject: a chat.
        case none
        /// `count` checked issues (1 = a single-issue run, 2+ = a batch).
        case issues(count: Int)
        /// A team action or a builtin.
        case action
    }

    /// The submit label — the ×4 contract: "Start chat" / "Start coding" /
    /// "Start batch · N" / "Run action".
    public static func submitTitle(for subject: Subject) -> String {
        switch subject {
        case .none: "Start chat"
        case let .issues(count): count > 1 ? "Start batch · \(count)" : "Start coding"
        case .action: "Run action"
        }
    }

    /// The field's placeholder — web `composerPlaceholder` byte for byte
    /// (EXP-825): a chat asks for the message; a picked action with a
    /// non-blank `promptPlaceholder` (`actionHint`, trimmed) shows it; every
    /// other subject asks for what is optional next to it. Issue chips never
    /// read an action's hint.
    public static func placeholder(for subject: Subject, actionHint: String?) -> String {
        switch subject {
        case .none:
            return "Ask the agent…"
        case .action:
            let hint = actionHint?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return hint.isEmpty ? "Additional instructions (optional)…" : hint
        case .issues:
            return "Additional instructions (optional)…"
        }
    }
}
