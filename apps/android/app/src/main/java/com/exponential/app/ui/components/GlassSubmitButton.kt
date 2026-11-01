package com.exponential.app.ui.components

import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow
import com.exponential.app.ui.theme.pressScale

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
    val shape = GlassTokens.ButtonShape
    // EXP-1243 (D17): the primary button presses to 0.97.
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
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
            .pressScale(pressed && enabled)
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
            .then(
                if (enabled) {
                    Modifier.clickable(
                        interactionSource = interactionSource,
                        indication = LocalIndication.current,
                        onClick = onClick,
                    )
                } else {
                    Modifier
                },
            )
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

/** The pinned geometry of THE destructive button (web `Button
 *  variant="destructive" size="xs"`: h-6, px-1.5 beside a glyph, gap-1,
 *  text-xs, a 12px glyph, rounded-md) — iOS `DestructiveButtonTokens`. */
object DestructiveButtonTokens {
    val Height = 24.dp
    val HorizontalPadding = 6.dp
    val Spacing = 4.dp
    val Glyph = 12.dp
    val Radius = 6.dp
    /** `dark:bg-destructive/60`. */
    const val FillAlpha = 0.6f
}

/**
 * EXP-1554: THE destructive action (Delete agent / app / team / account),
 * one recipe ×4: the `destructive` fill at 60 %, white label, a leading
 * `ui-delete` glyph, small, hugging its label. Never full-width, never a red
 * text row, never a tinted chip. The host places it (under the form
 * left-aligned, or trailing in a Danger zone row).
 */
@Composable
fun DestructiveButton(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector = ExpIcons.uiDelete,
    enabled: Boolean = true,
    loading: Boolean = false,
) {
    val shape = RoundedCornerShape(DestructiveButtonTokens.Radius)
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val active = enabled && !loading
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(DestructiveButtonTokens.Spacing),
        modifier = modifier
            .pressScale(pressed && active)
            .alpha(if (enabled) 1f else 0.5f)
            .height(DestructiveButtonTokens.Height)
            .clip(shape)
            .background(DesignTokens.Palette.Destructive.copy(alpha = DestructiveButtonTokens.FillAlpha), shape)
            .then(
                if (active) {
                    Modifier.clickable(
                        interactionSource = interactionSource,
                        indication = LocalIndication.current,
                        onClick = onClick,
                    )
                } else {
                    Modifier
                },
            )
            .padding(horizontal = DestructiveButtonTokens.HorizontalPadding),
    ) {
        if (loading) {
            CircularProgressIndicator(
                color = Color.White,
                strokeWidth = 1.5.dp,
                modifier = Modifier.size(DestructiveButtonTokens.Glyph),
            )
        } else {
            Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(DestructiveButtonTokens.Glyph))
        }
        Text(
            label,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
            color = Color.White,
            maxLines = 1,
        )
    }
}

/**
 * EXP-1554: web's Danger zone ×4 (team General, Account): the danger band,
 * then ONE row = the muted consequence + THE [DestructiveButton] trailing.
 */
@Composable
fun DangerZoneSection(
    description: String,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    loading: Boolean = false,
    buttonModifier: Modifier = Modifier,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = modifier.fillMaxWidth()) {
        SectionBand {
            Text(
                "Danger zone",
                style = MaterialTheme.typography.labelLarge,
                color = DesignTokens.Semantic.Red,
            )
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier
                .fillMaxWidth()
                .glassRow()
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Text(
                description,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.weight(1f),
            )
            DestructiveButton(label, onClick = onClick, enabled = enabled, loading = loading, modifier = buttonModifier)
        }
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
