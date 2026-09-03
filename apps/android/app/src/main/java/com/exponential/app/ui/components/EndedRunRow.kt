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
import com.exponential.app.domain.RunResumeTarget
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * EXP-637: one run in the Actions screen's "Recent automated runs" (the Agents
 * tab's own "Recent runs" list was dropped in EXP-676).
 *
 * A FINISHED row is EXPANDABLE, and the summary is deliberately NOT inline: a
 * close-out paragraph on every collapsed row would drown the list. Collapsed
 * shows the run's name and when it ended; tapping expands to the agent's full
 * summary (rendered as the GFM it is) and the Resume affordance, which only
 * exists while [resumeTarget] resolves (own ended run, its own machine online
 * and `resume-run`-capable).
 *
 * EXP-686: a run that is still going ([isLive]) says "Running", carries no
 * chevron and nothing to expand — the whole row opens the live session
 * through [onOpen] instead. No self-reported outcome is shown anywhere.
 *
 * Same rule on web, desktop and iOS.
 */
@Composable
fun EndedRunRow(
    title: String,
    summary: String?,
    timeLabel: String,
    modifier: Modifier = Modifier,
    // The monospace lead-in an issue-scoped run shows (its identifier); action
    // and chat runs have none.
    identifier: String? = null,
    // The machine that ran it, when the surface tracks one.
    deviceLabel: String? = null,
    // The run is still going: "Running", and the row opens it.
    isLive: Boolean = false,
    onOpen: (() -> Unit)? = null,
    resumeTarget: RunResumeTarget? = null,
    resuming: Boolean = false,
    onResume: (RunResumeTarget) -> Unit = {},
) {
    var expanded by remember { mutableStateOf(false) }
    var confirmResume by remember { mutableStateOf(false) }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("ended-run-row")
            .glassRow()
            .clickable { if (onOpen != null) onOpen() else expanded = !expanded }
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
                    if (isLive) {
                        // Live reads green like every other "running now"
                        // signal (iOS parity); a finished run says nothing.
                        Text(
                            "Running",
                            style = MaterialTheme.typography.bodySmall,
                            color = DesignTokens.Semantic.Green,
                            maxLines = 1,
                        )
                    }
                    // "started 5m ago" while it runs, "ended 5m ago" once it
                    // finished — iOS `runByline` parity.
                    val trailing = listOfNotNull(
                        deviceLabel?.takeIf { it.isNotBlank() },
                        timeLabel.takeIf { it.isNotEmpty() }
                            ?.let { if (isLive) "started $it" else "ended $it" },
                    ).joinToString(" · ")
                    if (trailing.isNotEmpty()) {
                        Text(
                            // The dot only separates — nothing precedes it on
                            // a finished row anymore.
                            if (isLive) "· $trailing" else trailing,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            // A live row has nothing to expand — it opens instead.
            if (!isLive) {
                Icon(
                    if (expanded) ExpIcons.uiChevronUp else ExpIcons.uiChevronDown,
                    contentDescription = if (expanded) "Collapse run" else "Expand run",
                    modifier = Modifier.padding(start = 8.dp).size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
        if (expanded && !isLive) {
            Spacer(Modifier.height(8.dp))
            // The wire form is GFM (EXP-686): render it, and say so explicitly
            // when there is nothing to render — MarkdownView draws nothing for
            // a blank string.
            val body = summary?.takeIf { it.isNotBlank() }
            if (body != null) {
                MarkdownView(markdown = body)
            } else {
                Text(
                    "This run left no summary.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
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
                    GlassPill(
                        "Resume",
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
