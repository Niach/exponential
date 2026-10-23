import Foundation

// NEW-8: messages live in conversations; a session's thread is the messages
// of its own (kind `session`) conversation. Every message is keyed through
// the `conversations` shape, never through a legacy `messages.session_id`.
// fixtures/message-conversation.json; mirrors: TS
// `src/message-conversation.ts`, desktop `domain::message_conversation`,
// Android `domain/MessageConversation.kt`.

public struct MessageConversation: Equatable, Sendable {
    public let id: String
    /// The session a `session` conversation belongs to; nil on a dm/channel.
    public let sessionId: String?

    public init(id: String, sessionId: String?) {
        self.id = id
        self.sessionId = sessionId
    }
}

public enum MessageConversations {
    /// conversation id → session id, for the conversations that belong to a
    /// session.
    public static func conversationSessions(_ conversations: [MessageConversation]) -> [String: String] {
        var map: [String: String] = [:]
        for conversation in conversations {
            if let sessionId = conversation.sessionId, !sessionId.isEmpty {
                map[conversation.id] = sessionId
            }
        }
        return map
    }

    /// The messages whose conversation belongs to a session, each keyed to
    /// that session, in input order. A dm/channel message, or one whose
    /// conversation has not synced yet, is left out; a legacy session id on
    /// the row is never read: the conversation decides.
    public static func keyMessagesBySession<M>(
        _ messages: [M],
        conversationId: (M) -> String,
        sessions: [String: String]
    ) -> [(message: M, sessionId: String)] {
        messages.compactMap { message in
            sessions[conversationId(message)].map { (message, $0) }
        }
    }

    public static func keyMessagesBySession<M>(
        _ messages: [M],
        conversationId: (M) -> String,
        conversations: [MessageConversation]
    ) -> [(message: M, sessionId: String)] {
        keyMessagesBySession(messages, conversationId: conversationId, sessions: conversationSessions(conversations))
    }
}
