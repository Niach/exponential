package com.exponential.app.ui.agent

import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.markdown.IssueRefTarget

/**
 * One issue the composer can chip for a run (EXP-825, the iOS `IssueOption`
 * twin) — repositoryId gates same-repo batches; status/priority feed the
 * picker row's list-style visuals (EXP-173). Deliberately no defaults: a
 * producer that forgets them would compile fine and silently render every
 * row as Backlog/no-priority via fromWire's fallback.
 */
data class IssueOption(
    override val id: String,
    override val identifier: String,
    override val title: String,
    val repositoryId: String?,
    // EXP-922: also `IssueSearch.Row.status` — undone issues rank above done
    // ones in the composer's picker, like every other search.
    override val status: String?,
    val priority: String?,
    // EXP-892: the shared IssueSearch ranking fields. Same no-defaults rule —
    // a producer that forgets them silently ranks every row as the oldest.
    override val description: String?,
    override val createdAt: String?,
    override val updatedAt: String?,
    // #EXP-2096: the status as its OWN team resolves it (a custom row, its
    // pie-clock); null = only the anchor is known (the launch pool).
    val resolvedStatus: ResolvedIssueStatus?,
) : IssueSearch.Row {
    /** The row's and the chip's status glyph: the resolved status, else the
     *  anchor against the builtin defaults. */
    val glyph: ResolvedIssueStatus
        get() = resolvedStatus ?: IssueStatusResolver.resolve(null, status, emptyList())
}

/** #EXP-2096 (agent-settings.json `assistant.chatTeams` (5), (6)): a chat's
 *  `+` › Issue row from its `#` target: the same status glyph, read from the
 *  ISSUE's own team, and the same ranking fields. */
fun issueOption(target: IssueRefTarget): IssueOption = IssueOption(
    id = target.issueId,
    identifier = target.identifier,
    title = target.title,
    repositoryId = null,
    status = target.status,
    priority = null,
    description = target.description,
    createdAt = target.createdAt,
    updatedAt = target.updatedAt,
    resolvedStatus = target.resolvedStatus,
)
