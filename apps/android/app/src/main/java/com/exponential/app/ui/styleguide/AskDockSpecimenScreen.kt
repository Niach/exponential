package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.Dock
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TaskQueue
import com.exponential.app.domain.TaskQueueItem
import com.exponential.app.domain.TaskQueueItemKind
import com.exponential.app.domain.TaskQueueItemState
import com.exponential.app.domain.TaskQueueSection
import com.exponential.app.ui.components.DockCard
import com.exponential.app.ui.components.DockPile
import com.exponential.app.ui.components.DockSheetSpecimenFrame
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.PlanAskCard

/** The capture's anchor (shot `sg_ask-dock`). */
const val AskDockSpecimenScreenTestId = "ask-dock-specimen-screen"

private val LONG_PLAN = listOf(
    "## Plan: one Dock ×4",
    "",
    "1. Add `fixtures/dock.json` + the TS model, mirrored ×4.",
    "2. Float the waiting cards above the composer; the transcript is the one scroll.",
    "3. Pile several cards like the toasts; a tap opens one as a sheet.",
    "4. Share THE plan options: Yes · Yes, and start with a fresh context · No.",
    "5. Styleguide entry + scoped shots on every platform.",
    "6. Open the PR with the Guide.",
).joinToString("\n")

private val QUESTIONS = listOf(
    SessionTree.ThreadRow.PlanQuestion(
        id = "q-scope",
        prompt = "Which cards move into the Dock first?",
        multi = false,
        custom = true,
        options = listOf(
            SessionTree.ThreadRow.QuestionChoice("Plan and question cards", "The rest follows as its own issue"),
            SessionTree.ThreadRow.QuestionChoice("Everything", "Plan, question, tasks, queued messages, banners"),
        ),
    ),
    SessionTree.ThreadRow.PlanQuestion(
        id = "q-eui",
        prompt = "Should DockCard also go into Exponential UI?",
        multi = false,
        custom = false,
        options = listOf(
            SessionTree.ThreadRow.QuestionChoice("Yes, as DockCard", null),
            SessionTree.ThreadRow.QuestionChoice("Styleguide only for now", null),
        ),
    ),
)

private val RICH_PLAN = SessionTree.ThreadRow.Plan(
    id = "p1",
    sessionId = "a",
    plan = LONG_PLAN,
    decision = null,
    fresh = false,
    questions = QUESTIONS,
)

private val ASKS: List<Dock.Source> = listOf(
    Dock.Source.Ask("p1", AskDock.Kind.Plan, LONG_PLAN, "Ask dock", 2, 10),
    Dock.Source.Ask("q1", AskDock.Kind.Question, "Ship both fixes in one PR, or split them?", "Dock polish", 0, 30),
)

private fun item(i: Int, label: String, description: String?, state: TaskQueueItemState) =
    TaskQueueItem("task:$i", TaskQueueItemKind.Task, state, "", label, description)

private val STATE: List<Dock.Source> = listOf(
    Dock.Source.Queued(
        listOf(
            Dock.QueuedMessage("m1", "also check the watchdog once the root is idle"),
            Dock.QueuedMessage("m2", "then open the PR"),
        ),
    ),
    Dock.Source.Tasks(
        TaskQueue(
            todo = TaskQueueSection(
                "3 to do",
                "",
                listOf(
                    item(0, "Measuring on the Galaxy A54", "2m 35s", TaskQueueItemState.Running),
                    item(1, "Rerun the web captures", null, TaskQueueItemState.Pending),
                    item(2, "Open the PR with the numbers", null, TaskQueueItemState.Pending),
                ),
            ),
            completed = TaskQueueSection("2 completed", "", emptyList()),
        ),
    ),
)

/**
 * THE Dock specimen (dock.json + ask-dock.json ×4; web `AskDockSpecimen`):
 * the DockCards at rest in their four states drawn by the REAL [DockCard] /
 * [DockPile] / [DockSheetSpecimenFrame]: one waiting plan, the pile (a plan,
 * a question, THE Queue, the queued messages), the pile fanned, and the
 * plan's sheet ([PlanAskCard] in sheet mode). A capture-only screen, shot
 * `sg_ask-dock`.
 */
@Composable
fun AskDockSpecimenScreen() {
    val single = Dock.dockRows(ASKS.take(1))
    val pile = Dock.dockRows(STATE + ASKS)
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(AskDockSpecimenScreenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 72.dp),
                verticalArrangement = Arrangement.spacedBy(24.dp),
            ) {
                SpecimenCase("One waiting plan") {
                    DockCard(row = single[0], onOpen = {})
                }
                SpecimenCase("A plan, a question, the Queue and the queued messages, piled") {
                    DockPile(rows = pile, expanded = false, onExpandedChange = {}, onOpen = {})
                }
                SpecimenCase("The pile fanned (hover or a tap)") {
                    DockPile(rows = pile, expanded = true, onExpandedChange = {}, onOpen = {})
                }
                SpecimenCase("The plan's sheet") {
                    DockSheetSpecimenFrame(row = single[0], height = 420.dp) { scope ->
                        PlanAskCard(RICH_PLAN, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> }, sheet = scope)
                    }
                }
            }
        }
    }
}

@Composable
private fun SpecimenCase(label: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        content()
    }
}
