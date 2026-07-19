package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Full-width glass-pill segmented control — a 1:1 port of the iOS My Work
 * Inbox/My Issues tab language (EXP-192): one glass capsule container holding
 * equal-width segments, the active one filled white-0.12. Optional
 * per-segment count [badge] (indigo capsule, the Inbox unread count).
 */
@Composable
fun <T> GlassSegmentedControl(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    badge: (T) -> Int = { 0 },
) {
    val capsule = RoundedCornerShape(percent = 50)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(capsule)
            .background(GlassTokens.RowFill, capsule)
            .border(GlassTokens.Hairline, Color.White.copy(alpha = 0.12f), capsule)
            .padding(4.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        options.forEach { option ->
            val active = option == selected
            Row(
                modifier = Modifier
                    .weight(1f)
                    .clip(capsule)
                    .background(
                        if (active) Color.White.copy(alpha = 0.12f) else Color.Transparent,
                        capsule,
                    )
                    .clickable { onSelect(option) }
                    .padding(vertical = 7.dp),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    label(option),
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = if (active) FontWeight.SemiBold else FontWeight.Normal,
                    color = MaterialTheme.colorScheme.onSurface.copy(
                        alpha = if (active) 1f else TextEmphasis.Secondary,
                    ),
                )
                val count = badge(option)
                if (count > 0) {
                    Spacer(Modifier.width(6.dp))
                    Text(
                        count.toString(),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.SemiBold,
                        color = Color.White,
                        modifier = Modifier
                            .clip(capsule)
                            .background(BadgeIndigo, capsule)
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                    )
                }
            }
        }
    }
}

/** The brand indigo (#6366f1) — iOS `Accent.indigo`, the count-badge fill. */
private val BadgeIndigo = Color(0xFF6366F1)
