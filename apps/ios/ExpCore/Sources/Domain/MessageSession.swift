import Foundation

// #EXP-1730 (one-tree model): a message lives on a SESSION
// (`messages.session_id`); clients read the row's own session and never key
// through `conversations`. fixtures/message-session.json; mirrors: TS
// `src/message-session.ts`, desktop `domain::message_session`, Android
// `domain/MessageSession.kt`. The local store keys the same way in SQL
// (`SessionQueries.keyedMessagesSQL`).

public enum MessageSessions {
    /// The messages that carry a session, each keyed to it, in input order;
    /// a message with no session (nil or empty: a legacy dm/channel row)
    /// belongs to none.
    public static func keyMessagesBySession<M>(
        _ messages: [M],
        sessionId: (M) -> String?
    ) -> [(message: M, sessionId: String)] {
        messages.compactMap { message in
            guard let id = sessionId(message), !id.isEmpty else { return nil }
            return (message, id)
        }
    }
}
