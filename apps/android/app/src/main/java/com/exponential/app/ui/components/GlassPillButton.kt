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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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
) {
    val fg = MaterialTheme.colorScheme.onSurface.copy(
        alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
    )
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        modifier = modifier
            .glassButton()
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(14.dp), tint = fg)
        }
        Text(label, style = MaterialTheme.typography.labelMedium, color = fg)
    }
}

/**
 * Full-width primary on glass — the iOS form submit (`Color.white.opacity(0.15)`
 * enabled / `0.06` disabled, 10pt corners, hairline stroke): "Create board",
 * "Continue". Replaces Material's filled Button on glass forms (EXP-577).
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
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(Color.White.copy(alpha = if (enabled) 0.15f else 0.06f), shape)
            .border(GlassTokens.Hairline, Color.White.copy(alpha = 0.10f), shape)
            .then(if (enabled) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(vertical = 14.dp),
    ) {
        if (icon != null) icon()
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
            color = Color.White.copy(alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Tertiary),
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
            .background(Color.White.copy(alpha = 0.08f), shape)
            .border(GlassTokens.Hairline, Color.White.copy(alpha = 0.15f), shape)
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
