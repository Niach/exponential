package com.exponential.app.domain

/**
 * EXP-1686: the client half of `packages/domain-contract/src/session-brief.ts`,
 * locked to `fixtures/session-brief.json` `forwardTextCases` by SessionBriefTest.
 */
object SessionBrief {
    /**
     * The text of issues forwarded to an agent: the refs, then the person's
     * text. The refs are the bubble's record; the agent's brief rides
     * `issue_brief` server-side. Nothing forwarded = the text untouched.
     */
    fun agentForwardText(text: String, identifiers: List<String>): String {
        if (identifiers.isEmpty()) return text
        val refs = identifiers.joinToString(" ") { "#$it" }
        val body = text.trim()
        return if (body.isEmpty()) refs else "$refs\n\n$body"
    }
}
