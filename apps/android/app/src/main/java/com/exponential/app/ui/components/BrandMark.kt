package com.exponential.app.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.R

/**
 * EXP-846: the Exponential mark, for the surfaces that have to say "this was
 * US" — the transcript rows of our own MCP tool calls, which wear the product's
 * mark the way a file edit wears the wrench.
 *
 * It paints the mark the app ALREADY ships (`ic_splash_icon`, the self-contained
 * logo circle the launch animation morphs from) rather than a second hand-kept
 * copy of the geometry. That drawable's circle fills only the middle
 * [MARK_VIEWPORT_RATIO] of its 108dp viewport, so [size] is the size of the
 * visible MARK and the image box is scaled up around it — asking for 12dp here
 * gets a 12dp disc, not a 12dp box with a 6dp disc in it.
 */
@Composable
fun ExponentialMark(size: Dp = 12.dp, modifier: Modifier = Modifier) {
    Image(
        painter = painterResource(R.drawable.ic_splash_icon),
        contentDescription = null,
        modifier = modifier.size(size / MARK_VIEWPORT_RATIO),
    )
}

/** The logo circle's diameter (58) over the drawable's viewport (108). */
private const val MARK_VIEWPORT_RATIO = 58f / 108f
