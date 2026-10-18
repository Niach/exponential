package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ScriptCall
import com.exponential.app.domain.ScriptCardInput
import com.exponential.app.domain.ScriptCards
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WorkCardInput
import com.exponential.app.domain.WorkCardPhaseInput
import com.exponential.app.domain.WorkCardTask
import com.exponential.app.domain.WorkCardWorkflowAgent
import com.exponential.app.domain.WorkCards
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.session.CommentCallCard
import com.exponential.app.ui.session.ScriptCardView
import com.exponential.app.ui.session.WorkCardView
import com.exponential.app.ui.thread.SessionCard
import com.exponential.app.ui.thread.ThreadReportLine
import com.exponential.app.ui.thread.ThreadSuggestionActions
import com.exponential.app.ui.thread.ThreadSuggestionsCard
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis

/** Which thread card a capture shows (one shot each). */
enum class ThreadCardsKind(val testId: String) {
    Work("work-card-screen"),
    Script("script-card-screen"),
    Comment("comment-call-card-screen"),
    /** EXP-1323: the suggested tasks card in every state, a report line and
     *  a session card with its live caption. */
    Suggestions("thread-suggestions-screen"),
    /** EXP-1280: THE rich plan / question card (plan-card.json, shot sg_plan-card). */
    PlanCard("plan-card-screen"),
}

/**
 * EXP-1240 / EXP-1279 / EXP-1241 (EXP-1259 D12-D14): the styleguide's run
 * thread cards on Android, a capture-only screen (sg_work-card,
 * sg_script-card, sg_comment-call-card) drawing each card with its REAL
 * composable over the draft's (draft7) data: a workflow mid-review, a task
 * list and a subagent; a script with grouped calls and a failure; a comment
 * call with the clamped body.
 */
@Composable
fun ThreadCardsScreen(kind: ThreadCardsKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 72.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (kind) {
                    ThreadCardsKind.Work -> WorkCardSpecimens()
                    ThreadCardsKind.Script -> ScriptCardSpecimens()
                    ThreadCardsKind.Comment -> CommentCardSpecimens()
                    ThreadCardsKind.Suggestions -> SuggestionsSpecimens()
                    ThreadCardsKind.PlanCard -> PlanCardSpecimens()
                }
            }
        }
    }
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

@Composable
private fun WorkCardSpecimens() {
    Caption("Workflow")
    WorkCardView(card = WorkCards.of(SampleWorkflow), onOpenLane = {})
    Caption("Tasks")
    WorkCardView(
        card = WorkCards.of(
            WorkCardInput.Tasks(
                listOf("Survey", "Draft", "Fixtures").map { WorkCardTask(it, "completed") } +
                    WorkCardTask("Wire the Review face", "in_progress") +
                    listOf("Desktop", "iOS", "Android").map { WorkCardTask(it, "pending") },
            ),
        ),
    )
    Caption("Subagent")
    WorkCardView(
        card = WorkCards.of(
            WorkCardInput.Subagent(title = "Survey UI code", state = "done", toolCalls = 127, elapsedMs = 600_000),
        ),
        onOpenSubagent = {},
    )
}

private val SampleWorkflow = WorkCardInput.Workflow(
    name = "exponential-ui-renderer-hardening",
    status = "running",
    elapsedMs = 4_320_000,
    phases = listOf("Contract", "Implement", "Conformance", "Review", "Verify")
        .mapIndexed { index, title -> WorkCardPhaseInput(index + 1, title) },
    agents = listOf(
        WorkCardWorkflowAgent(1, "contract", 1, "opus", "done", 600_000),
        WorkCardWorkflowAgent(2, "impl:react", 2, "opus", "done", 1_200_000),
        WorkCardWorkflowAgent(3, "impl:gpui", 2, "opus", "done", 1_500_000),
        WorkCardWorkflowAgent(4, "conf:swift", 3, "opus", "done", 480_000),
        WorkCardWorkflowAgent(5, "conf:compose", 3, "opus", "done", 300_000),
        WorkCardWorkflowAgent(6, "review:catalog", 4, "claude-opus-5-5", "done", 509_000),
        WorkCardWorkflowAgent(7, "fix:react", 4, "opus", "done", 1_900_000),
        WorkCardWorkflowAgent(8, "recheck:core", 4, "opus", "done", 103_000),
        WorkCardWorkflowAgent(9, "recheck:react", 4, "opus", "running"),
    ),
)

