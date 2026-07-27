package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf

// Inline `@email` mentions (REV2-42) — the Android counterpart of
// apps/web/src/lib/mention-refs.ts (+ the TipTap decoration in
// mention-pill-extension.ts) and the desktop `scan_mentions` /
// `DecorationStyle::MentionPill`. `@dev@example.com` is the single GFM
// interchange form: it stays plain text in the stored markdown, so detection
// happens only at RENDER time — MarkdownView shows a known member's token as
// their name pill, unknown addresses stay plain text, and the editor's
// @-autocomplete (BlockTextField) keeps inserting the plain token.

/** Resolves a mention email to a visible team member; null = unknown. */
@Immutable
class MentionResolver(
    /** The team's visible members (same list the @-autocomplete offers). */
    val members: List<MentionMember>,
) {
    private val byEmail: Map<String, MentionMember> =
        members.associateBy { it.email.lowercase() }

    fun resolve(email: String): MentionMember? = byEmail[email.lowercase()]
}

/**
 * Provided by screens that can resolve team members (issue detail covers the
 * description read view and the comment thread); null (the default) keeps
 * every token plain text.
 */
val LocalMentions = compositionLocalOf<MentionResolver?> { null }

object Mentions {

    /** A token occurrence in [findAll]; `[start, end)` spans `@` + the email. */
    data class Match(val start: Int, val end: Int, val email: String)

    // Byte-identical to MENTION_SOURCE in apps/web/src/lib/mention-refs.ts —
    // the same rule the server uses to resolve mentions and fire
    // issue_mention notifications, so a pill appears exactly where a
    // notification was sent.
    private val REGEX = Regex("@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,})")

    /** All `@email` tokens in [text], emails as written (not normalized). */
    fun findAll(text: String): List<Match> {
        if (!text.contains('@')) return emptyList()
        return REGEX.findAll(text)
            .map { m -> Match(m.range.first, m.range.last + 1, m.groupValues[1]) }
            .toList()
    }
}
