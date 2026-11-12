package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.NavBadges
import com.exponential.app.ui.theme.DesignTokens

/**
 * #FEED-232 (nav-badges.json): THE count badge ×4. A 16dp capsule (min width
 * 16, 4dp side padding), 10sp semibold tabular figures; `attention` = the
 * needs-you yellow with black figures, `muted` = muted fill + muted-foreground
 * figures. Zero or less draws nothing.
 */
@Composable
fun CountBadge(
    count: Int,
    tone: NavBadges.Tone,
    modifier: Modifier = Modifier,
    testTag: String = "count-badge",
) {
    val label = NavBadges.countBadgeLabel(count)
    if (label.isEmpty()) return
    val (fill, ink) = when (tone) {
        NavBadges.Tone.Attention -> DesignTokens.Semantic.Yellow to Color.Black
        NavBadges.Tone.Muted -> DesignTokens.Palette.Muted to DesignTokens.Palette.MutedForeground
    }
    Box(
        modifier = modifier
            .height(16.dp)
            .widthIn(min = 16.dp)
            .background(fill, CircleShape)
            .padding(horizontal = 4.dp)
            .testTag(testTag),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            style = TextStyle(
                fontSize = 10.sp,
                lineHeight = 12.sp,
                fontWeight = FontWeight.SemiBold,
                fontFeatureSettings = "tnum",
                color = ink,
            ),
            maxLines = 1,
        )
    }
}

/** [CountBadge] over a destination's [NavBadges.Badge]. */
@Composable
fun CountBadge(badge: NavBadges.Badge, modifier: Modifier = Modifier, testTag: String = "count-badge") =
    CountBadge(badge.count, badge.tone, modifier, testTag)
