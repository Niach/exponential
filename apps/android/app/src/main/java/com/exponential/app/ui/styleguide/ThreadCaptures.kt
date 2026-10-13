package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.thread.SessionCard
import com.exponential.app.ui.thread.ThreadAgent
import com.exponential.app.ui.thread.ThreadComposerBox
import com.exponential.app.ui.thread.ThreadContent
import com.exponential.app.ui.thread.ThreadRowMeta
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Which thread capture to draw (one shot each). */
enum class ThreadCaptureKind(val testId: String) {
    /** sg_thread: a thread on a phone (draft 3's content). */
    Thread("thread-capture-screen"),

    /** sg_thread-runs: the Triage agent channel (draft 5). */
    AgentChannel("agent-channel-capture-screen"),

    /** sg_session-row: THE session card, one per state. */
    SessionCards("session-cards-capture-screen"),
}

/**
 * EXP-1296: the thread screen as capture-only screens — the REAL
 * [ThreadContent] / [SessionCard] over the drafts' data built through the
 * real domain model ([SessionTree.buildSessionThread],
 * [SessionTree.sessionCard]). The issue + Guide pages are empty slots (the
 * capture shows the Conversation page).
 */
@Composable
fun ThreadCaptures(kind: ThreadCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            when (kind) {
                ThreadCaptureKind.Thread -> ThreadCapture(
                    view = SessionTree.buildSessionThread(
                        root = ThreadRoot,
                        sessions = listOf(ThreadRoot, ThreadChild),
                        sessionIssues = ThreadLinks,
                        issues = ThreadIssues,
                        messages = ThreadMessages,
                        pullRequests = emptyList(),
                        now = CaptureNow,
                    ),
                    agent = null,
                    placeholder = SessionTree.ThreadCopy.PLACEHOLDER,
                )
                ThreadCaptureKind.AgentChannel -> ThreadCapture(
                    view = SessionTree.buildSessionThread(
                        root = TriageRoot,
                        sessions = listOf(TriageRoot, Fb9Root),
                        sessionIssues = emptyList(),
                        issues = ThreadIssues,
                        messages = TriageMessages,
                        pullRequests = emptyList(),
                        now = ChannelNow,
                    ),
                    agent = ThreadAgent(name = "Triage", icon = "bug", color = "#a78bfa"),
                    placeholder = "Message Triage",
                )
                ThreadCaptureKind.SessionCards -> Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .statusBarsPadding()
                        .padding(horizontal = 16.dp, vertical = 72.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    CardStates.forEachIndexed { index, session ->
                        SessionCard(SessionTree.sessionCard(session, index + 1, CaptureNow), onClick = {})
                    }
                }
            }
        }
    }
}

@Composable
private fun ThreadCapture(view: SessionTree.ThreadView, agent: ThreadAgent?, placeholder: String) {
    var selected by remember { mutableStateOf("conversation") }
    var draft by remember { mutableStateOf("") }
    ThreadContent(
        view = view,
        agent = agent,
        selectedKey = selected,
        onSelectKey = { selected = it },
        issueStatus = { CaptureStatuses[it] },
        meta = { row -> CaptureMeta[row.id] },
        onOpenCard = {},
        onAnswer = { _, _ -> },
        onDecidePlan = { _, _ -> },
        canUnqueue = { true },
        onUnqueue = {},
        onBack = {},
        onRename = {},
        onAdd = {},
        onIssueMenu = {},
        onAgentSettings = {},
        composer = { modifier ->
            ThreadComposerBox(
                value = draft,
                onValueChange = { draft = it },
                placeholder = placeholder,
                pendingImages = emptyList(),
                imageError = null,
                sending = false,
                onAdd = {},
                onRemoveImage = {},
                onSend = {},
                modifier = modifier,
            )
        },
        issuePage = { _, _ -> },
        guidePage = { _ -> },
    )
}

// ── fake data (the drafts: Acme, APP-5, Triage) ──────────────────────────────

private const val CaptureNow = "2026-10-13T20:04:00.000Z"

/** The Triage channel is photographed mid-morning, minutes after its last reply. */
private const val ChannelNow = "2026-10-13T09:20:00.000Z"
private const val CaptureTeam = "t-acme"
private const val Alex = "u-alex"

