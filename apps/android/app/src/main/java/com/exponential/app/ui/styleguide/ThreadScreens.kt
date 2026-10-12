package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.SessionDevicePresentation
import com.exponential.app.domain.ThreadView
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.components.CodingSessionRow
import com.exponential.app.ui.components.SessionRowSize
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.session.RunStatusRow
import com.exponential.app.domain.RunRowTone
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.thread.ThreadChildTabs
import com.exponential.app.ui.thread.ThreadConversation
import com.exponential.app.ui.thread.ThreadHeader
import com.exponential.app.ui.thread.ThreadMenuSheet
import com.exponential.app.ui.thread.ThreadMessageBubble
import com.exponential.app.ui.thread.ThreadReplyBar
import com.exponential.app.ui.thread.ThreadRunMeta

/** Which thread capture a screen shows (one shot each). */
enum class ThreadScreenKind(val testId: String) {
    /** sg_thread — draft 1/3: one live run, the issues its agent linked. */
    Thread("thread-screen"),

    /** sg_thread-runs — draft 2: m runs, a resume, a parallel card, the child menu. */
    ThreadRuns("thread-runs-screen"),

    /** sg_session-row — THE session row in both sizes (EXP-1248). */
    SessionRows("session-rows-screen"),
}

/**
 * EXP-1284: the styleguide's THREAD screens on Android, capture-only (the
 * ThreadCardsScreen pattern): the real thread composables ([ThreadHeader],
 * [ThreadChildTabs], [ThreadConversation], [ThreadReplyBar],
 * [ThreadMenuSheet]) over a view the REAL model ([ThreadView.build]) built
 * from fake rows matching the approved drafts — draft 1/3 for sg_thread,
 * draft 2 for sg_thread-runs. sg_session-row moved here too: the Recent
 * sheet it used to photograph lists threads now.
 */
@Composable
fun ThreadScreensScreen(kind: ThreadScreenKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            when (kind) {
                ThreadScreenKind.Thread -> ThreadSpecimen(ThreadFakes.single(), menu = false)
                ThreadScreenKind.ThreadRuns -> ThreadSpecimen(ThreadFakes.runs(), menu = true)
                ThreadScreenKind.SessionRows -> SessionRowSpecimens()
            }
        }
    }
}

@Composable
private fun ThreadSpecimen(fake: ThreadFake, menu: Boolean) {
    val view = remember(fake) { ThreadView.build(fake.thread, fake.items, fake.issues, fake.runs) }
    val status = fake.statuses
    Column(modifier = Modifier.fillMaxSize().statusBarsPadding()) {
        ThreadHeader(title = view.title, caption = view.headerCaption, onBack = {})
        ThreadChildTabs(
            strip = view.strip,
            selectedKey = "conversation",
            onSelect = {},
            onAdd = {},
            issueStatus = { status[it] },
            // The child the agent just linked wears its ring (draft 1).
            fresh = { it == fake.freshIssueId },
        )
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            ThreadConversation(
                view = view,
                runMeta = { fake.meta[it] },
                issueStatus = { status[it] },
                onOpenIssue = {},
                onOpenRun = {},
                parallelCaption = { row -> listOfNotNull(row.deviceLabel, fake.ages[row.runId]).joinToString(" · ") },
                issueTitle = { id -> fake.issues.firstOrNull { it.id == id }?.let { it.identifier to it.title } },
                runLead = { block -> fake.leads[block.runId]?.let { ThreadMessageBubble(it) } },
                runBody = { block, _ ->
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        fake.bodies[block.runId]?.let { MarkdownView(markdown = it) }
                        fake.statusRows[block.runId]?.let { (caption, tool) ->
                            RunStatusRow(
                                agent = "claude",
                                markState = CodingSessionDisplayState.Working,
                                ended = false,
                                caption = caption,
                                tone = RunRowTone.Muted,
                                toolLine = tool,
                                showWork = false,
                                onToggle = null,
                            )
                        }
                    }
                },
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 96.dp),
            )
            ThreadReplyBar(
                value = "",
                onValueChange = {},
                onSend = {},
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding(),
            )
        }
    }
    if (menu) {
        val issue = view.strip.filterIsInstance<ThreadView.Child.IssueChild>().lastOrNull()
        ThreadMenuSheet(
            issue = issue,
            canStart = true,
            canFile = true,
            onLink = {},
            onNewIssue = {},
            onStartRun = {},
            onUnlink = {},
            onDismiss = {},
        )
    }
}

