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
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.ThreadAskDock

/** The capture's anchor (shot `sg_ask-dock`). */
const val AskDockSpecimenScreenTestId = "ask-dock-specimen-screen"

private val LONG_PLAN = listOf(
    "## Plan: one ask dock ×4",
    "",
    "1. Add `fixtures/ask-dock.json` + the TS model, mirrored ×4.",
    "2. Move waiting cards out of the transcript into the composer's slot.",
    "3. Stack several cards; a tap grows one into the big card.",
    "4. Share THE plan options: Yes · Yes, and start with a fresh context · No.",
    "5. Styleguide entry + scoped shots on every platform.",
    "6. Open the PR with the Guide.",
).joinToString("\n")

private fun plan(id: String, text: String, sessionId: String) =
    SessionTree.ThreadRow.Plan(id = id, sessionId = sessionId, plan = text, decision = null, fresh = false)

private val STACK: List<SessionTree.ThreadRow> = listOf(
    plan("p1", LONG_PLAN, "a"),
    plan("p2", "1. Group the thread list by state\n2. Drop the per-row counters", "b"),
    SessionTree.ThreadRow.Question(
        id = "q1",
        sessionId = "c",
        question = "Ship both fixes in one PR, or split them?",
        options = listOf(
            SessionTree.ThreadRow.QuestionChoice("One PR", null),
            SessionTree.ThreadRow.QuestionChoice("Split them", null),
        ),
        custom = true,
        answer = null,
        answered = false,
    ),
)

private val NAMES = mapOf("a" to "Ask dock", "b" to "Thread list", "c" to "Dock polish")
private val sessionName: (String) -> String? = { NAMES[it] }

/**
 * THE ask dock specimen (ask-dock.json ×4; web `AskDockSpecimen`): the REAL
 * [ThreadAskDock] in its three states — one waiting plan (the big card), two
 * plans + a question (the stack), one card focused out of the stack (the back
 * arrow). A capture-only screen, shot `sg_ask-dock`.
 */
@Composable
fun AskDockSpecimenScreen() {
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
                    ThreadAskDock(STACK.take(1), sessionName, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> })
                }
                SpecimenCase("Two plans and a question") {
                    ThreadAskDock(STACK, sessionName, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> })
                }
                SpecimenCase("Focused out of the stack") {
                    ThreadAskDock(
                        STACK,
                        sessionName,
                        onAnswer = { _, _, _ -> },
                        onDecidePlan = { _, _, _ -> },
                        initialFocusedId = "p2",
                    )
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
