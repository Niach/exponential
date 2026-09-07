package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import com.exponential.app.domain.ResolvedIssueStatus

// Inline `#IDENTIFIER` issue references (masterplan §5e) — the Android
// counterpart of apps/web/src/lib/issue-refs.ts (+ the TipTap decoration in
// issue-ref-extension.ts and the team IssueRefProvider). The token is the
// single GFM interchange form (`#MET-115` stays plain text in the stored
// markdown, like `@email` mentions), so detection happens only at render time:
// MarkdownView pills a token when it resolves to a synced issue in the same
// team and leaves unknown identifiers as plain text, and the editor's
// #-autocomplete (BlockTextField) inserts the plain token. The
// parser/serializer never see these — GFM byte-parity is untouched.

/** An identifier resolved to a visible (Room-synced) issue. */
@Immutable
data class IssueRefTarget(
    val issueId: String,
    val identifier: String,
    val title: String = "",
    /**
     * The issue's resolved status (EXP-314) — the chip paints its pie-clock
     * glyph over the token's `#` (EXP-423). Null (a screen that only powers the
     * #-autocomplete, or a not-yet-synced status) renders the chip without a
     * glyph and keeps the `#` visible.
     */
    val resolvedStatus: ResolvedIssueStatus? = null,
)

/**
 * Team-scoped resolver + tap navigation + autocomplete search for
 * `#IDENTIFIER` tokens. Mirrors the web IssueRefProvider: [candidates] is the
 * team's visible issues newest-first, so an empty-query search surfaces
 * fresh work.
 */
@Immutable
class IssueRefHandler(
    /** Visible issues in the team, newest-first (from the Room issues table). */
    val candidates: List<IssueRefTarget>,
    /**
     * Whether [onOpen] actually navigates. False on screens that provide this
     * handler for the #-autocomplete ALONE (the create screen): an editor chip
     * must not swallow a tap there, it has to fall through and place the caret
     * (EXP-423).
     */
    val canOpen: Boolean = true,
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
    fun search(query: String, limit: Int = 8): List<IssueRefTarget> {
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

/**
 * EXP-760 — the steering feed also chips identifiers written WITHOUT a `#`
 * (agents narrate `EXP-758`). Provided as `true` by the session screen ONLY:
 * it is display-only, so descriptions, comments and every stored-text path
 * keep the `#IDENTIFIER` contract. Read where a composable can, then handed to
 * [annotateLine] as a parameter (it is not a composable itself).
 */
val LocalIssueRefBare = compositionLocalOf { false }

object IssueRefs {

    /**
     * A token occurrence in [findAll]'s input; `[start, end)` spans the token
     * as written. [bare] marks the `#`-less form, whose first character is a
     * letter — nothing may paint a status glyph over it or hide it.
     */
    data class Match(
        val start: Int,
        val end: Int,
        val identifier: String,
        val bare: Boolean = false,
    )

    // Mirrors ISSUE_REF_SOURCE in apps/web/src/lib/issue-refs.ts: `#` must not
    // be glued to a word or another `#` (so `foo#MET-1` / `##MET-1` don't
    // match), the identifier is `{PREFIX}-{number}`, and the match must end at
    // a token boundary (so `#MET-115-2` / `#MET-115abc` don't half-match).
    private const val HASH = "(?<![\\w#])#([A-Za-z][A-Za-z0-9]*-\\d+)"
    private const val TAIL = "(?![\\w-])"

    // Mirrors ISSUE_REF_BARE_SOURCE (EXP-760): an UPPERCASE prefix only, so
    // `utf-8` / `x86-64` / `exp-758` never chip, and not glued to a word, a
    // `#` or a `-` — `foo-EXP-1` stays text while `exp/EXP-758` chips.
    private const val BARE = "(?<![\\w#-])([A-Z][A-Z0-9]*-\\d+)"

    private val REGEX = Regex("$HASH$TAIL")
    private val REGEX_BARE = Regex("(?:$HASH|$BARE)$TAIL")

    /**
     * All `#IDENTIFIER` tokens in [text], identifiers as written (not
     * normalized). [bare] additionally matches bare `EXP-758` tokens — the
     * steering feeds' display-only mode, never a stored-text path.
     */
    fun findAll(text: String, bare: Boolean = false): List<Match> {
        if (!bare && !text.contains('#')) return emptyList()
        val regex = if (bare) REGEX_BARE else REGEX
        return regex.findAll(text)
            .map { m ->
                // Group 1 is the `#` form, group 2 the bare one; exactly one
                // participates (the bare regex has both, the other only one).
                val hashed = m.groupValues[1]
                val identifier = if (hashed.isNotEmpty()) hashed else m.groupValues[2]
                Match(
                    m.range.first,
                    m.range.last + 1,
                    identifier,
                    bare = hashed.isEmpty(),
                )
            }
            .toList()
    }
}
