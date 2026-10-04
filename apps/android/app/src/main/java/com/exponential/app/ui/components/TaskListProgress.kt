package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.TASK_LIST_STATUS_COMPLETED
import com.exponential.app.domain.TASK_LIST_STATUS_IN_PROGRESS

/** EXP-1191: past this many entries the mark is one continuous track. */
const val TASK_LIST_PROGRESS_MAX_SEGMENTS = 12

private const val DONE_ALPHA = 0.70f
private const val RUNNING_ALPHA = 0.35f
private const val PENDING_ALPHA = 0.12f

/**
 * EXP-1191 — the agent task list's own mark ×4 (web `TaskListProgress`), so
 * its line above the composer never reads as a queued steer message or as the
 * composer: one 10×4dp segment per entry, done solid, the running one half,
 * the rest faint. More than [TASK_LIST_PROGRESS_MAX_SEGMENTS] entries draw one
 * 80×4dp track filled to `done / total` instead. Decorative: the `2/4` count
 * beside it carries the meaning.
 */
@Composable
fun TaskListProgress(statuses: List<String>, modifier: Modifier = Modifier) {
    val total = statuses.size
    if (total == 0) return
    val ink = MaterialTheme.colorScheme.onSurface
    val pill = RoundedCornerShape(50)
    if (total > TASK_LIST_PROGRESS_MAX_SEGMENTS) {
        val done = statuses.count { it == TASK_LIST_STATUS_COMPLETED }
        Box(
            modifier = modifier
                .size(width = 80.dp, height = 4.dp)
                .clip(pill)
                .background(ink.copy(alpha = PENDING_ALPHA))
                .testTag("task-list-progress"),
        ) {
            Box(
                Modifier
                    .fillMaxHeight()
                    .fillMaxWidth(done.toFloat() / total)
                    .clip(pill)
                    .background(ink.copy(alpha = DONE_ALPHA)),
            )
        }
        return
    }
    Row(
        modifier = modifier.testTag("task-list-progress"),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        statuses.forEach { status ->
            val alpha = when (status) {
                TASK_LIST_STATUS_COMPLETED -> DONE_ALPHA
                TASK_LIST_STATUS_IN_PROGRESS -> RUNNING_ALPHA
                else -> PENDING_ALPHA
            }
            Box(
                Modifier
                    .size(width = 10.dp, height = 4.dp)
                    .clip(pill)
                    .background(ink.copy(alpha = alpha)),
            )
        }
    }
}
