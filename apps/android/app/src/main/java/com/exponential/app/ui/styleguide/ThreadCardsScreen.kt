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
import com.exponential.app.domain.ScriptCall
import com.exponential.app.domain.ScriptCardInput
import com.exponential.app.domain.ScriptCards
import com.exponential.app.domain.WorkCardInput
import com.exponential.app.domain.WorkCardPhaseInput
import com.exponential.app.domain.WorkCardTask
import com.exponential.app.domain.WorkCardWorkflowAgent
import com.exponential.app.domain.WorkCards
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.IssueChipSize
import com.exponential.app.ui.session.CommentCallCard
import com.exponential.app.ui.session.ScriptCardView
import com.exponential.app.ui.session.WorkCardView
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis

/** Which thread card a capture shows (one shot each). */
enum class ThreadCardsKind(val testId: String) {
    Work("work-card-screen"),
    Script("script-card-screen"),
    Comment("comment-call-card-screen"),
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

@Composable
private fun CommentCardSpecimens() {
    Caption("Commented on")
    CommentCallCard(
        issue = {
            IssueChip(
                identifier = "VAPP-84",
                title = "Exponential UI: an open generative-UI SDK",
                status = null,
                size = IssueChipSize.Sm,
            )
        },
        author = "Danny",
        caption = "via MCP · 2 minutes ago",
        mark = { AgentRunMark(agent = "claude", state = null, size = 14.dp, badgeSize = 0.dp, modifier = Modifier.size(14.dp)) },
        body = "**Status 2026-10-08, evening**\n\n#VAPP-98 (renderer hardening) is open as PR #1007; the host API " +
            "run #VAPP-91 is not based on the new contract yet, so I sent it a heads-up. The conformance " +
            "lanes are green on React and gpui; Swift and Compose wait on the fixture bump.",
        onOpen = {},
    )
    Caption("Commenting on")
    CommentCallCard(
        issue = {
            IssueChip(identifier = "VAPP-84", title = "Exponential UI", status = null, size = IssueChipSize.Sm)
        },
        author = null,
        caption = null,
        body = null,
        running = true,
    )
}
