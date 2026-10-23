// NEW-8: messages live in conversations; a session's thread is the
// messages of its own (kind `session`) conversation. Clients key every
// message through the `conversations` shape, never through a legacy
// `messages.session_id`.
// fixtures/message-conversation.json; mirrors: desktop
// `domain::message_conversation`, iOS `MessageConversation.swift`, Android
// `domain/MessageConversation.kt`.

export interface MessageConversation {
  id: string
  /** The session a `session` conversation belongs to; null on a dm/channel. */
  sessionId: string | null
}

/** conversation id → session id, for the conversations that belong to a session. */
export function conversationSessions(
  conversations: readonly MessageConversation[]
): Map<string, string> {
  const map = new Map<string, string>()
  for (const conversation of conversations) {
    if (conversation.sessionId) map.set(conversation.id, conversation.sessionId)
  }
  return map
}

/** The messages whose conversation belongs to a session, each keyed to that
 *  session, in input order. A dm/channel message, or one whose conversation
 *  has not synced yet, is left out; a legacy `sessionId` on the row is
 *  overridden by its conversation. */
export function keyMessagesBySession<M extends { conversationId: string }>(
  messages: readonly M[],
  conversations: readonly MessageConversation[] | ReadonlyMap<string, string>
): (M & { sessionId: string })[] {
  const sessions =
    conversations instanceof Map
      ? (conversations as ReadonlyMap<string, string>)
      : conversationSessions(conversations as readonly MessageConversation[])
  const keyed: (M & { sessionId: string })[] = []
  for (const message of messages) {
    const sessionId = sessions.get(message.conversationId)
    if (sessionId) keyed.push({ ...message, sessionId })
  }
  return keyed
}
