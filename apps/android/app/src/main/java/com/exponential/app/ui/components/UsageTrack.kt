package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import at.exponential.ui.primitives.MeterTrack
import com.exponential.app.domain.AgentUsageSeverity
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

// EXP-909: THE bar primitive. Every meter in the usage overlay — the windows,
// the Context line, the three-abreast mini form — and every meter on the
// Devices page draws this one track, so a percentage can never read in two
// tones depending on which list it landed in. (Web `Meter` in `@exp/ui`,
// desktop `usage_bar::meter`, iOS `AgentUsageTrack` in ExpUI.)

/** The default meter height in the overlay's full (two-line) window form. */
val UsageTrackHeight: Dp = 6.dp

/** …and in the MINI form, where three sit abreast under one identity line. */
val UsageTrackMiniHeight: Dp = 4.dp

/**
 * Normal / ≥75 warning / ≥95 danger — the shared thresholds
 * ([com.exponential.app.domain.AgentUsagePresentation.severity]), in mobile's
 * tones. Deduped with [ContextRing]: the ring and the bar are two shapes of
 * ONE scale, and they drifted apart once already.
 */
fun severityColor(severity: AgentUsageSeverity): Color = when (severity) {
    AgentUsageSeverity.Danger -> DesignTokens.Semantic.Red
    AgentUsageSeverity.Warning -> DesignTokens.Semantic.Yellow
    AgentUsageSeverity.Normal -> GlassTokens.UsageFill
}

/** The filled track every usage meter renders. */
@Composable
fun UsageTrack(
    percent: Double,
    severity: AgentUsageSeverity,
    modifier: Modifier = Modifier,
    height: Dp = UsageTrackHeight,
) {
    // SLOP-18 / VAPP-89: the SDK's `MeterTrack`. Its fill takes the track's
    // own capsule (EXP-698): a square-ended bar inside a rounded track left
    // two corner slivers of track showing at 100%.
    MeterTrack(
        fraction = (percent / 100.0).coerceIn(0.0, 1.0).toFloat(),
        modifier = modifier,
        height = height,
        track = GlassTokens.StrokeStrong,
        fill = severityColor(severity),
    )
}
