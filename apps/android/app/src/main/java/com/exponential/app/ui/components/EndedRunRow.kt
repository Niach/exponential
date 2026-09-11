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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * EXP-637: one run in a runs list — the Actions screen's "Recent automated
 * runs" and the Agents screen's "Past".
 *
 * EXP-773 made it a plain LINK. A row used to expand to the agent's close-out
 * summary and a Resume pill; both now live at the top of the fullscreen
 * session view, next to the transcript they belong to. So a row is title ·
 * state · byline and a tap opens that session — live or finished, the same
 * gesture, the same destination. Same rule on web, desktop and iOS.
 */
@Composable
fun EndedRunRow(
    title: String,
    timeLabel: String,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
    // The monospace lead-in an issue-scoped run shows (its identifier); action
    // and chat runs have none.
    identifier: String? = null,
    // The machine that ran it, when the surface tracks one.
    deviceLabel: String? = null,
    /**
     * EXP-746: the Agents screen's "Past" caption, composed once by
     * `pastRunByline` — `<device> · <time>` (EXP-833).
     * When set it REPLACES the device/time line below, so the ×4 string is
     * whatever that one function produced; the Automations list passes none
     * and keeps the caption it always had.
     */
    byline: String? = null,
    // The run is still going: "Running".
    isLive: Boolean = false,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .testTag("ended-run-row")
            .glassRow()
            .clickable(onClick = onOpen)
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
                    // finished — iOS `runByline` parity. EXP-746: a finished
                    // row given a composed byline shows that instead.
                    val trailing = byline?.takeIf { it.isNotBlank() && !isLive }
                        ?: listOfNotNull(
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
            Icon(
                ExpIcons.uiChevronRight,
                contentDescription = null,
                modifier = Modifier.padding(start = 8.dp).size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}
