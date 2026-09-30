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
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The remote-start feedback caption every launcher host renders the same way
 * (EXP-323 — extracted from the Actions screen so Reviews/Changes report a
 * start identically). Renders nothing while [state] is Idle.
 *
 * Always laid out INSIDE its host's content: the old `floating` notice form
 * lost its last caller, and floating one-shot outcomes are toasts now
 * (EXP-1031, `LocalToaster`).
 */
@Composable
fun SteerRunCaptionRow(
    state: ActionRunState,
    modifier: Modifier = Modifier,
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
