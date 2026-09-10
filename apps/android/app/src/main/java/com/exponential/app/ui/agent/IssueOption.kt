package com.exponential.app.ui.agent

/**
 * One issue the composer can chip for a run (EXP-825, the iOS `IssueOption`
 * twin) — repositoryId gates same-repo batches; status/priority feed the
 * picker row's list-style visuals (EXP-173). Deliberately no defaults: a
 * producer that forgets them would compile fine and silently render every
 * row as Backlog/no-priority via fromWire's fallback.
 */
data class IssueOption(
    val id: String,
    val identifier: String,
    val title: String,
    val repositoryId: String?,
    val status: String?,
    val priority: String?,
)
