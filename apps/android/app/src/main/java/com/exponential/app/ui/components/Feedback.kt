package com.exponential.app.ui.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.MarkSpan
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// EXP-1275 / EXP-1271 (EXP-1259 D8): THE feedback states on Android, one
// system ×4 off `fixtures/feedback-states.json` (the pure half is
// `domain/FeedbackMark.kt`): the inline spinner (+ [InlineStatus]), the pending
// state ([rememberDelayedFlag], [PendingPill], the composer's submit), the
// skeleton rows, THE MARK and the blocking overlay. Errors are toasts
// ([LocalToaster]) or the field's own inline line; nothing here draws one.

/**
 * `flag`, but only once it has held true for [delayMs]; false again at once.
 * A pending state that resolves inside the delay never shows (D8.2).
 */
@Composable
fun rememberDelayedFlag(flag: Boolean, delayMs: Long = FeedbackStates.PENDING_DELAY_MS): Boolean {
    var shown by remember { mutableStateOf(flag && delayMs <= 0) }
    LaunchedEffect(flag, delayMs) {
        if (!flag) {
            shown = false
            return@LaunchedEffect
        }
        if (delayMs > 0) delay(delayMs)
        shown = true
    }
    return flag && shown
}

/**
 * D8.1 INLINE: the 10dp spinner every band, header and caption shares (12dp =
 * `pending.spinner`, a pending button's glyph slot). A fixed three-quarter
 * ring that turns, like iOS + web: Material's indeterminate indicator grows
 * its arc from nothing, so at 10dp (and in any frozen frame) it read as a
 * speck. Static under reduce motion.
 */
@Composable
fun InlineSpinner(
    modifier: Modifier = Modifier,
    size: Dp = FeedbackStates.INLINE_SPINNER.dp,
    color: Color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
) {
    val reduced = LocalReduceMotion.current
    // Read in the draw phase only: a turning spinner never recomposes its row.
    val turn = if (reduced) {
        null
    } else {
        rememberInfiniteTransition(label = "inline-spinner").animateFloat(
            initialValue = 0f,
            targetValue = 360f,
            animationSpec = infiniteRepeatable(tween(SPINNER_PERIOD_MS, easing = LinearEasing), RepeatMode.Restart),
            label = "inline-spinner-turn",
        )
    }
    Canvas(
        modifier = modifier
            .size(size)
            .semantics { contentDescription = FeedbackStates.WORD_LOADING },
    ) {
        // iOS `max(1.25, size / 8)`, inset by half the stroke so it never clips.
        val stroke = maxOf(1.25.dp.toPx(), this.size.minDimension / 8f)
        val inset = stroke / 2f
        drawArc(
            color = color,
            startAngle = (turn?.value ?: 0f) - 90f,
            sweepAngle = 270f,
            useCenter = false,
            topLeft = Offset(inset, inset),
            size = androidx.compose.ui.geometry.Size(this.size.width - stroke, this.size.height - stroke),
            style = Stroke(width = stroke, cap = StrokeCap.Round),
        )
    }
}

/** One turn of [InlineSpinner] (iOS `InlineSpinner.period`). */
private const val SPINNER_PERIOD_MS = 800

/** What an autosaving field's caption says right now. */
enum class InlineStatusState { Saving, Saved, Idle }

/**
 * D8.1 INLINE: the spinner + the progressive word (`Saving` → `Saved`); idle
 * shows the settled caption ([idleLabel], e.g. "Edited 2m ago") or nothing.
 */
