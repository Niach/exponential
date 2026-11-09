package com.exponential.app.ui.components

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import at.exponential.ui.primitives.Kbd
import at.exponential.ui.primitives.KbdGroup
import at.exponential.ui.primitives.LocalPrimitiveTokens
import at.exponential.ui.primitives.ProvidePrimitiveTokens
import com.exponential.app.domain.Voice
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * S-3: a push-to-talk binding as THE keycaps (the web `ShortcutKeys`): one
 * SDK [Kbd] per `shortcutLabel` part in a [KbdGroup]; empty =
 * `copy.shortcutNone` as muted words. The cap paints the tokens' `accent`;
 * the app's is the opaque `#262626`, so the caps get the glass active wash
 * (the exponential theme's dark `accent` `#ffffff1c`, the IDE's keycap).
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
    ProvidePrimitiveTokens(LocalPrimitiveTokens.current.copy(accent = GlassTokens.RowFillActive)) {
        KbdGroup(modifier) {
            for (part in Voice.shortcutLabel(value).split(Voice.ShortcutGrammar.SEPARATOR)) {
                Kbd(part)
            }
        }
    }
}