/** The rows one capture draws, fed through the REAL model. */
private data class ThreadFake(
    val thread: ThreadView.Thread,
    val items: List<ThreadView.Item>,
    val issues: List<ThreadView.Issue>,
    val runs: List<ThreadView.Run>,
    val statuses: Map<String, com.exponential.app.domain.ResolvedIssueStatus>,
    val meta: Map<String, ThreadRunMeta>,
    val leads: Map<String, String> = emptyMap(),
    val bodies: Map<String, String> = emptyMap(),
    /** run id → (status caption, tool line). */
    val statusRows: Map<String, Pair<String, String?>> = emptyMap(),
    val ages: Map<String, String> = emptyMap(),
    val freshIssueId: String? = null,
)

private object ThreadFakes {
    private fun status(key: IssueStatus) = IssueStatusResolver.builtinDefaults.first { it.builtinKey == key }

    private fun item(id: String, kind: String, target: String, sort: Double, at: String, source: String = "user", by: String? = null) =
        ThreadView.Item(
            id = id,
            kind = kind,
            issueId = target.takeIf { kind == "issue" },
            sessionId = target.takeIf { kind == "run" },
            source = source,
            addedBySessionId = by,
            sortOrder = sort,
            createdAt = at,
        )

    private fun run(
        id: String,
        name: String,
        device: String,
        status: String,
        busy: Boolean,
        at: String,
        parent: String? = null,
        resumedFrom: String? = null,
        issueId: String? = null,
    ) = ThreadView.Run(
        id = id, name = name, userId = "me", deviceLabel = device, status = status, needsInput = false,
        agentBusy = busy, issueId = issueId, parentSessionId = parent, resumedFromId = resumedFrom,
        startedAt = at, endedAt = null,
    )

    /** Draft 1/3: Run 1 live on macbook; it linked EXP-1276 and filed EXP-1277. */
    fun single(): ThreadFake = ThreadFake(
        thread = ThreadView.Thread("t1", null, null, "2026-10-12T21:14:00Z"),
        items = listOf(
            item("i-r1", "run", "r1", 1.0, "2026-10-12T21:00:00Z"),
            item("i-a", "issue", "a", 2.0, "2026-10-12T21:02:00Z", source = "agent", by = "r1"),
            item("i-b", "issue", "b", 3.0, "2026-10-12T21:04:00Z", source = "agent", by = "r1"),
        ),
        issues = listOf(
            ThreadView.Issue("a", "EXP-1276", "ios keyboard", "2026-10-11T09:00:00Z"),
            ThreadView.Issue("b", "EXP-1277", "ios session error", "2026-10-12T21:03:40Z"),
        ),
        runs = listOf(run("r1", "iOS keyboard and model picker", "macbook", "running", busy = true, at = "2026-10-12T21:00:00Z")),
        statuses = mapOf("a" to status(IssueStatus.InProgress), "b" to status(IssueStatus.Backlog)),
        meta = mapOf("r1" to ThreadRunMeta("claude", busy = true, name = "iOS keyboard and model picker")),
        leads = mapOf(
            "r1" to "the keyboard on ios doesn't close when i slide it down. also i think the model picker is empty",
        ),
        bodies = mapOf(
            "r1" to "Two separate bugs. I'll file the model picker one and keep both in this thread.\n\n" +
                "The keyboard fix is a one-button accessory bar over the keyboard. The picker is empty " +
                "because the account list loads after the sheet opens; it now waits on it.\n\n" +
                "Both fixes are in one PR. Tests are running.",
        ),
        statusRows = mapOf("r1" to ("Building on macbook · 6m" to "xcodebuild test ExpUI")),
        freshIssueId = "b",
    )

