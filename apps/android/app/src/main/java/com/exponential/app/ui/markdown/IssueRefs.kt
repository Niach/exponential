package com.exponential.app.ui.markdown

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.compositionLocalOf
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatusResolver
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
    override val identifier: String,
    override val title: String = "",
    /**
     * The issue's resolved status (EXP-314) — the chip paints its pie-clock
     * glyph over the token's `#` (EXP-423). Null (a screen that only powers the
     * #-autocomplete, or a not-yet-synced status) renders the chip without a
     * glyph and keeps the `#` visible.
     */
    val resolvedStatus: ResolvedIssueStatus? = null,
    /**
     * EXP-892: the ranking fields of the shared [IssueSearch] engine. Filled
     * wherever a handler is built from Room issues (issue detail, the create
     * screen, the agent + steer composers); a target synthesized from a
     * server hit carries what the hit knew and ranks as the oldest row.
     */
    override val description: String? = null,
    override val createdAt: String? = null,
    override val updatedAt: String? = null,
    /**
     * EXP-922: the builtin status ANCHOR, so the `#` menu lists undone work
     * above finished work like every other search. Null = ranks as open.
     */
    override val status: String? = null,
) : IssueSearch.Row {
    /** [IssueSearch.Row]'s id — this type has called it [issueId] since §5e. */
    override val id: String get() = issueId
}

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
    /**
     * EXP-892: the server-side full-text search (`issues.search` — title,
     * description AND comment bodies, stemmed) behind the locally ranked
     * rows. Null on a host that cannot be asynchronous; the menu is then
     * local-only, which is exactly what it always was.
     */
    val searchServer: (suspend (String) -> List<IssueRefTarget>)? = null,
    val onOpen: (IssueRefTarget) -> Unit,
) {
    /** Uppercased identifier → target (last wins on duplicates, like the web Map). */
    private val targets: Map<String, IssueRefTarget> =
        candidates.associateBy { it.identifier.uppercase() }

    /** Resolve an identifier (case-insensitive) to a visible issue, or null. */
    fun resolve(identifier: String): IssueRefTarget? = targets[identifier.uppercase()]

    /**
     * The editor's `#` autocomplete, ranked by the ONE shared engine
     * (EXP-892): identifier before title before description, empty query =
     * most recently created. Web IssueRefProvider.search parity.
     */
    fun search(query: String, limit: Int = ISSUE_REF_MENU_LIMIT): List<IssueRefTarget> =
        IssueSearch.rank(candidates, query, limit = limit)

    /**
     * [search] with the server's full-text [hits] spliced in behind it: a hit
     * the local pool already holds renders as its LIVE synced row, an unsynced
     * one from the fields the hit itself carried.
     */
    fun searchWith(
        query: String,
        hits: List<IssueRefTarget>,
        limit: Int = ISSUE_REF_MENU_LIMIT,
    ): List<IssueRefTarget> {
        val local = search(query, limit)
        if (hits.isEmpty()) return local
        val byId = candidates.associateBy { it.issueId }
        return IssueSearch.mergeServerHits(local, hits, limit = limit) { hit ->
            byId[hit.issueId] ?: hit
        }
    }
}

/** Rows the `#` menu shows at most — the one cap all four clients share. */
const val ISSUE_REF_MENU_LIMIT = 8

/**
 * A synced Room issue as an autocomplete target: the chip's status glyph is
 * precomputed here (EXP-423) and the EXP-892 ranking fields ride along, so
 * every ViewModel builds the vocabulary the same way.
 */
fun issueRefTarget(issue: IssueEntity, statuses: List<ResolvedIssueStatus>): IssueRefTarget =
    IssueRefTarget(
        issueId = issue.id,
        identifier = issue.identifier,
        title = issue.title,
        resolvedStatus = IssueStatusResolver.resolve(issue, statuses),
        description = issue.description,
        createdAt = issue.createdAt,
        updatedAt = issue.updatedAt,
        status = issue.status,
    )

/**
 * A server full-text hit as a target — an issue this device has not synced
 * (or not yet). It carries only the anchor enum, so the glyph resolves against
 * the CONSTRUCTED builtins: a custom status row of a team we have not synced
 * is not knowable, and the builtins render the same either way. No timestamps,
 * so it ranks as the oldest row — which is moot, the merge preserves the
 * server's relevance order behind the local rows.
 */
fun issueRefTarget(hit: SearchIssueHit): IssueRefTarget = IssueRefTarget(
    issueId = hit.id,
    identifier = hit.identifier,
    title = hit.title,
    resolvedStatus = IssueStatusResolver.resolve(
        statusId = null,
        anchor = hit.status,
        team = IssueStatusResolver.builtinDefaults,
    ),
    status = hit.status,
)

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