@Composable
fun InlineStatus(state: InlineStatusState, modifier: Modifier = Modifier, idleLabel: String? = null) {
    val text = when (state) {
        InlineStatusState.Saving -> FeedbackStates.WORD_SAVING
        InlineStatusState.Saved -> FeedbackStates.WORD_SAVED
        InlineStatusState.Idle -> idleLabel
    } ?: return
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(FeedbackStates.INLINE_GAP.dp),
    ) {
        if (state == InlineStatusState.Saving) InlineSpinner()
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/**
 * D8.2 PENDING: a pill that started something. After the delay its glyph slot
 * becomes the spinner and its label the progressive [pendingLabel]
 * ("Starting"); it never shrinks below its idle width and is disabled while
 * [pending].
 */
@Composable
fun PendingPill(
    label: String,
    pendingLabel: String,
    pending: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    size: PillSize = PillSize.Md,
    primary: Boolean = false,
    enabled: Boolean = true,
) {
    val shown = rememberDelayedFlag(pending)
    val density = LocalDensity.current
    var idleWidth by remember { mutableStateOf(0.dp) }
    GlassPill(
        if (shown) pendingLabel else label,
        modifier = modifier
            .widthIn(min = idleWidth)
            .onSizeChanged { if (!shown) idleWidth = with(density) { it.width.toDp() } },
        onClick = onClick,
        size = size,
        primary = primary,
        icon = icon,
        enabled = enabled && !pending,
        loading = shown,
    )
}

/**
 * D8.3 PLACEHOLDER: skeleton rows in the real row geometry ([rowHeight], a
 * dot where the glyph sits, bars where the text sits), the whole block
 * pulsing opacity per `placeholder`, static under reduce motion. Replaces
 * every "Loading…" text and the phones' centred first-load spinners.
 */
@Composable
fun SkeletonRows(
    modifier: Modifier = Modifier,
    rows: Int = FeedbackStates.PLACEHOLDER_ROWS,
    rowHeight: Dp = 44.dp,
    dot: Boolean = true,
) {
    val reduced = LocalReduceMotion.current
    val low = FeedbackStates.PLACEHOLDER_OPACITY_LOW.toFloat()
    val high = FeedbackStates.PLACEHOLDER_OPACITY_HIGH.toFloat()
    val opacity = if (reduced) {
        FeedbackStates.PLACEHOLDER_REDUCED_OPACITY.toFloat()
    } else {
        val transition = rememberInfiniteTransition(label = "skeleton")
        val pulse by transition.animateFloat(
            initialValue = low,
            targetValue = high,
            animationSpec = infiniteRepeatable(
                // low → high → low over ONE pulse (`pulseMs`).
                tween(FeedbackStates.PLACEHOLDER_PULSE_MS / 2, easing = FastOutSlowInEasing),
                RepeatMode.Reverse,
            ),
            label = "skeleton-pulse",
        )
        pulse
    }
    val fill = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.12f)
    val bar = RoundedCornerShape(FeedbackStates.PLACEHOLDER_RADIUS.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .alpha(opacity)
            .testTag(FeedbackStates.SKELETON_TEST_ID)
            .semantics { contentDescription = FeedbackStates.WORD_LOADING },
    ) {
        repeat(rows) { index ->
            val (lead, title) = SKELETON_BAR_WIDTHS[index % SKELETON_BAR_WIDTHS.size]
            Row(
                modifier = Modifier.fillMaxWidth().height(rowHeight).padding(horizontal = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (dot) {
                    Box(Modifier.size(FeedbackStates.PLACEHOLDER_DOT.dp).background(fill, CircleShape))
                }
                Box(Modifier.width(lead).height(FeedbackStates.PLACEHOLDER_BAR_HEIGHT.dp).background(fill, bar))
                Box(Modifier.width(title).height(FeedbackStates.PLACEHOLDER_BAR_HEIGHT.dp).background(fill, bar))
            }
        }
    }
}

/** The bars' widths per row (web `BAR_WIDTHS`): a stack never reads as one ruled block. */
private val SKELETON_BAR_WIDTHS = listOf(56.dp to 168.dp, 56.dp to 120.dp, 56.dp to 192.dp)

/**
 * D8.4 THE MARK: the logo disc (foreground) whose three cut stripes sweep in
 * turn on a `mark.loopMs` loop, the cuts punched through to the CALLER's
 * ground. Static logo under reduce motion; [phase] freezes one frame of the
 * loop (the styleguide's specimens).
 */
@Composable
fun FeedbackMark(
    modifier: Modifier = Modifier,
    size: Dp = FeedbackStates.MARK_SIZE_INLINE.dp,
    color: Color = MaterialTheme.colorScheme.onSurface,
    phase: Float? = null,
) {
    val reduced = LocalReduceMotion.current
    val spans: List<MarkSpan?> = when {
        reduced -> FeedbackStates.STATIC_SPANS
        phase != null -> FeedbackStates.markSpans(phase.toDouble())
        else -> {
            val transition = rememberInfiniteTransition(label = "feedback-mark")
            val loop by transition.animateFloat(
                initialValue = 0f,
                targetValue = 1f,
                animationSpec = infiniteRepeatable(
                    tween(FeedbackStates.MARK_LOOP_MS, easing = LinearEasing),
                    RepeatMode.Restart,
                ),
                label = "feedback-mark-phase",
            )
            FeedbackStates.markSpans(loop.toDouble())
        }
    }
    MarkCanvas(
        spans = spans,
        color = color,
        modifier = modifier.size(size).testTag(FeedbackStates.MARK_TEST_ID),
    )
}

/**
 * The disc + its cut stripes at [spans], painted into its own layer so the
 * cuts CLEAR to whatever is behind the mark. ONE painter for the brand mark
 * ([ExponentialMark], static spans) and the animated [FeedbackMark], off the
 * ONE copy of the stripe geometry ([FeedbackStates.STRIPES]).
 */
@Composable
internal fun MarkCanvas(spans: List<MarkSpan?>, color: Color, modifier: Modifier = Modifier) {
    Canvas(modifier = modifier.graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)) {
        drawExponentialMark(spans, color)
    }
}

