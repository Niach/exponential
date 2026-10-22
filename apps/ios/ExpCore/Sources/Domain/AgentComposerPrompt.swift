import Foundation

/// EXP-825: the composer's free text on the wire (`sessions.send({text})`).
/// With a subject picked it is OPTIONAL additional instructions (the brief's
/// Request section); with none it IS the request. Images ride it as the steer embed format
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
    /// Wave D: non-image files ride after the image embeds as
    /// `[<filename>](/api/attachments/<id>)` lines (`SteerImageMessage.buildMessage`).
    public static func build(
        text: String, attachmentIds: [String], files: [SteerImageMessage.File] = []
    ) -> String? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty, attachmentIds.isEmpty, files.isEmpty { return nil }
        return SteerImageMessage.buildMessage(text: trimmed, imageIds: attachmentIds, files: files)
    }

    /// Whether the draft (before its embeds) fits the contract cap.
    public static func withinLimit(_ text: String) -> Bool {
        text.count <= maxLength
    }

    /// What the composer is about to start.
    public enum Subject: Equatable, Sendable {
        /// No subject: the text is the request.
        case none
        /// `count` checked issues (the new thread's issues).
        case issues(count: Int)
        /// NEW-2: a team prompt picked in the `/` menu (`promptId`).
        case prompt
        /// NEW-3: a message into the caller's DM with a team agent
        /// (`agentId`); composer-menu.json `message-agent`.
        case agent
    }

    /// The submit label (web `submitLabelFor`): "Start chat" / "Start
    /// coding" / "Start session" for a prompt / "Send" into an agent's DM.
    public static func submitTitle(for subject: Subject) -> String {
        switch subject {
        case .none: "Start chat"
        case .issues: "Start coding"
        case .prompt: "Start session"
        case .agent: "Send"
        }
    }

    /// EXP-1038: the composer's HEADLINE verb, above the field and beside the
    /// subject chips — the contract's `composerUi*` copy ×4: "Implement" for
    /// issue chips, "Ask the agent" for a subjectless chat (which has no
    /// chips, so the verb stands alone). A prompt's or an agent's chip IS the
    /// headline: no verb (web `launchHeadlineVerb`).
    public static func headline(for subject: Subject) -> String {
        switch subject {
        case .none: DomainContract.composerUiChatHeadline
        case .issues: DomainContract.composerUiImplementHeadline
        case .prompt, .agent: ""
        }
    }

    /// The field's placeholder — web `composerPlaceholder` (the contract's
    /// `composerUi*` copy): a chat asks for the message; with any subject up
    /// the field is the secondary half and asks for what is optional.
    public static func placeholder(for subject: Subject) -> String {
        switch subject {
        case .none: DomainContract.composerUiChatPlaceholder
        case .issues, .prompt, .agent: DomainContract.composerUiInstructionsPlaceholder
        }
    }
}