private fun captureSession(
    id: String,
    rootId: String = id,
    parentId: String? = null,
    kind: String = DomainContract.sessionKindThread,
    status: String = DomainContract.sessionStatusRunning,
    busy: Boolean = false,
    needsInput: Boolean = false,
    blocked: SessionTree.Blocked? = null,
    title: String? = null,
    name: String? = null,
    icon: String? = null,
    color: String? = null,
    guide: List<SessionTree.GuideEntry>? = null,
    startedAt: String? = "2026-10-13T19:56:00.000Z",
    endedAt: String? = null,
    createdAt: String = "2026-10-13T19:56:00.000Z",
) = SessionTree.Session(
    id = id,
    teamId = CaptureTeam,
    parentId = parentId,
    rootId = rootId,
    kind = kind,
    userId = Alex,
    status = status,
    busy = busy,
    needsInput = needsInput,
    blocked = blocked,
    shared = true,
    title = title,
    guide = guide,
    actionName = null,
    name = name,
    icon = icon,
    color = color,
    sortOrder = 0.0,
    startedAt = startedAt,
    endedAt = endedAt,
    createdAt = createdAt,
    updatedAt = endedAt ?: createdAt,
)

private fun captureMessage(
    id: String,
    sessionId: String,
    author: String,
    body: String = "",
    card: JsonObject? = null,
    createdAt: String,
    consumed: Boolean = true,
) = SessionTree.Message(
    id = id,
    sessionId = sessionId,
    authorKind = author,
    authorId = if (author == DomainContract.messageAuthorKindUser) Alex else null,
    body = body,
    card = card,
    answeredAt = null,
    consumedAt = if (consumed) createdAt else null,
    createdAt = createdAt,
)

private fun card(vararg pairs: Pair<String, Any>): JsonObject = JsonObject(
    pairs.associate { (key, value) ->
        key to when (value) {
            is List<*> -> JsonArray(value.map { JsonPrimitive(it.toString()) })
            else -> JsonPrimitive(value.toString())
        }
    },
)

private val ThreadRoot = captureSession(
    id = "s-root",
    guide = listOf(SessionTree.GuideEntry(text = "Board snapshot cache")),
)

private val ThreadChild = captureSession(
    id = "s-profile",
    rootId = "s-root",
    parentId = "s-root",
    title = "Profile the launch path",
    needsInput = true,
    startedAt = "2026-10-13T19:57:00.000Z",
    createdAt = "2026-10-13T19:57:00.000Z",
)

private val ThreadIssues = listOf(
    SessionTree.Issue("i-app5", "APP-5", "Reduce cold start below 800 ms"),
    SessionTree.Issue("i-app10", "APP-10", "Migrate image cache to on-disk LRU"),
    SessionTree.Issue("i-fb3", "FB-3", "Syntax highlighting ships every language"),
)

private val ThreadLinks = ThreadIssues.mapIndexed { index, issue ->
    SessionTree.SessionIssue("s-root", issue.id, index.toDouble())
}

private val ThreadMessages = listOf(
    captureMessage(
        "m-ask", "s-root", DomainContract.messageAuthorKindUser,
        body = "Cold start is at 1.4s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of the critical path.",
        createdAt = "2026-10-13T19:56:10.000Z",
    ),
    captureMessage(
        "m-card", "s-root", DomainContract.messageAuthorKindAgent,
        card = card("kind" to DomainContract.messageCardKindSession, "sessionId" to "s-profile"),
        createdAt = "2026-10-13T19:57:00.000Z",
    ),
    captureMessage(
        "m-reply", "s-root", DomainContract.messageAuthorKindAgent,
        body = "**Board snapshot cache**\nThe last rendered board is written to disk on every sync and painted on launch, " +
            "then replaced in place once the shape catches up.\n\nDeferred the shape subscribe until after the first frame: " +
            "740 ms on the CI device, under the 800 ms target. Filed #FB-3 for the editor's highlighting bundle.",
        createdAt = "2026-10-13T20:01:00.000Z",
    ),
    captureMessage(
        "m-filed", "s-root", DomainContract.messageAuthorKindSystem,
        card = card("kind" to DomainContract.messageCardKindEvent, "event" to DomainContract.messageEventKindIssueFiled, "issueId" to "i-fb3"),
        createdAt = "2026-10-13T20:01:30.000Z",
    ),
    captureMessage(
        "m-question", "s-root", DomainContract.messageAuthorKindAgent,
        card = card(
            "kind" to DomainContract.messageCardKindQuestion,
            "question" to "Cold start is at 740ms (target <800ms). Lazy-load the markdown editor too, or open the PR with what we have?",
            "options" to listOf("Also lazy-load the markdown editor", "Open the PR with what we have"),
        ),
        createdAt = "2026-10-13T20:02:00.000Z",
    ),
)

