package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.threads.ThreadsContent
import com.exponential.app.ui.threads.threadRowMeta

/** Which Threads capture to draw (one shot each). */
enum class ThreadsCaptureKind(val testId: String) {
    /** sg_reviews: the Threads tab (it replaced Reviews). */
    Threads("threads-capture-screen"),
}

/**
 * EXP-1296: the Threads tab as a capture-only screen — the REAL
 * [ThreadsContent] over the draft's data (8-phones panel 2) built through
 * the real domain model ([SessionTree.partitionThreads],
 * [SessionTree.listAgents], [threadRowMeta]).
 */
@Composable
fun ThreadsCaptures(kind: ThreadsCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            when (kind) {
                ThreadsCaptureKind.Threads -> {
                    val groups = SessionTree.partitionThreads(
                        SampleSessions, SampleLinks, SampleIssues, emptyList(), teamId = null,
                    )
                    ThreadsContent(
                        groups = groups,
                        agents = SessionTree.listAgents(SampleSessions, teamId = null),
                        teams = listOf(SampleTeam),
                        rowMeta = threadRowMeta(
                            groups, SampleSessions, SampleLinks, SampleIssues,
                            SampleSessions.associate { it.id to "claude" },
                        ),
                        onOpenThread = {},
                        modifier = Modifier.statusBarsPadding(),
                    )
                }
            }
        }
    }
}

private const val TEAM = "t-acme"

private val SampleTeam = TeamEntity(
    id = TEAM,
    name = "Acme",
    slug = "acme",
    createdAt = "2026-10-01T09:00:00.000Z",
    updatedAt = "2026-10-01T09:00:00.000Z",
)

private fun session(
    id: String,
    kind: String = DomainContract.sessionKindThread,
    status: String = DomainContract.sessionStatusIdle,
    busy: Boolean = false,
    needsInput: Boolean = false,
    shared: Boolean = true,
    title: String? = null,
    name: String? = null,
    icon: String? = null,
    color: String? = null,
    sortOrder: Double = 0.0,
    updatedAt: String,
) = SessionTree.Session(
    id = id,
    teamId = TEAM,
    parentId = null,
    rootId = id,
    kind = kind,
    userId = "u-alex",
    status = status,
    busy = busy,
    needsInput = needsInput,
    blocked = null,
    shared = shared,
    title = title,
    guide = null,
    actionName = null,
    name = name,
    icon = icon,
    color = color,
    sortOrder = sortOrder,
    startedAt = updatedAt,
    endedAt = if (status == DomainContract.sessionStatusEnded) updatedAt else null,
    createdAt = "2026-10-13T08:00:00.000Z",
    updatedAt = updatedAt,
)

private val SampleSessions = listOf(
    session(
        "a-triage", kind = DomainContract.sessionKindAgent, status = DomainContract.sessionStatusRunning,
        needsInput = true, name = "Triage", icon = "sparkles", color = "#8b5cf6", sortOrder = 0.0,
        updatedAt = "2026-10-13T09:40:00.000Z",
    ),
    session(
        "a-designer", kind = DomainContract.sessionKindAgent, name = "Designer", icon = "palette",
        color = "#ec4899", sortOrder = 1.0, updatedAt = "2026-10-13T09:00:00.000Z",
    ),
    session(
        "a-release", kind = DomainContract.sessionKindAgent, status = DomainContract.sessionStatusRunning,
        busy = true, name = "Release", icon = "rocket", color = "#22c55e", sortOrder = 2.0,
        updatedAt = "2026-10-13T09:35:00.000Z",
    ),
    session(
        "s-labels", status = DomainContract.sessionStatusRunning, needsInput = true,
        title = "Batch-edit labels on many issues", updatedAt = "2026-10-13T09:45:00.000Z",
    ),
    session(
        "s-group", status = DomainContract.sessionStatusRunning, busy = true,
        title = "Group board issues by assignee", updatedAt = "2026-10-13T09:44:00.000Z",
    ),
    session(
        "s-offline", status = DomainContract.sessionStatusRunning, busy = true, shared = false,
        title = "Spike: offline queue", updatedAt = "2026-10-13T09:30:00.000Z",
    ),
    session(
        "s-cold", status = DomainContract.sessionStatusEnded,
        title = "Reduce cold start below 800 ms", updatedAt = "2026-10-13T08:30:00.000Z",
    ),
    session(
        "s-keyboard", title = "iOS keyboard and model picker", updatedAt = "2026-10-13T08:10:00.000Z",
    ),
)

private val SampleIssues = listOf(
    SessionTree.Issue("i-15", "APP-15", "Batch-edit labels on many issues"),
    SessionTree.Issue("i-14", "APP-14", "Group board issues by assignee"),
    SessionTree.Issue("i-5", "APP-5", "Reduce cold start below 800 ms"),
)

private val SampleLinks = listOf(
    SessionTree.SessionIssue("s-labels", "i-15", 0.0),
    SessionTree.SessionIssue("s-group", "i-14", 0.0),
    SessionTree.SessionIssue("s-cold", "i-5", 0.0),
)
