import Foundation

// EXP-1686: a to-agent send's refs. A port of `agentForwardText` of
// `packages/domain-contract/src/session-brief.ts`, locked by
// fixtures/session-brief.json `forwardTextCases` (`SessionBriefTests`).
public enum SessionBrief {
    /// The text of a message forwarding issues to an agent: their `#IDENT`
    /// refs lead the person's text. Nothing forwarded = the text untouched.
    public static func agentForwardText(_ text: String, identifiers: [String]) -> String {
        guard !identifiers.isEmpty else { return text }
        let refs = identifiers.map { "#\($0)" }.joined(separator: " ")
        let body = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return body.isEmpty ? refs : "\(refs)\n\n\(body)"
    }

    /// The wire input of a send to an agent, before sealing (chats.json START
    /// CODING): an empty text is omitted, never `""`.
    public static func agentForwardInput(
        agentId: String, text: String?, attachmentIds: [String], issueIds: [String], identifiers: [String]
    ) -> SessionsSendInput {
        let body = agentForwardText(text ?? "", identifiers: identifiers)
        return .agent(
            agentId: agentId,
            text: body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : body,
            attachmentIds: attachmentIds.isEmpty ? nil : attachmentIds,
            issueIds: issueIds.isEmpty ? nil : issueIds
        )
    }
}
