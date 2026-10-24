package com.exponential.app.ui.components

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Chats
import com.exponential.app.ui.theme.TextEmphasis

/**
 * THE `Load earlier` above a transcript window (chats.json WINDOW, #EXP-1380):
 * the run view's and every chat's, one look ×4 — the web's ghost `sm` button
 * in muted small text, iOS `TranscriptLoadEarlierButton` (footnote, secondary).
 */
@Composable
fun TranscriptLoadEarlierButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    TextButton(
        onClick = onClick,
        modifier = modifier.fillMaxWidth().heightIn(min = 28.dp).testTag(Chats.TestIds.LOAD_EARLIER),
        colors = ButtonDefaults.textButtonColors(
            contentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        ),
        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
    ) {
        Text(Chats.Copy.LOAD_EARLIER, style = MaterialTheme.typography.labelMedium)
    }
}
