package com.exponential.app.ui.components

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import at.exponential.ui.primitives.Kbd
import at.exponential.ui.primitives.KbdGroup
import com.exponential.app.domain.Voice
import com.exponential.app.ui.theme.TextEmphasis

/**
 * S-3: a push-to-talk binding as THE keycaps (the web `ShortcutKeys`): one
 * SDK [Kbd] per `shortcutLabel` part in a [KbdGroup]; empty =
 * `copy.shortcutNone` as muted words.
 */
@Composable
fun ShortcutKeys(value: String, modifier: Modifier = Modifier) {
    if (value.isEmpty()) {
        Text(
            Voice.shortcutLabel(value),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = modifier,
        )
        return
    }
    KbdGroup(modifier) {
        for (part in Voice.shortcutLabel(value).split(Voice.ShortcutGrammar.SEPARATOR)) {
            Kbd(part)
        }
    }
}
