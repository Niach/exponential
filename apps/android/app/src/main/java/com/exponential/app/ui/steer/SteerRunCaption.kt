package com.exponential.app.ui.steer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassNotice
import com.exponential.app.ui.components.GlassNoticeDefaults
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The remote-start feedback caption every launcher host renders the same way
 * (EXP-323 — extracted from the Actions screen so Reviews/Changes report a
 * start identically). Renders nothing while [state] is Idle.
 *
 * [floating] is for hosts that overlay the caption on scrolling content (the
 * Review screen's bottom bar): bare text ghosted straight over the diff rows
 * underneath, so it gets the opaque glass pill instead (EXP-357). Hosts that
 * lay it out INSIDE their content (Actions, Reviews) keep the bare row.
 */
@Composable
fun SteerRunCaptionRow(
    state: ActionRunState,
    modifier: Modifier = Modifier,
    floating: Boolean = false,
) {
    val text = when (state) {
        is ActionRunState.Idle -> return
        is ActionRunState.Sending -> "Sending start command…"
        is ActionRunState.Sent ->
            "Start sent to ${state.deviceLabel}. Waiting for the desktop…"
        is ActionRunState.Failed -> state.message
    }
    val showSpinner = state is ActionRunState.Sending || state is ActionRunState.Sent
    val color = if (state is ActionRunState.Failed) MaterialTheme.colorScheme.error else null
    if (floating) {
        // A NOTICE, not a pill: "Start sent to <device>. Waiting for the
        // desktop…" and a server-written failure both wrap to two lines on a
        // phone, which a fixed-height capsule can only clip (EXP-698).
        GlassNotice(
            text,
            modifier = modifier,
            contentColor = color,
            leading = if (showSpinner) {
                {
                    CircularProgressIndicator(
                        modifier = Modifier.size(GlassNoticeDefaults.GlyphSize),
                        strokeWidth = 2.dp,
                        color = LocalContentColor.current,
                    )
                }
            } else {
                null
            },
        )
        return
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(GlassPillDefaults.SmSpacing),
        modifier = modifier.padding(vertical = 2.dp),
    ) {
        if (showSpinner) {
            CircularProgressIndicator(
                modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = color ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}
