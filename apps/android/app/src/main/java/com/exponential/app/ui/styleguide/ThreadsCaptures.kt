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
 * [ThreadsContent] over the seed's tree (EXP-1302) built through
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
                            SampleSessions.associate { it.id to if (it.id == "s-group") "codex" else "claude" },
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
    parentId: String? = null,
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
    parentId = parentId,
    rootId = parentId ?: id,
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

/**
 * EXP-1302: the seed's tree (seedSessionTree in
 * apps/web/scripts/seed-screenshots.ts, live state per
 * apps/web/scripts/lib/demo-live-sessions.ts), so this list reads like the
 * web and desktop `threads` shots: the three idle agents, the cold-start
 * thread waiting on a person through its child, three running roots and two
 * recent ones.
 */
private val SampleSessions = listOf(
    session(
        "a-triage", kind = DomainContract.sessionKindAgent, name = "Triage", icon = "bug",
        color = "#a855f7", sortOrder = 1.0, updatedAt = "2026-10-13T07:14:40.000Z",
    ),
    session(
        "a-designer", kind = DomainContract.sessionKindAgent, name = "Designer", icon = "palette",
        color = "#ec4899", sortOrder = 2.0, updatedAt = "2026-10-13T07:14:40.000Z",
    ),
    session(
        "a-release", kind = DomainContract.sessionKindAgent, name = "Release", icon = "package",
        color = "#22c55e", sortOrder = 3.0, updatedAt = "2026-10-13T07:14:40.000Z",
    ),
    session(
        "s-cold", status = DomainContract.sessionStatusRunning, updatedAt = "2026-10-13T09:40:00.000Z",
    ),
    session(
        "s-profile", parentId = "s-cold", status = DomainContract.sessionStatusRunning, needsInput = true,
        title = "Profile the launch path", updatedAt = "2026-10-13T09:41:00.000Z",
    ),
    session(
        "s-offline", status = DomainContract.sessionStatusRunning, shared = false,
        title = "Spike: offline queue", updatedAt = "2026-10-13T09:41:00.000Z",
    ),
    session(
        "s-reports", status = DomainContract.sessionStatusRunning, busy = true,
        title = "Sort today's widget reports", updatedAt = "2026-10-13T09:40:30.000Z",
    ),
    session(
        "s-group", status = DomainContract.sessionStatusRunning, busy = true,
        updatedAt = "2026-10-13T09:40:00.000Z",
    ),
    session("s-labels", updatedAt = "2026-10-13T08:03:00.000Z"),
    session(
        "s-scroll", status = DomainContract.sessionStatusEnded, updatedAt = "2026-10-13T05:41:00.000Z",
    ),
)

private val SampleIssues = listOf(
    SessionTree.Issue("i-5", "APP-5", "Reduce cold start below 800 ms"),
    SessionTree.Issue("i-14", "APP-14", "Group board issues by assignee"),
    SessionTree.Issue("i-15", "APP-15", "Batch-edit labels from the board"),
    SessionTree.Issue("i-16", "APP-16", "Fix flaky scroll restore on the issue list"),
    SessionTree.Issue("i-fb9", "FB-9", "Screenshots attached to feedback are cut off on Android"),
)

private val SampleLinks = listOf(
    SessionTree.SessionIssue("s-cold", "i-5", 0.0),
    SessionTree.SessionIssue("s-group", "i-14", 0.0),
    SessionTree.SessionIssue("s-reports", "i-fb9", 0.0),
    SessionTree.SessionIssue("s-labels", "i-15", 0.0),
    SessionTree.SessionIssue("s-scroll", "i-16", 0.0),
)
