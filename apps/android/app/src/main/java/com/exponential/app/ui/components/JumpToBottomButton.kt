package com.exponential.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1191: the ONE scroll-to-bottom control ×4 — a 32dp round, icon-only
 * button (the `ui-arrow-down` concept, 16dp at secondary emphasis) on the
 * floating chrome's opaque glass fill with a hairline, no shadow. The caller
 * places it (horizontally centred, 12dp above its bottom bar) and decides
 * [visible] (scrolled away from the newest row of a non-empty feed); it fades
 * and scales in and out on the shared `fast` motion token. The visual is
 * 32dp, the tap target the platform's 48dp minimum.
 */
@Composable
fun JumpToBottomButton(
    visible: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = fadeIn(Motion.fast()) + scaleIn(Motion.fast(), initialScale = 0.8f),
        exit = fadeOut(Motion.fast()) + scaleOut(Motion.fast(), targetScale = 0.8f),
    ) {
        Box(
            modifier = Modifier.size(JumpToBottomSize),
            contentAlignment = Alignment.Center,
        ) {
            Box(
                modifier = Modifier
                    .minimumInteractiveComponentSize()
                    .size(JumpToBottomSize)
                    .clip(CircleShape)
                    .background(GlassTokens.OpaqueCardFill, CircleShape)
                    .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, CircleShape)
                    .clickable(role = Role.Button, onClick = onClick)
                    .testTag("jump-to-bottom"),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    ExpIcons.uiArrowDown,
                    contentDescription = "Jump to bottom",
                    modifier = Modifier.size(16.dp),
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
    }
}

private val JumpToBottomSize = 32.dp
