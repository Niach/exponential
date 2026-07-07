package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf

// Inline `#IDENTIFIER` issue references (masterplan §5e) — the Android
// counterpart of apps/web/src/lib/issue-refs.ts (+ the TipTap decoration in
// issue-ref-extension.ts and the workspace IssueRefProvider). The token is the
// single GFM interchange form (`#MET-115` stays plain text in the stored
// markdown, like `@email` mentions), so detection happens only at render time:
// MarkdownView pills a token when it resolves to a synced issue in the same
// workspace and leaves unknown identifiers as plain text, and the editor's
// #-autocomplete (BlockTextField) inserts the plain token. The
// parser/serializer never see these — GFM byte-parity is untouched.

/** An identifier resolved to a visible (Room-synced) issue. */
@Immutable
data class IssueRefTarget(
    val issueId: String,
    val identifier: String,
    val title: String = "",
)

/**
 * Workspace-scoped resolver + tap navigation + autocomplete search for
 * `#IDENTIFIER` tokens. Mirrors the web IssueRefProvider: [candidates] is the
 * workspace's visible issues newest-first, so an empty-query search surfaces
 * fresh work.
 */
@Immutable
class IssueRefHandler(
    /** Visible issues in the workspace, newest-first (from the Room issues table). */
    val candidates: List<IssueRefTarget>,
    val onOpen: (IssueRefTarget) -> Unit,
) {
    /** Uppercased identifier → target (last wins on duplicates, like the web Map). */
    private val targets: Map<String, IssueRefTarget> =
        candidates.associateBy { it.identifier.uppercase() }

    /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
    fun resolve(identifier: String): IssueRefTarget? = targets[identifier.uppercase()]

    /**
     * Identifier/title substring search for the editor's `#` autocomplete;
     * empty query = most recent. Mirrors web IssueRefProvider.search.
     */
    fun search(query: String, limit: Int = 6): List<IssueRefTarget> {
        val q = query.trim().lowercase()
        val out = ArrayList<IssueRefTarget>(limit)
        for (candidate in candidates) {
            if (
                q.isNotEmpty() &&
                !candidate.identifier.lowercase().contains(q) &&
                !candidate.title.lowercase().contains(q)
            ) {
                continue
            }
            out.add(candidate)
            if (out.size >= limit) break
        }
        return out
    }
}

/**
 * Provided by screens that can resolve + navigate (issue detail covers the
 * description read view, the comment thread, and every embedded editor's
 * #-autocomplete; the create screen provides it for autocomplete only); null
 * (the default) keeps every token plain text and disables the affordance.
 */
val LocalIssueRefs = compositionLocalOf<IssueRefHandler?> { null }

object IssueRefs {

    /** A token occurrence in [findAll]'s input; `[start, end)` spans `#` + identifier. */
    data class Match(val start: Int, val end: Int, val identifier: String)

    // Mirrors ISSUE_REF_SOURCE in apps/web/src/lib/issue-refs.ts: `#` must not
    // be glued to a word or another `#` (so `foo#MET-1` / `##MET-1` don't
    // match), the identifier is `{PREFIX}-{number}`, and the match must end at
    // a token boundary (so `#MET-115-2` / `#MET-115abc` don't half-match).
    private val REGEX = Regex("(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)(?![\\w-])")

    /** All `#IDENTIFIER` tokens in [text], identifiers as written (not normalized). */
    fun findAll(text: String): List<Match> {
        if (!text.contains('#')) return emptyList()
        return REGEX.findAll(text)
            .map { m -> Match(m.range.first, m.range.last + 1, m.groupValues[1]) }
            .toList()
    }
}
