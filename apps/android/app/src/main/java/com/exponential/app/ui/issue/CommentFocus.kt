package com.exponential.app.ui.issue

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.DesignTokens
import kotlinx.coroutines.delay

// EXP-1241 (EXP-1259 D14): `issue/{id}?comment=<id>` (a comment call's card in
// a run, web `?comment=`) lands on the issue scrolled to that comment, which
// then glows briefly: a wash that holds, then fades out. Opacity only, so it
// stays under reduce motion too.

private const val HIGHLIGHT_HOLD_MS = 900
private const val HIGHLIGHT_FADE_MS = 700

/** Scrolls this comment into view once [active] and washes it briefly. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun Modifier.commentFocus(active: Boolean): Modifier {
    if (!active) return this
    val requester = remember { BringIntoViewRequester() }
    val glow = remember { Animatable(1f) }
    LaunchedEffect(Unit) {
        // One frame for the column to place the row before scrolling to it.
        delay(DesignTokens.Motion.Duration.Fast.toLong())
        requester.bringIntoView()
        glow.animateTo(0f, tween(HIGHLIGHT_FADE_MS, delayMillis = HIGHLIGHT_HOLD_MS))
    }
    val wash = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.08f * glow.value)
    return this
        .bringIntoViewRequester(requester)
        .background(wash, RoundedCornerShape(12.dp))
}
