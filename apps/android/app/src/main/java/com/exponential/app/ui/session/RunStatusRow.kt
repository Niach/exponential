package com.exponential.app.ui.session

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.RunRowTone
import com.exponential.app.domain.showWorkLabel
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.SessionRowBadgeSize
import com.exponential.app.ui.components.SessionRowLeadGap
import com.exponential.app.ui.components.SessionRowMarkSize
import com.exponential.app.ui.issue.DoneBlue
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1175: the Run face's STATUS ROW, on top in both modes — the agent's run
 * mark as the spinner ([markState] = the ×4 `runningRowMarkState`), the
 * `runRowCaption` in its tone, the newest tool line muted under it (live runs
 * only), and the trailing `Show work` / `Hide work` switch. Fixture
 * `run-row.json` (×4).
 */
@Composable
internal fun RunStatusRow(
    agent: String?,
    markState: CodingSessionDisplayState?,
    ended: Boolean,
    caption: String,
    tone: RunRowTone,
    toolLine: String?,
    showWork: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .testTag("run-status-row"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentRunMark(
            agent = agent,
            state = markState,
            size = SessionRowMarkSize,
            badgeSize = SessionRowBadgeSize,
            ended = ended,
        )
        Spacer(Modifier.width(SessionRowLeadGap))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                caption,
                style = MaterialTheme.typography.bodySmall,
                color = runRowToneColor(tone),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!toolLine.isNullOrBlank()) {
                Text(
                    toolLine,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        TextButton(onClick = onToggle, modifier = Modifier.testTag("run-show-work")) {
            Text(showWorkLabel(showWork), style = MaterialTheme.typography.labelMedium)
        }
    }
}

/** The session row's status-line colours, by tone name. */
@Composable
private fun runRowToneColor(tone: RunRowTone): Color = when (tone) {
    RunRowTone.Muted -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    RunRowTone.Amber -> NeedsInputAmber
    RunRowTone.Emerald -> ReviewGreen
    RunRowTone.Sky -> DoneBlue
}
