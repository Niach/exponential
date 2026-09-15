package com.exponential.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AgentUsagePresentation
import com.exponential.app.domain.AgentUsageSeverity
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

// EXP-893: the run's context window as a RADIAL meter — the desktop
// composer's ring, on the phone the Run face's left circle (collapsed bar)
// and the expanded composer's footer glyph. It opens the Usage sheet; the
// numbers live there, the ring only says how full the window is.

/** Normal / ≥75 warning / ≥95 danger — the shared thresholds, mobile's tones
 *  (the usage cards' track fill, `AgentUsageBar`). */
fun severityColor(severity: AgentUsageSeverity): Color = when (severity) {
    AgentUsageSeverity.Danger -> DesignTokens.Semantic.Red
    AgentUsageSeverity.Warning -> DesignTokens.Semantic.Yellow
    AgentUsageSeverity.Normal -> GlassTokens.UsageFill
}

/**
 * A ring filled clockwise from twelve to [percent] of the way round, in the
 * severity tone the percentage earns. A null [percent] (the engine reported
 * no context yet) draws the empty track alone, so the circle it sits in still
 * reads as the usage control.
 */
@Composable
fun ContextRing(
    percent: Int?,
    modifier: Modifier = Modifier,
    size: Dp = 22.dp,
    stroke: Dp = 3.dp,
) {
    val fraction = ((percent ?: 0).coerceIn(0, 100)) / 100f
    val tone = severityColor(AgentUsagePresentation.severity((percent ?: 0).toDouble()))
    val track = GlassTokens.StrokeStrong
    val label = percent?.let { "Context $it% used" } ?: "Context usage"
    Canvas(
        modifier = modifier
            .size(size)
            .testTag("session-context-ring")
            .semantics { contentDescription = label },
    ) {
        val strokePx = stroke.toPx()
        val inset = strokePx / 2f
        val arcSize = Size(this.size.width - strokePx, this.size.height - strokePx)
        val topLeft = Offset(inset, inset)
        drawArc(
            color = track,
            startAngle = -90f,
            sweepAngle = 360f,
            useCenter = false,
            topLeft = topLeft,
            size = arcSize,
            style = Stroke(width = strokePx, cap = StrokeCap.Round),
        )
        if (fraction > 0f) {
            drawArc(
                color = tone,
                startAngle = -90f,
                sweepAngle = 360f * fraction,
                useCenter = false,
                topLeft = topLeft,
                size = arcSize,
                style = Stroke(width = strokePx, cap = StrokeCap.Round),
            )
        }
    }
}
