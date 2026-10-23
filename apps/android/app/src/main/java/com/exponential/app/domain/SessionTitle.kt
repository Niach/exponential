package com.exponential.app.domain

// EXP-1341/NEW-1: THE display title of any session ×4 (never `Session N`).
// Hand-ported from `packages/domain-contract/src/session-title.ts` and locked
// to `fixtures/session-title.json` by SessionTitleTest. Mirrors: desktop
// `domain::session_title`, iOS ExpCore `SessionTitle.swift`.

object SessionTitle {

    // fixture `titles` / `titleMax` / `untitled`
    const val TITLE_ISSUE = "#{identifier} {title}"
    const val TITLE_ISSUES = "#{identifier} {title} + {more}"
    const val TITLE_MAX = 80
    const val UNTITLED = "Untitled"

    data class TitleSession(
        val id: String,
        val rootId: String,
        val title: String?,
    )

    data class TitleLink(
        /** NEW-1: the session that HOLDS the issue (the row sits on it). */
        val sessionId: String,
        val issueId: String,
        val sortOrder: Double,
    )

    data class TitleIssue(val id: String, val identifier: String, val title: String)

    data class TitleMessage(
        val id: String,
        val sessionId: String,
        val authorKind: String,
        val authorSessionId: String?,
        val body: String,
        /** True when the message carries a card (cards never name a session). */
        val hasCard: Boolean,
        val createdAt: String,
    )

    private fun ms(iso: String): Long = if (iso.isEmpty()) 0L else WireTimestamps.parseEpochMs(iso) ?: 0L

    /** Cut to [max] characters, ending in `…`. */
    private fun clip(text: String, max: Int): String =
        if (text.length > max) "${text.substring(0, maxOf(0, max - 1)).trimEnd()}…" else text

    /** The first usable line of a body: non-blank, not an image embed. */
    private fun firstLine(body: String): String? =
        body.split("\n").map { it.trim() }.firstOrNull { it.isNotEmpty() && !it.startsWith("![") }

    /** The session's OWN rows, in link order (root or child alike, NEW-1). */
    fun sessionIssues(session: TitleSession, links: List<TitleLink>, issues: List<TitleIssue>): List<TitleIssue> {
        val byId = issues.associateBy { it.id }
        return links
            .filter { it.sessionId == session.id }
            .sortedWith(compareBy<TitleLink>({ it.sortOrder }, { it.issueId }))
            .mapNotNull { byId[it.issueId] }
    }

    /** The first prompt's usable line: a person's text or another session's input. */
    fun firstPrompt(sessionId: String, messages: List<TitleMessage>): String? =
        messages
            .filter { m ->
                m.sessionId == sessionId && !m.hasCard &&
                    (
                        m.authorKind == DomainContract.messageAuthorKindUser ||
                            (!m.authorSessionId.isNullOrEmpty() && m.authorSessionId != sessionId)
                        )
            }
            .sortedWith(compareBy<TitleMessage>({ ms(it.createdAt) }, { it.id }))
            .firstNotNullOfOrNull { firstLine(it.body) }

    private fun issueLabel(issues: List<TitleIssue>): String {
        val first = issues[0]
        val template = (if (issues.size > 1) TITLE_ISSUES else TITLE_ISSUE)
            .replace("{identifier}", first.identifier)
            .replace("{more}", (issues.size - 1).toString())
        val room = TITLE_MAX - template.replace("{title}", "").length
        return template.replace("{title}", clip(first.title.trim(), maxOf(1, room)))
    }

    /** THE display title (fixture `session-title.json` rungs). */
    fun sessionTitle(
        session: TitleSession,
        links: List<TitleLink>,
        issues: List<TitleIssue>,
        messages: List<TitleMessage>,
    ): String {
        val named = session.title?.trim()?.takeIf { it.isNotEmpty() }
        if (named != null) return named
        val own = sessionIssues(session, links, issues)
        if (own.isNotEmpty()) return issueLabel(own)
        return firstPrompt(session.id, messages)?.let { clip(it, TITLE_MAX) } ?: UNTITLED
    }

    /** Every given session's title, keyed by id (the tree's card names). */
    fun sessionTitles(
        sessions: List<TitleSession>,
        links: List<TitleLink>,
        issues: List<TitleIssue>,
        messages: List<TitleMessage>,
    ): Map<String, String> = sessions.associate { it.id to sessionTitle(it, links, issues, messages) }
}
