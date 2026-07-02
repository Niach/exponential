package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf

// Inline `#IDENTIFIER` issue references (masterplan §5e) — the render-only
// Android counterpart of apps/web/src/lib/issue-refs.ts (+ the TipTap
// decoration in issue-ref-extension.ts). The token is the single GFM
// interchange form (`#MET-115` stays plain text in the stored markdown, like
// `@email` mentions), so detection happens only at render time: MarkdownView
// pills a token when it resolves to a synced issue in the same workspace and
// leaves unknown identifiers as plain text. The parser/serializer never see
// these — GFM byte-parity is untouched.

/** An identifier resolved to a visible (Room-synced) issue. */
@Immutable
data class IssueRefTarget(val issueId: String, val identifier: String)

/** Workspace-scoped resolver + tap navigation for `#IDENTIFIER` pills. */
@Immutable
class IssueRefHandler(
    /** Uppercased identifier → target (built from the Room issues table). */
    private val targets: Map<String, IssueRefTarget>,
    val onOpen: (IssueRefTarget) -> Unit,
) {
    /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
    fun resolve(identifier: String): IssueRefTarget? = targets[identifier.uppercase()]
}

/**
 * Provided by screens that can resolve + navigate (issue detail covers the
 * description read view and the comment thread); null (the default) keeps
 * every token plain text.
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
