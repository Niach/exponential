package com.exponential.app.domain

// #EXP-1730 (one-tree model): a message lives on a SESSION
// (`messages.session_id`); a session's thread is the messages whose own
// session id is that session, never keyed through `conversations`.
// Hand-ported from `packages/domain-contract/src/message-session.ts`,
// locked to `fixtures/message-session.json` by MessageSessionTest.
// Mirrors: desktop `domain::message_session`, iOS ExpCore
// `MessageSession.swift`.

object MessageSession {

    /**
     * The messages that carry a session, each paired with that session's id,
     * in input order. A message with no session (a legacy dm/channel row) is
     * left out; a legacy conversation id on the row is never read.
     */
    fun <M> keyMessagesBySession(
        messages: List<M>,
        sessionId: (M) -> String?,
    ): List<Pair<M, String>> {
        val keyed = ArrayList<Pair<M, String>>(messages.size)
        for (message in messages) {
            val id = sessionId(message)
            if (!id.isNullOrEmpty()) keyed.add(message to id)
        }
        return keyed
    }
}
