package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.ThreadPage
import com.exponential.app.ui.components.DEFAULT_AGENT
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.thread.ThreadCopy
import com.exponential.app.ui.thread.ThreadListRow
import com.exponential.app.ui.thread.ThreadListRowModel

/**
 * EXP-1284: lists show THREADS, not bare runs — a row is the thread's title
 * over its `listCaption`, the live run's mark (spinner while busy, amber
 * while it waits) or the conversation glyph, and it opens the thread page.
 * Grouped by `partitionThreadList`: Running = a live run of mine.
 */
fun ThreadPage.Bundle.listRow(): ThreadListRowModel = ThreadListRowModel(
    threadId = thread.id,
    title = view.title,
    caption = view.listCaption,
    live = view.live,
    busy = view.busy,
    needsInput = view.needsInput,
    agent = (myLiveRun ?: liveRun)?.agent?.takeIf { it.isNotBlank() } ?: DEFAULT_AGENT,
)

/**
 * The Agent page's Running band: my threads with a live run of mine (EXP-312:
 * a live run is the owner's), one band per team once there are several.
 */
internal fun LazyListScope.agentThreadsList(
    running: List<ThreadPage.Bundle>,
    onOpenThread: (String) -> Unit,
    teams: List<TeamEntity> = emptyList(),
) {
    val bands = teamBands(running, teams) { it.thread.teamId }
    if (running.isEmpty() || bands.all { it.team == null }) {
        item(key = "__running_header__") { SectionHeader(ThreadCopy.RUNNING) }
    }
    if (running.isEmpty()) {
        item(key = "__no_running__") { EmptyState(EmptyStateCopy.runningAgents) }
        return
    }
    bands.forEach { band ->
        band.team?.let { team ->
            item(key = "__running_team_${team.id}__") { TeamSectionHeader(team) }
        }
        item(key = "__running_threads_${band.team?.id.orEmpty()}__") {
            Column(modifier = Modifier.fillMaxWidth().testTag("agent-thread-rows")) {
                band.items.forEach { bundle ->
                    val row = bundle.listRow()
                    ThreadListRow(
                        title = row.title,
                        caption = row.caption,
                        live = row.live,
                        busy = row.busy,
                        needsInput = row.needsInput,
                        agent = row.agent,
                        onClick = { onOpenThread(row.threadId) },
                    )
                }
            }
        }
    }
}

/** The Agent page's Recent (the history button's sheet): every other
 *  thread, newest activity first, banded per team. */
@Composable
fun RecentThreadsSheet(
    recent: List<ThreadPage.Bundle>,
    onOpenThread: (String) -> Unit,
    onDismiss: () -> Unit,
    teams: List<TeamEntity> = emptyList(),
) {
    GlassSheet(title = ThreadCopy.RECENT, onDismiss = onDismiss) {
        if (recent.isEmpty()) {
            EmptyState(EmptyStateCopy.recentRuns)
            return@GlassSheet
        }
        val bands = remember(recent, teams) { teamBands(recent, teams) { it.thread.teamId } }
        LazyColumn(
            modifier = Modifier.fillMaxWidth().testTag("recent-threads-sheet"),
            contentPadding = PaddingValues(horizontal = 16.dp),
        ) {
            bands.forEach { band ->
                band.team?.let { team ->
                    item(key = "__recent_team_${team.id}__") { TeamSectionHeader(team) }
                }
                band.items.forEach { bundle ->
                    item(key = "recent-${bundle.thread.id}") {
                        val row = bundle.listRow()
                        ThreadListRow(
                            title = row.title,
                            caption = row.caption,
                            live = row.live,
                            busy = row.busy,
                            needsInput = row.needsInput,
                            agent = row.agent,
                            onClick = {
                                onDismiss()
                                onOpenThread(row.threadId)
                            },
                        )
                    }
                }
            }
        }
    }
}
