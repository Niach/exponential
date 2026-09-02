package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The ONE composer shell (EXP-698) — the issue detail's docked comment box,
 * the steer screen's "message the agent" box and the support thread's reply
 * box. All three drew the same thing by hand with different corner radii (24
 * / 24 / none), different fills and a different tool row; they are this now.
 *
 * The card is the standard [GlassTokens.CardRadius] glass card, NOT a 24dp
 * capsule-ish blob: a composer is a card with a field in it, the same rung as
 * every other card on the screen. [opaque] is for the two that FLOAT over
 * scrolling content (the docked comment bar, the steer bar) — the translucent
 * fill lets the feed ghost through otherwise. No elevation on either: the
 * hairline is the edge.
 *
 * Slots, top to bottom: [leading] (a row above the field — the support
 * thread's Reply / Internal-note pills), [strip] (queued attachments),
 * [field], and a bottom row of [tools] (ghost glyph buttons) with [submit]
 * pushed to the end.
 */
@Composable
fun GlassComposer(
    modifier: Modifier = Modifier,
    opaque: Boolean = false,
    leading: (@Composable ColumnScope.() -> Unit)? = null,
    strip: (@Composable ColumnScope.() -> Unit)? = null,
    tools: (@Composable RowScope.() -> Unit)? = null,
    submit: (@Composable () -> Unit)? = null,
    field: @Composable ColumnScope.() -> Unit,
) {
    val shape = RoundedCornerShape(GlassTokens.CardRadius)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(if (opaque) GlassTokens.OpaqueCardFill else GlassTokens.CardFill, shape)
            .border(
                GlassTokens.Hairline,
                if (opaque) GlassTokens.StrokeStrong else GlassTokens.StrokeCard,
                shape,
            )
            .padding(
                horizontal = GlassComposerDefaults.HorizontalPadding,
                vertical = GlassComposerDefaults.VerticalPadding,
            ),
    ) {
        leading?.invoke(this)
        strip?.invoke(this)
        field()
        if (tools != null || submit != null) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                tools?.invoke(this)
                Spacer(Modifier.weight(1f))
                submit?.invoke()
            }
        }
    }
}

/**
 * One ghost glyph in a composer's tool row — attach, mention, issue-ref,
 * emoji. No chrome of its own (the composer card is the surface); a secondary
 * glyph that dims to quaternary when it cannot act.
 */
@Composable
fun ComposerToolButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
) {
    IconButton(onClick = onClick, enabled = enabled) {
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(GlassComposerDefaults.ToolGlyphSize),
            tint = Color.White.copy(
                alpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Quaternary,
            ),
        )
    }
}

/**
 * The composer's submit — a ghost round button whose GLYPH carries the state:
 * full white when there is something to send, quaternary when there is not,
 * a spinner while it is in flight. No filled capsule: the send arrow is the
 * only bright thing in the composer, so it needs no second signal.
 */
@Composable
fun ComposerSubmitButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    enabled: Boolean,
    sending: Boolean = false,
) {
    IconButton(onClick = onClick, enabled = enabled && !sending) {
        if (sending) {
            CircularProgressIndicator(
                modifier = Modifier.size(GlassComposerDefaults.SpinnerSize),
                strokeWidth = 2.dp,
                color = Color.White,
            )
        } else {
            Icon(
                icon,
                contentDescription = contentDescription,
                modifier = Modifier.size(GlassComposerDefaults.SubmitGlyphSize),
                tint = if (enabled) Color.White else Color.White.copy(alpha = TextEmphasis.Quaternary),
            )
        }
    }
}

/** The composer's own numbers, so no call site re-types an inset (EXP-698). */
object GlassComposerDefaults {
    val HorizontalPadding: Dp = 12.dp
    val VerticalPadding: Dp = 8.dp
    val ToolGlyphSize: Dp = DesignTokens.Size.ControlSm
    val SubmitGlyphSize: Dp = 24.dp
    val SpinnerSize: Dp = 18.dp
}
