package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
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
