package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import com.exponential.app.ui.components.DockSheetBody
import com.exponential.app.ui.components.DockSheetFooter
import com.exponential.app.ui.components.DockSheetSpecimenFrame
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis

/** The capture's anchor (shot `sg_dock-card`). */
const val DockCardSpecimenScreenTestId = "dock-card-specimen-screen"

private fun item(i: Int, label: String, description: String?, state: TaskQueueItemState) =
    TaskQueueItem("task:$i", TaskQueueItemKind.Task, state, "", label, description)

private val SOURCES: List<Dock.Source> = listOf(
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
    Dock.Source.Suggestions("m4", "3 suggested tasks", "1 started", "Lazy-load the markdown editor", 2),
    Dock.Source.Banner(
        id = "wall",
        icon = "ui-usage",
        label = "Usage limit",
        text = "Claude's 5-hour window is used up until 14:00",
        tone = Dock.Tint.Warning,
        action = "Switch account",
    ),
    Dock.Source.Ask("q1", AskDock.Kind.Question, "Ship both fixes in one PR, or split them?", "Dock polish", 0, 30),
)

/** EXP-1374: the session's own state cards (session-card.json `banner`, the
 *  web entry's `STATE_ROWS`): interrupted (its device went offline) and
 *  background tasks. */
private val STATE_ROWS: List<Dock.Row> = Dock.dockRows(
    listOf(SessionTree.SessionState.Interrupted, SessionTree.SessionState.Background).map { state ->
        Dock.Source.Banner(
            id = state.wire,
            icon = SessionTree.CardCopy.ICONS.getValue(state),
            label = SessionTree.CardCopy.STATES.getValue(state),
            text = if (state == SessionTree.SessionState.Interrupted) {
                SessionTree.CardCopy.BANNER_INTERRUPTED.getValue("device_offline")
            } else {
                SessionTree.CardCopy.BANNER_BACKGROUND
            },
            tone = Dock.Tint.Muted,
            action = null,
        )
    },
)

/** The banner's action word (the session host's blocked banner idiom). */
@Composable
private fun SpecimenAction(row: Dock.Row) {
    if (row.id == "banner:wall") {
        GlassPill("Switch account", onClick = {}, size = PillSize.Sm, mode = PillMode.Action)
    }
}

/**
 * THE DockCard (dock.json ×4): EXACTLY the web styleguide entry
 * (apps/styleguide entries/dock-card.tsx) drawn by the REAL [DockCard] /
 * [DockPile] / [DockSheetSpecimenFrame]: one card, a banner with its action,
 * the five cards piled, the pile fanned and the sheet frame (the pinned
 * header, the scrolling body, the pinned footer). The primitive; the
 * ask-dock specimen shows the plan and question cards on it. A capture-only
 * screen, shot `sg_dock-card`.
 */
@Composable
fun DockCardSpecimenScreen() {
    val rows = Dock.dockRows(SOURCES)
    val question = rows.first { it.kind == Dock.Kind.Question }
    val banner = rows.first { it.kind == Dock.Kind.Banner }
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(DockCardSpecimenScreenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 16.dp, vertical = 72.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Caption("One card: the label, the caption, the meta, the title, the chevron")
                DockCard(row = question, onOpen = {})
                Caption("A banner: never opens; its action word trails")
                DockCard(row = banner, action = { SpecimenAction(banner) })
                Caption("The session's own state: interrupted, background tasks")
                STATE_ROWS.forEach { row -> DockCard(row = row) }
                Caption("Five cards piled: the front one whole, the rest peeking out above it")
                DockPile(rows = rows, expanded = false, onExpandedChange = {}, onOpen = {}, action = { SpecimenAction(it) })
                Caption("The pile fanned (hover or a tap): every card, 6px apart")
                DockPile(rows = rows, expanded = true, onExpandedChange = {}, onOpen = {}, action = { SpecimenAction(it) })
                Caption("The sheet: the pinned header, the scrolling body, the pinned footer")
                DockSheetSpecimenFrame(row = question, height = 300.dp) {
                    DockSheetBody {
                        Text(
                            "Ship both fixes in one PR, or split them?",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "Choose one",
                            modifier = Modifier.padding(top = 12.dp),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                    DockSheetFooter {
                        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                            GlassPill("Answer", onClick = {}, size = PillSize.Sm, primary = true)
                        }
                    }
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
