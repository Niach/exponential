// #EXP-1730 (one-tree model): a message lives on a session
// (`messages.session_id`); clients read the row's own `sessionId`, never the
// `conversations` table. fixtures/message-session.json; mirrors: desktop
// `domain::message_session`, iOS `MessageSession.swift`, Android
// `domain/MessageSession.kt`.

/** The messages that carry a session, each keyed to it, in input order; a
 *  message with no session (a legacy dm/channel row) is left out. */
export function keyMessagesBySession<M extends { sessionId?: string | null }>(
  messages: readonly M[]
): (M & { sessionId: string })[] {
  return messages.filter((message): message is M & { sessionId: string } => typeof message.sessionId === `string` && message.sessionId.length > 0)
}
