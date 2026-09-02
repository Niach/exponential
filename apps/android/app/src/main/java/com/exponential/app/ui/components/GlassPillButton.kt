package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

/**
 * Compact capsule action (optional icon + label) on a glass pill — the inline
 * header / card affordance the iOS settings use for "Add repository" /
 * "New board" and the Agents screen uses for "Start coding". Dims to
 * quaternary emphasis and ignores taps when [enabled] is false.
 */
@Composable
fun GlassPillButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    enabled: Boolean = true,
    /** EXP-678: taller when the pill has to match a neighbouring chip's height
     *  — the steer screen's Merge pill beside the "Latest changes" chip. */
    verticalPadding: Dp = 6.dp,
    /** A call in flight: the icon becomes a spinner, so the pill keeps its
     *  width and the caller only has to dim it via [enabled]. */
    loading: Boolean = false,
    /** EXP-688: a solid fill under the glass tint, for a pill floating over
     *  scrolling content (the steer screen's Merge pill). */
    opaque: Boolean = false,
    /** EXP-694: the label's face — monospace for an issue identifier, the
     *  Agents row's pill (web `font-mono`, iOS `.monospaced()`). Null keeps
     *  the label style's own family. */
    fontFamily: FontFamily? = null,
) {
    val fg = MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = modifier
            .glassButton(opaque = opaque)
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = verticalPadding),
    ) {
        if (loading) {
            CircularProgressIndicator(
                modifier = Modifier.size(14.dp),
                strokeWidth = 2.dp,
                color = fg,
            )
        } else if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(14.dp), tint = fg)
        }
        val labelStyle = MaterialTheme.typography.labelMedium
        Text(
            label,
            style = if (fontFamily != null) labelStyle.copy(fontFamily = fontFamily) else labelStyle,
            color = fg,
        )
    }
}

/**
 * Full-width primary on glass — "Create board", "Continue". Replaces Material's
 * filled Button on glass forms (EXP-577).
 *
 * EXP-694: an ENABLED submit is the solid near-white primary with dark content
 * and no visible hairline (web's dialog footer / the desktop `.primary()`
 * button / iOS `GlassSubmitButton` — one look on all four clients). The old
 * `white.opacity(0.15)` fill read as a disabled control on a #18181B sheet.
 * Disabled is unchanged: `0.06` fill, `0.10` hairline, tertiary label.
 */
@Composable
fun GlassSubmitButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: (@Composable () -> Unit)? = null,
) {
    val shape = RoundedCornerShape(10.dp)
    val content = if (enabled) {
        DesignTokens.Palette.PrimaryForeground
    } else {
        Color.White.copy(alpha = TextEmphasis.Tertiary)
    }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(
                if (enabled) DesignTokens.Palette.Primary else GlassTokens.CardFill,
                shape,
            )
            .border(
                GlassTokens.Hairline,
                // The fill carries the shape on its own once it is opaque.
                if (enabled) Color.Transparent else GlassTokens.StrokeCard,
                shape,
            )
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = 14.dp),
    ) {
        // The icon slot (a glyph, or the in-flight spinner) draws in the
        // button's own content color — white-on-white otherwise.
        if (icon != null) {
            CompositionLocalProvider(LocalContentColor provides content) { icon() }
        }
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
            color = content,
        )
    }
}

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
    val shape = RoundedCornerShape(10.dp)
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
