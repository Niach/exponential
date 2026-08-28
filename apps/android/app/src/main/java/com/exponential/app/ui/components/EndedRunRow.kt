package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.RunOutcome
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.domain.runOutcomeOf
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * EXP-637: one FINISHED run in the Actions screen's "Recent automated runs"
 * (the Agents tab's own "Recent runs" list was dropped in EXP-676).
 *
 * The row is EXPANDABLE, and the summary is deliberately NOT inline: a
 * close-out paragraph on every collapsed row would drown the list. Collapsed
 * shows the run's name plus its outcome glyph + tinted label and when it
 * ended; tapping expands to the agent's full summary (plain text — the wire
 * form is GFM, and mobile renders it as written) and the Resume affordance,
 * which only exists while [resumeTarget] resolves (own ended run, its own
 * machine online and `resume-run`-capable).
 *
 * Same rule on web, desktop and iOS.
 */
@Composable
fun EndedRunRow(
    title: String,
    outcome: String?,
    summary: String?,
    timeLabel: String,
    modifier: Modifier = Modifier,
    // The monospace lead-in an issue-scoped run shows (its identifier); action
    // and chat runs have none.
    identifier: String? = null,
    // The machine that ran it, when the surface tracks one.
    deviceLabel: String? = null,
    resumeTarget: RunResumeTarget? = null,
    resuming: Boolean = false,
    onResume: (RunResumeTarget) -> Unit = {},
) {
    var expanded by remember { mutableStateOf(false) }
    var confirmResume by remember { mutableStateOf(false) }
    val presentation = runOutcomeOf(outcome)
    val tint = when (presentation) {
        RunOutcome.Done -> DesignTokens.Semantic.Blue
        RunOutcome.Blocked -> DesignTokens.Semantic.Yellow
        // Nothing changed, or nothing was reported: neither good nor bad news.
        RunOutcome.NoChanges, RunOutcome.Ended ->
            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    }
    val glyph = when (presentation) {
        RunOutcome.Done -> ExpIcons.runOutcomeDone
        RunOutcome.Blocked -> ExpIcons.runOutcomeBlocked
        RunOutcome.NoChanges, RunOutcome.Ended -> ExpIcons.runOutcomeNoChanges
    }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("ended-run-row")
            .glassRow()
            .clickable { expanded = !expanded }
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (identifier != null) {
                        Text(
                            identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                        )
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(
                        title,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.height(2.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Icon(
                        glyph,
                        contentDescription = null,
                        modifier = Modifier.size(12.dp),
                        tint = tint,
                    )
                    Text(
                        presentation.label,
                        style = MaterialTheme.typography.bodySmall,
                        color = tint,
                        maxLines = 1,
                    )
                    val trailing = listOfNotNull(
                        deviceLabel?.takeIf { it.isNotBlank() },
                        timeLabel.takeIf { it.isNotEmpty() }?.let { "ended $it" },
                    )
                    if (trailing.isNotEmpty()) {
                        Text(
                            "· ${trailing.joinToString(" · ")}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            Icon(
                if (expanded) ExpIcons.uiChevronUp else ExpIcons.uiChevronDown,
                contentDescription = if (expanded) "Collapse run" else "Expand run",
                modifier = Modifier.padding(start = 8.dp).size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (expanded) {
            Spacer(Modifier.height(8.dp))
            Text(
                summary?.takeIf { it.isNotBlank() } ?: "This run left no summary.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            if (resumeTarget != null) {
                Spacer(Modifier.height(10.dp))
                if (resuming) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(12.dp),
                            strokeWidth = 2.dp,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "Resuming on ${resumeTarget.deviceLabel}…",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                } else {
                    GlassPillButton(
                        label = "Resume",
                        icon = ExpIcons.runResume,
                        onClick = { confirmResume = true },
                        modifier = Modifier.testTag("resume-run"),
                    )
                }
            }
        }
    }

    // A resume relaunches the agent on that machine — cheap, but not silent:
    // same confirm shape as the other remote commands.
    if (confirmResume && resumeTarget != null) {
        AlertDialog(
            onDismissRequest = { confirmResume = false },
            title = { Text("Resume this run?") },
            text = {
                Text(
                    "Starts the agent again on ${resumeTarget.deviceLabel}, in the same " +
                        "workspace, picking up where the run stopped.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmResume = false
                        onResume(resumeTarget)
                    },
                ) { Text("Resume") }
            },
            dismissButton = {
                TextButton(onClick = { confirmResume = false }) { Text("Cancel") }
            },
        )
    }
}
