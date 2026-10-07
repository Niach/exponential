package com.exponential.app.ui.agent

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueSearch

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
) : IssueSearch.Row

/**
 * EXP-1233: a synced issue row as a composer chip — the Fix merge conflicts
 * headline draws its pull request's issues with the issues subject's chip.
 * No repository: these chips never gate a batch.
 */
internal fun IssueEntity.toIssueOption(): IssueOption = IssueOption(
    id = id,
    identifier = identifier,
    title = title,
    repositoryId = null,
    status = status,
    priority = priority,
    description = description,
    createdAt = createdAt,
    updatedAt = updatedAt,
)
