package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The paint a [GlassSubmitButton] wears (EXP-1212) — web `Button`'s
 * `default` / `outline` / `destructive` variants. [Outline] is the
 * [GlassOAuthButton] chrome (card fill + strong hairline); [Destructive] the
 * solid destructive fill with white content.
 */
enum class GlassButtonRole { Primary, Outline, Destructive }

/**
 * Full-width primary on glass — "Create board", "Continue". Replaces Material's
 * filled Button on glass forms (EXP-577).
 *
 * EXP-694: an ENABLED submit is the solid near-white primary with dark content
 * and no visible hairline (web's dialog footer / the desktop `.primary()`
 * button / iOS `GlassSubmitButton` — one look on all four clients). The old
 * `white.opacity(0.15)` fill read as a disabled control on a #18181B sheet.
 * Disabled is unchanged: `0.06` fill, `0.10` hairline, tertiary label.
 *
 * EXP-1212: [role] repaints it as the outline or destructive button and
 * [compact] is the 40dp web dialog-footer height with the `text-sm` label
 * ([GlassAlert]'s stacked buttons).
 */
@Composable
fun GlassSubmitButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    role: GlassButtonRole = GlassButtonRole.Primary,
    compact: Boolean = false,
    icon: (@Composable () -> Unit)? = null,
) {
    val shape = GlassTokens.ButtonShape
    val content = when {
        !enabled -> Color.White.copy(alpha = TextEmphasis.Tertiary)
        role == GlassButtonRole.Primary -> DesignTokens.Palette.PrimaryForeground
        else -> Color.White
    }
    val fill = when {
        !enabled -> GlassTokens.CardFill
        role == GlassButtonRole.Primary -> DesignTokens.Palette.Primary
        role == GlassButtonRole.Destructive -> DesignTokens.Palette.Destructive
        else -> GlassTokens.CardFill
    }
    val stroke = when {
        !enabled -> GlassTokens.StrokeCard
        role == GlassButtonRole.Outline -> GlassTokens.StrokeStrong
        // The fill carries the shape on its own once it is opaque.
        else -> Color.Transparent
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        modifier = modifier
            .fillMaxWidth()
            .then(if (compact) Modifier.height(CompactHeight) else Modifier)
            .clip(shape)
            .background(fill, shape)
            .border(GlassTokens.Hairline, stroke, shape)
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .then(if (compact) Modifier else Modifier.padding(vertical = 14.dp)),
    ) {
        // The icon slot (a glyph, or the in-flight spinner) draws in the
        // button's own content color — white-on-white otherwise.
        if (icon != null) {
            CompositionLocalProvider(LocalContentColor provides content) { icon() }
        }
        Text(
            label,
            style = if (compact) {
                MaterialTheme.typography.labelLarge
            } else {
                MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium)
            },
            color = content,
        )
    }
}

/** Web's dialog-footer button height (`h-10`). */
private val CompactHeight = 40.dp

/**
 * The provider sign-in button — iOS `InstanceView.oauthButton` /
 * `LoginView.oauthButton`: icon + label on a `white.opacity(0.08)` fill with a
 * `0.15` hairline, 10dp corners, 14dp vertical padding.
 */
@Composable
fun GlassOAuthButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: @Composable () -> Unit,
) {
    val shape = GlassTokens.ButtonShape
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassTokens.CardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            .clickable(onClick = onClick)
            .padding(vertical = 14.dp),
    ) {
        CompositionLocalProvider(LocalContentColor provides Color.White) {
            icon()
            Text(
                label,
                style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
                color = Color.White,
            )
        }
    }
}
