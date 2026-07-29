package com.exponential.app.ui.steer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

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
            "Start sent to ${state.deviceLabel} — waiting for the desktop…"
        is ActionRunState.Failed -> state.message
    }
    val showSpinner = state is ActionRunState.Sending || state is ActionRunState.Sent
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = modifier
            .then(if (floating) Modifier.glassButton(opaque = true) else Modifier)
            .padding(
                horizontal = if (floating) 12.dp else 0.dp,
                vertical = if (floating) 7.dp else 2.dp,
            ),
    ) {
        if (showSpinner) {
            CircularProgressIndicator(
                modifier = Modifier.size(12.dp),
                strokeWidth = 2.dp,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = if (state is ActionRunState.Failed) {
                MaterialTheme.colorScheme.error
            } else {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            },
        )
    }
}