    /** Draft 2: Run 1 ended, Run 2 its resume (ended), Runs 3 and 4 = live
     *  children of Run 2 on two machines, three issues, a Guide. */
    fun runs(): ThreadFake = ThreadFake(
        thread = ThreadView.Thread("t2", "UI batch plan", """[{"topic":"Summary","text":"The batch plan."}]""", "2026-10-12T21:10:00Z"),
        items = listOf(
            item("i-r1", "run", "r1", 1.0, "2026-10-12T18:00:00Z"),
            item("i-x", "issue", "x", 2.0, "2026-10-12T18:10:00Z"),
            item("i-y", "issue", "y", 3.0, "2026-10-12T18:20:00Z"),
            item("i-z", "issue", "z", 4.0, "2026-10-12T18:30:00Z"),
            item("i-r2", "run", "r2", 5.0, "2026-10-12T19:00:00Z"),
            item("i-r3", "run", "r3", 6.0, "2026-10-12T20:48:00Z"),
            item("i-r4", "run", "r4", 7.0, "2026-10-12T21:01:00Z"),
        ),
        issues = listOf(
            ThreadView.Issue("x", "EXP-1259", "UX batch 2", "2026-10-10T09:00:00Z"),
            ThreadView.Issue("y", "EXP-1284", "Threads", "2026-10-11T09:00:00Z"),
            ThreadView.Issue("z", "EXP-1270", "getting started", "2026-10-11T10:00:00Z"),
        ),
        runs = listOf(
            run("r1", "Labeled 165 issues, drafted the batch, filed EXP-1284", "macbook", "ended", false, "2026-10-12T18:00:00Z"),
            run("r2", "Applied the decisions, rewrote the plan", "macbook", "ended", false, "2026-10-12T19:00:00Z", resumedFrom = "r1"),
            run("r3", "Threads schema + MCP", "mint", "running", true, "2026-10-12T20:48:00Z", parent = "r2", issueId = "y"),
            run("r4", "Getting started cards", "macbook", "running", true, "2026-10-12T21:01:00Z", parent = "r2", issueId = "z"),
        ),
        statuses = mapOf(
            "x" to status(IssueStatus.Backlog),
            "y" to status(IssueStatus.Backlog),
            "z" to status(IssueStatus.Backlog),
        ),
        meta = mapOf(
            "r1" to ThreadRunMeta("claude", busy = false, name = "Labeled 165 issues, drafted the batch, filed EXP-1284"),
            "r2" to ThreadRunMeta("claude", busy = false, name = "Applied the decisions, rewrote the plan"),
            "r3" to ThreadRunMeta("claude", busy = true, name = "Threads schema + MCP"),
            "r4" to ThreadRunMeta("claude", busy = true, name = "Getting started cards"),
        ),
        ages = mapOf("r3" to "22m", "r4" to "9m"),
    )
}

/**
 * sg_session-row: THE session row in both sizes (EXP-1248) on fake runs — a
 * working parent with a nested child, an issue run ready for review and a
 * finished one (big), then the sidebar's small rows.
 */
@Composable
private fun SessionRowSpecimens() {
    val now = remember { java.time.Instant.now() }
    fun session(
        id: String,
        status: String,
        title: String,
        parent: String? = null,
        busy: Boolean = false,
        issueId: String? = null,
        prState: String? = null,
        minutes: Long = 12,
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team",
        userId = "me",
        status = status,
        agent = "claude",
        agentBusy = busy,
        actionName = if (issueId == null) "Chat" else null,
        agentTitle = title.takeIf { issueId == null },
        parentSessionId = parent,
        deviceLabel = "Alex's MacBook Pro",
        prState = prState,
        startedAt = now.minusSeconds(minutes * 60).toString(),
        createdAt = now.minusSeconds(minutes * 60).toString(),
        updatedAt = now.toString(),
        endedAt = if (status == "ended") now.minusSeconds(3_600).toString() else null,
    )
    val issue = com.exponential.app.data.db.IssueEntity(
        id = "i-14",
        boardId = "board",
        number = 14,
        identifier = "APP-14",
        title = "Group board issues by assignee",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prState = "open",
        createdAt = now.toString(),
        updatedAt = now.toString(),
    )
    val device = SessionDevicePresentation(label = "Alex's MacBook Pro", online = true)
    val guides = TreeGuides.compute(listOf(0, 1, 0, 0))
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 72.dp),
    ) {
        CodingSessionRow(session("s1", "running", "Threads schema + MCP", busy = true, minutes = 22), null, device, SessionRowSize.Big, onClick = {}, depth = 0, guide = guides[0])
        CodingSessionRow(session("s2", "running", "Getting started cards", parent = "s1", busy = true, minutes = 9), null, device, SessionRowSize.Big, onClick = {}, depth = 1, guide = guides[1])
        CodingSessionRow(session("s3", "in_review", "", issueId = issue.id, prState = "open"), issue, device, SessionRowSize.Big, onClick = {}, depth = 0, guide = guides[2])
        CodingSessionRow(session("s4", "ended", "Label and relate every open issue"), null, device, SessionRowSize.Big, onClick = {}, depth = 0, guide = guides[3])
        androidx.compose.foundation.layout.Spacer(Modifier.padding(top = 24.dp))
        CodingSessionRow(session("s5", "running", "iOS keyboard and model picker", busy = true), null, device, SessionRowSize.Small, onClick = {})
        CodingSessionRow(session("s6", "ended", "UI batch plan"), null, device, SessionRowSize.Small, onClick = {})
    }
}