private fun DrawScope.drawExponentialMark(spans: List<MarkSpan?>, color: Color) {
    val scale = size.minDimension / 100f
    val disc = Path().apply {
        addOval(androidx.compose.ui.geometry.Rect(Offset.Zero, androidx.compose.ui.geometry.Size(100f * scale, 100f * scale)))
    }
    drawPath(disc, color)
    clipPath(disc) {
        spans.forEachIndexed { index, span ->
            val path = markStripePath(index, span, scale) ?: return@forEachIndexed
            drawPath(
                path,
                Color.Black,
                style = Stroke(width = FeedbackStates.MARK_STRIPE_WIDTH.toFloat() * scale, cap = StrokeCap.Butt),
                blendMode = BlendMode.Clear,
            )
        }
    }
}

/** One stripe's visible span, sampled over [t0, t1] at `mark.samples` steps. */
private fun markStripePath(index: Int, span: MarkSpan?, scale: Float): Path? {
    if (span == null || span.to <= span.from) return null
    val steps = FeedbackStates.MARK_SAMPLES
    val path = Path()
    for (step in 0..steps) {
        val t = span.from + (span.to - span.from) * step / steps
        val (x, y) = FeedbackStates.markStripePoint(index, t)
        if (step == 0) path.moveTo(x.toFloat() * scale, y.toFloat() * scale)
        else path.lineTo(x.toFloat() * scale, y.toFloat() * scale)
    }
    return path
}

/**
 * D8.5 BLOCKING: the window's ONE overlay — only for switching team,
 * switching account and signing out. [whileBlocking] raises it after the
 * pending delay (an instant switch never flashes) for exactly as long as its
 * work runs; [BlockingOverlayHost] (mounted once at the app root) draws it.
 */
object Blocking {
    /** The line the overlay shows, or null while nothing blocks. */
    var label: String? by mutableStateOf(null)
        private set

    private var token = 0

    /** Run [work] behind the overlay, released however it ends. */
    suspend fun <T> whileBlocking(label: String, work: suspend () -> T): T = coroutineScope {
        val mine = ++token
        val show = launch {
            delay(FeedbackStates.PENDING_DELAY_MS)
            if (token == mine) this@Blocking.label = label
        }
        try {
            work()
        } finally {
            show.cancel()
            if (token == mine) this@Blocking.label = null
        }
    }
}

/** The overlay's surface: the window ground at `backdropOpacity`, the mark at 40, one muted line. */
@Composable
fun BlockingSurface(label: String, modifier: Modifier = Modifier, phase: Float? = null) {
    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background.copy(alpha = FeedbackStates.BLOCKING_BACKDROP_OPACITY.toFloat()))
            // Swallow every touch under it: nothing behind is actionable.
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
            .testTag(FeedbackStates.BLOCKING_TEST_ID),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            FeedbackMark(size = FeedbackStates.BLOCKING_MARK_SIZE.dp, phase = phase)
            Spacer(Modifier.height(FeedbackStates.BLOCKING_GAP.dp))
            Text(
                label,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
    }
}

/** Mounted ONCE over the app's content: draws [Blocking]'s overlay while it is up. */
@Composable
fun BlockingOverlayHost() {
    val label = Blocking.label ?: return
    BlockingSurface(label)
}
