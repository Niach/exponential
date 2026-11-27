package com.exponential.app.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.components.Trace
import com.exponential.app.ui.components.TraceCollapsible
import com.exponential.app.ui.components.TraceLine
import com.exponential.app.ui.components.TraceSize
import com.exponential.app.ui.icons.ExpIcons

/**
 * #EXP-1873 + #FEED-317 (transcript-grouping.json): a run of quiet schedule
 * ticks as THE tool-group row (the Trace sm collapsible grouped tool calls
 * use): `trigger-schedule` + `checkInLabel(n, first, last)`; n > 1 opens
 * (closed by default) to one line per tick, oldest first; n = 1 never opens.
 * [ticks] = each tick's `HH:mm` (null = unknown), oldest first.
 */
@Composable
fun ScheduleTicksRow(ticks: List<String?>, modifier: Modifier = Modifier) {
    val count = ticks.size.coerceAtLeast(1)
    val title = TranscriptGrouping.checkInLabel(count, ticks.firstOrNull(), ticks.lastOrNull())
    Trace(size = TraceSize.Sm, modifier = modifier.testTag("session-schedule-group")) {
        if (count == 1) {
            TraceLine(title = title, icon = ExpIcons.triggerSchedule)
            return@Trace
        }
        var expanded by rememberSaveable { mutableStateOf(false) }
        TraceCollapsible(
            open = expanded,
            onOpenChange = { expanded = it },
            trigger = { isOpen, toggle ->
                TraceLine(title = title, icon = ExpIcons.triggerSchedule, open = isOpen, onClick = toggle)
            },
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(TraceSize.Sm.geometry.lineGap.dp)) {
                ticks.forEach { at ->
                    TraceLine(title = TranscriptGrouping.checkInLabel(1, at, at), icon = ExpIcons.triggerSchedule)
                }
            }
        }
    }
}