@Composable
private fun ScriptCardSpecimens() {
    val calls = List(55) { ScriptCall("exponential.exponential_issues_get", "EXP-${1253 + it}", ok = true, ms = 40) } +
        List(11) { ScriptCall("exponential.exponential_issues_update", "EXT-${1 + it}", ok = true, ms = 60) } +
        List(13) { ScriptCall("exponential.exponential_comments_create", "MKT-${6 + it}", ok = true, ms = 80) } +
        List(18) { ScriptCall("exponential.exponential_issue_relations_add", "EXT-${15 + it}", ok = it != 3, ms = 70) } +
        ScriptCall("computer.screenshot", ok = true, ms = 300)
    Caption("Ran a script")
    ScriptCardView(
        card = ScriptCards.of(ScriptCardInput(running = false, count = calls.size, calls = calls, elapsedMs = 3_040)),
        source = "const issues = await Promise.all(ids.map((id) => tools.exponential.exponential_issues_get({ id })))",
    )
    Caption("Running")
    ScriptCardView(card = ScriptCards.of(ScriptCardInput(running = true)), running = true)
}

private val InProgress = IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.InProgress }

@Composable
private fun CommentCardSpecimens() {
    Caption("Commented on")
    CommentCallCard(
        issue = {
            IssueChip(
                identifier = "VAPP-84",
                title = "Exponential UI: an open generative-UI SDK",
                status = InProgress,
            )
        },
        author = "Danny",
        caption = "via MCP · 2 minutes ago",
        mark = { AgentRunMark(agent = "claude", state = null, size = 14.dp, badgeSize = 0.dp, modifier = Modifier.size(14.dp)) },
        body = "**Status 2026-10-08, evening**\n\n#VAPP-98 (renderer hardening) is open as PR #1007; the host API " +
            "session #VAPP-91 is not based on the new contract yet, so I sent it a heads-up. The conformance " +
            "lanes are green on React and gpui; Swift and Compose wait on the fixture bump.",
        onOpen = {},
    )
    Caption("Commenting on")
    CommentCallCard(
        issue = {
            IssueChip(identifier = "VAPP-84", title = "Exponential UI", status = InProgress)
        },
        author = null,
        caption = null,
        body = null,
        running = true,
    )
}

private fun specimenCard(id: String, name: String, state: SessionTree.SessionState, elapsed: String?, caption: String?) =
    SessionTree.SessionCard(
        sessionId = id,
        icon = SessionTree.CardCopy.ICONS.getValue(state),
        name = name,
        kindWord = SessionTree.CardCopy.STATES.getValue(state),
        state = state,
        tone = SessionTree.CardCopy.TONES.getValue(state),
        elapsed = elapsed,
        running = state == SessionTree.SessionState.Working,
        testId = SessionTree.CardCopy.TEST_ID_CARD,
        caption = caption,
    )

/** EXP-1323: the `session-thread.json` live dispatcher case, drawn by the
 *  real thread composables (read-only actions: nothing is sent). */
@Composable
private fun SuggestionsSpecimens() {
    val actions = ThreadSuggestionActions(
        onStart = { _, _, _, _ -> },
        onStartAll = {},
        onHere = { _, _ -> },
        onDismiss = { _, _ -> },
        onFile = { _, _ -> },
        models = DomainContract.codingModelValues.map { it to modelLabel(it) },
    )
    Caption("Session")
    SessionCard(
        card = specimenCard("r", "Speed up the shop", SessionTree.SessionState.Working, "9m 00s", "Dispatching"),
        onClick = {},
    )
    Caption("Suggested tasks")
    ThreadSuggestionsCard(
        row = SessionTree.ThreadRow.Suggestions(
            id = "m2",
            sessionId = "r",
            header = "5 suggested tasks · 2 started",
            startAll = false,
            items = listOf(
                SessionTree.SuggestionRow(
                    id = "s1",
                    title = "Add width and height to cart thumbnails",
                    text = "Avoid layout shift in the cart.",
                    state = SessionTree.SuggestionState.Started,
                    card = specimenCard(
                        "c1", "Add width and height to cart thumbnails", SessionTree.SessionState.Working,
                        "6m 00s", "Reading CartLineItem.tsx",
                    ),
                    issueId = null,
                    identifier = null,
                ),
                SessionTree.SuggestionRow(
                    id = "s2",
                    title = "Lazy-load product images",
                    text = "Below-the-fold images load late.",
                    state = SessionTree.SuggestionState.Started,
                    card = specimenCard("c2", "Lazy-load product images", SessionTree.SessionState.Ended, "3m 00s", null),
                    issueId = null,
                    identifier = null,
                ),
                SessionTree.SuggestionRow(
                    "s3", "Cache headers", "Long-cache the static assets.",
                    SessionTree.SuggestionState.Filed, null, "i-9", "EXP-9",
                ),
                SessionTree.SuggestionRow(
                    "s4", "Measure LCP", "Run Lighthouse here.",
                    SessionTree.SuggestionState.Here, null, null, null,
                ),
                SessionTree.SuggestionRow(
                    "s6", "Preload the hero font", "Saves a round trip.",
                    SessionTree.SuggestionState.Open, null, null, null,
                ),
            ),
        ),
        actions = actions,
        onOpenSession = {},
    )
    Caption("Report")
    ThreadReportLine(text = "Report from Lazy-load product images", onOpen = {})
}