private val TriageRoot = captureSession(
    id = "s-triage",
    kind = DomainContract.sessionKindAgent,
    status = DomainContract.sessionStatusIdle,
    name = "Triage",
    icon = "bug",
    color = "#a78bfa",
    createdAt = "2026-10-13T09:00:00.000Z",
    startedAt = "2026-10-13T09:00:00.000Z",
)

private val Fb9Root = captureSession(
    id = "s-fb9",
    title = "FB-9 Sort today's widget reports",
    busy = true,
    startedAt = "2026-10-13T09:02:00.000Z",
    createdAt = "2026-10-13T09:02:00.000Z",
)

private val TriageMessages = listOf(
    captureMessage(
        "t-1", "s-triage", DomainContract.messageAuthorKindAgent,
        body = "Two new widget reports on Product Feedback. FB-8 is a duplicate of FB-3, merged it. FB-9 needs code, so I started on it:",
        createdAt = "2026-10-13T09:02:00.000Z",
    ),
    captureMessage(
        "t-2", "s-triage", DomainContract.messageAuthorKindAgent,
        card = card("kind" to DomainContract.messageCardKindThreadStarted, "sessionId" to "s-fb9"),
        createdAt = "2026-10-13T09:02:05.000Z",
    ),
    captureMessage(
        "t-3", "s-triage", DomainContract.messageAuthorKindUser,
        body = "Label the Android ones too, and check the board every hour.",
        createdAt = "2026-10-13T09:14:00.000Z",
    ),
    captureMessage(
        "t-4", "s-triage", DomainContract.messageAuthorKindAgent,
        body = "Done, 3 issues labelled android.",
        createdAt = "2026-10-13T09:14:30.000Z",
    ),
)

/** The rows' author + clock, as the screen resolves them off users. */
private val CaptureMeta: Map<String, ThreadRowMeta> = mapOf(
    "m-ask" to ThreadRowMeta("Alex Carter", "19:56"),
    "t-1" to ThreadRowMeta("", "09:02"),
    "t-3" to ThreadRowMeta("Alex Carter", "09:14"),
    "t-4" to ThreadRowMeta("", "09:14"),
)

private val CaptureStatuses = run {
    val builtins = IssueStatusResolver.builtinDefaults
    val inProgress = builtins.first { it.builtinKey == IssueStatus.InProgress }
    val backlog = builtins.first { it.builtinKey == IssueStatus.Backlog }
    mapOf("i-app5" to inProgress, "i-app10" to backlog, "i-fb3" to backlog)
}

/** One session per card state: queued, working, waiting, needs you, usage
 *  limit, ended. */
private val CardStates = listOf(
    captureSession("c-1", title = "Profile the launch path", status = DomainContract.sessionStatusQueued, startedAt = null),
    captureSession("c-2", title = "Defer the shape subscribe", busy = true, startedAt = "2026-10-13T19:51:26.000Z"),
    captureSession("c-3", title = "Board snapshot cache", startedAt = "2026-10-13T19:40:00.000Z"),
    captureSession("c-4", title = "Lazy-load the markdown editor", needsInput = true, startedAt = "2026-10-13T19:58:57.000Z"),
    captureSession(
        "c-5",
        title = "Migrate image cache",
        blocked = SessionTree.Blocked(window = "five_hour", resetsAt = "2026-10-13T22:00:00.000Z"),
        startedAt = "2026-10-13T18:59:00.000Z",
    ),
    captureSession(
        "c-6",
        status = DomainContract.sessionStatusEnded,
        startedAt = "2026-10-13T17:00:00.000Z",
        endedAt = "2026-10-13T17:37:00.000Z",
    ),
)
