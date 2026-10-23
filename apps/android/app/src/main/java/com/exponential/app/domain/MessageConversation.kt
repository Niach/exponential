package com.exponential.app.domain

// NEW-8: messages live in conversations; a session's thread is the messages
// of its own (kind `session`) conversation. Every message is keyed through
// the `conversations` shape, never through a legacy `messages.session_id`.
// Hand-ported from `packages/domain-contract/src/message-conversation.ts`,
// locked to `fixtures/message-conversation.json` by MessageConversationTest.
// Mirrors: desktop `domain::message_conversation`, iOS ExpCore
// `MessageConversation.swift`.

object MessageConversation {

    /** [sessionId] = the session a `session` conversation belongs to; null on a dm/channel. */
    data class Conversation(val id: String, val sessionId: String?)

    /** conversation id → session id, for the conversations that belong to a session. */
    fun conversationSessions(conversations: List<Conversation>): Map<String, String> {
        val map = HashMap<String, String>()
        for (conversation in conversations) {
            val sessionId = conversation.sessionId
            if (!sessionId.isNullOrEmpty()) map[conversation.id] = sessionId
        }
        return map
    }

    /**
     * The messages whose conversation belongs to a session, each paired with
     * that session's id, in input order. A dm/channel message, or one whose
     * conversation has not synced yet, is left out; a legacy session id on
     * the row is never read.
     */
    fun <M> keyMessagesBySession(
        messages: List<M>,
        sessions: Map<String, String>,
        conversationId: (M) -> String,
    ): List<Pair<M, String>> {
        val keyed = ArrayList<Pair<M, String>>(messages.size)
        for (message in messages) {
            val sessionId = sessions[conversationId(message)] ?: continue
            keyed.add(message to sessionId)
        }
        return keyed
    }

    fun <M> keyMessagesBySession(
        messages: List<M>,
        conversations: List<Conversation>,
        conversationId: (M) -> String,
    ): List<Pair<M, String>> = keyMessagesBySession(messages, conversationSessions(conversations), conversationId)
}
