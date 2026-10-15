package com.exponential.app.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.TaskQueue
import com.exponential.app.domain.TaskQueueItemState
import com.exponential.app.domain.TaskQueueSpec
import com.exponential.app.domain.TraceGeometry
import com.exponential.app.domain.TraceLineModel
import com.exponential.app.domain.TraceSpec
import com.exponential.app.domain.TraceStatus
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import kotlinx.coroutines.delay
import kotlin.math.abs
import kotlin.math.cbrt
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

// EXP-1324: THE Trace — shadcn's Trace (radix-nova) rebuilt in Compose from
// `packages/domain-contract/fixtures/trace.json` `geometry` (px = dp), the web
// `@exp/ui` primitive's twin. A LINE = [icon][gap][title][gap][description]
// in ONE row of `lineHeight`; a COLLAPSIBLE line appends the chevron
// (`chevronGap` after the line, rotating 90° over motion `standard`) and opens
// a railed content column. `running` paints the title with the design-tokens
// shimmer (reduce motion = plain), `error` the whole line destructive,
// `pending` the line at `pendingOpacity`. Sizes: [TraceSize.Default] (the
// styleguide default) and [TraceSize.Sm] (the session transcript + issue
// activity). Pure models: `domain/Trace.kt`, `domain/TaskQueue.kt`.

enum class TraceSize(val geometry: TraceGeometry) {
    Default(TraceSpec.DEFAULT),
    Sm(TraceSpec.SM),
}

private val LocalTraceSize = staticCompositionLocalOf { TraceSize.Default }

/** The size the enclosing [Trace] set (lines outside one read `Default`). */
val currentTraceSize: TraceSize
    @Composable get() = LocalTraceSize.current

/** muted-foreground: every line's resting ink. */
val TraceMuted: Color = DesignTokens.Palette.MutedForeground

/** A trace concept (packages/icons/icons.json) as its glyph. */
fun traceIcon(concept: String): ImageVector = when (concept) {
    "trace-terminal" -> ExpIcons.traceTerminal
    "trace-read" -> ExpIcons.traceRead
    "trace-edit" -> ExpIcons.traceEdit
    "trace-search" -> ExpIcons.traceSearch
    "trace-fetch" -> ExpIcons.traceFetch
    "trace-reasoning" -> ExpIcons.traceReasoning
    "trace-background" -> ExpIcons.traceBackground
    "trace-subagent" -> ExpIcons.traceSubagent
    "trace-failed" -> ExpIcons.traceFailed
    "trace-chevron" -> ExpIcons.traceChevron
    "trace-todo" -> ExpIcons.traceTodo
    "trace-completed" -> ExpIcons.traceCompleted
    "trace-task-running" -> ExpIcons.traceTaskRunning
    "trace-task-pending" -> ExpIcons.traceTaskPending
    "trace-task-done" -> ExpIcons.traceTaskDone
    else -> ExpIcons.traceTool
}

/** The line font at a size: sans, regular, the size's text + line height. */
fun traceTextStyle(geometry: TraceGeometry): TextStyle = TextStyle(
    fontFamily = FontFamily.Default,
    fontWeight = FontWeight.Normal,
    fontSize = geometry.fontSize.sp,
    lineHeight = geometry.lineHeight.sp,
    letterSpacing = 0.sp,
    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
)

/** Lines stacked `lineGap` apart; sets the size every part below reads. */
@Composable
fun Trace(
    modifier: Modifier = Modifier,
    size: TraceSize = TraceSize.Default,
    content: @Composable ColumnScope.() -> Unit,
) {
    CompositionLocalProvider(LocalTraceSize provides size) {
        Column(
            modifier = modifier,
            verticalArrangement = Arrangement.spacedBy(size.geometry.lineGap.dp),
            content = content,
        )
    }
}

/**
 * One Trace line. [icon] = the concept glyph (muted, destructive on error);
 * [leading] replaces it with any composable in the icon box (a brand mark, a
 * status glyph). [badge] paints the description as the outline Badge (the
 * subagent's duration). [open] non-null makes it a collapsible trigger with
 * the rotating chevron; [chevron] draws a static right chevron (a line that
 * navigates). [onClick] makes the whole row the target; interactive rows read
 * foreground while pressed.
 */
@Composable
fun TraceLine(
    title: AnnotatedString,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    leading: (@Composable () -> Unit)? = null,
    description: String? = null,
    badge: String? = null,
    status: TraceStatus? = null,
    open: Boolean? = null,
    chevron: Boolean = false,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
) {
    val g = currentTraceSize.geometry
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val ink = when {
        status == TraceStatus.Error -> DesignTokens.Palette.Destructive
        onClick != null && pressed -> DesignTokens.Palette.Foreground
        else -> TraceMuted
    }
    val style = traceTextStyle(g)
    Row(
        modifier = modifier
            .then(
                if (onClick != null) {
                    Modifier.clickable(
                        interactionSource = interaction,
                        indication = null,
                        role = Role.Button,
                        onClickLabel = onClickLabel,
                        onClick = onClick,
                    )
                } else {
                    Modifier
                },
            )
            .then(if (status == TraceStatus.Pending) Modifier.alpha(TraceSpec.PENDING_OPACITY) else Modifier)
            .height(g.lineHeight.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null || icon != null) {
            Box(modifier = Modifier.size(g.icon.dp), contentAlignment = Alignment.Center) {
                if (leading != null) {
                    leading()
                } else if (icon != null) {
                    Icon(icon, contentDescription = null, modifier = Modifier.size(g.icon.dp), tint = ink)
                }
            }
            Spacer(Modifier.width(g.gap.dp))
        }
        Text(
            title,
            style = style,
            color = ink,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .weight(1f, fill = false)
                .traceShimmer(ink, status == TraceStatus.Running),
        )
        if (badge != null) {
            Spacer(Modifier.width(g.gap.dp))
            TraceBadge(badge)
        } else if (description != null) {
            Spacer(Modifier.width(g.gap.dp))
            Text(
                description,
                style = style.copy(fontFeatureSettings = "tnum"),
                color = if (status == TraceStatus.Error) ink else TraceMuted.copy(alpha = TraceSpec.DESCRIPTION_ALPHA),
                maxLines = 1,
                softWrap = false,
            )
        }
        if (open != null || chevron) {
            Spacer(Modifier.width(g.chevronGap.dp))
            val rotation by animateFloatAsState(
                targetValue = if (open == true) 90f else 0f,
                animationSpec = Motion.standard(),
                label = "trace-chevron",
            )
            Icon(
                ExpIcons.traceChevron,
                contentDescription = null,
                modifier = Modifier.size(g.icon.dp).rotate(rotation),
                tint = ink,
            )
        }
    }
}

@Composable
fun TraceLine(
    title: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    leading: (@Composable () -> Unit)? = null,
    description: String? = null,
    badge: String? = null,
    status: TraceStatus? = null,
    open: Boolean? = null,
    chevron: Boolean = false,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
) = TraceLine(
    title = AnnotatedString(title),
    modifier = modifier,
    icon = icon,
    leading = leading,
    description = description,
    badge = badge,
    status = status,
    open = open,
    chevron = chevron,
    onClick = onClick,
    onClickLabel = onClickLabel,
)

/** A pure [TraceLineModel] as its line. */
@Composable
fun TraceLine(
    model: TraceLineModel,
    modifier: Modifier = Modifier,
    badge: Boolean = false,
    open: Boolean? = null,
    chevron: Boolean = false,
    onClick: (() -> Unit)? = null,
    onClickLabel: String? = null,
) = TraceLine(
    title = model.title,
    modifier = modifier,
    icon = traceIcon(model.icon),
    description = if (badge) null else model.description,
    badge = if (badge) model.description else null,
    status = model.status,
    open = open,
    chevron = chevron,
    onClick = onClick,
    onClickLabel = onClickLabel,
)

/** A title with a muted trailing subject (our MCP calls: "Created issue" +
 *  the subject at `descriptionAlpha`). */
fun traceTitleWithSubject(title: String, subject: String?): AnnotatedString = buildAnnotatedString {
    append(title)
    if (!subject.isNullOrBlank()) {
        withStyle(SpanStyle(color = TraceMuted.copy(alpha = TraceSpec.DESCRIPTION_ALPHA))) { append(" $subject") }
    }
}

/** The outline Badge at the line scale (web: rounded-full border px-1.5
 *  text-[10px] leading-4, muted at descriptionAlpha, tabular figures). */
@Composable
fun TraceBadge(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = TextStyle(
            fontFamily = FontFamily.Default,
            fontWeight = FontWeight.Normal,
            fontSize = 10.sp,
            lineHeight = 16.sp,
            letterSpacing = 0.sp,
            fontFeatureSettings = "tnum",
            lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None),
        ),
        color = TraceMuted.copy(alpha = TraceSpec.DESCRIPTION_ALPHA),
        maxLines = 1,
        softWrap = false,
        modifier = modifier
            .border(1.dp, DesignTokens.Palette.Border, RoundedCornerShape(50))
            .padding(horizontal = 6.dp),
    )
}

/**
 * A collapsible line: the [TraceLine] trigger (the whole row toggles) over
 * the railed content column, which opens/closes over motion `standard`
 * (reduce motion: at once).
 */
@Composable
fun TraceCollapsible(
    open: Boolean,
    onOpenChange: (Boolean) -> Unit,
    trigger: @Composable (open: Boolean, toggle: () -> Unit) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier = modifier) {
        trigger(open) { onOpenChange(!open) }
        val spec: FiniteAnimationSpec<IntSize> = Motion.standard()
        AnimatedVisibility(
            visible = open,
            enter = expandVertically(spec, expandFrom = Alignment.Top),
            exit = shrinkVertically(spec, shrinkTowards = Alignment.Top),
        ) {
            TraceCollapsibleContent(content = content)
        }
    }
}

/** The collapsible's content: `contentTop` below the row, `contentBottom`
 *  above what follows, the `railWidth` hairline at `railInset`, children
 *  inset `contentInset` / `contentTrailing`, muted line text by default. */
@Composable
fun TraceCollapsibleContent(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val g = currentTraceSize.geometry
    CompositionLocalProvider(LocalTextStyle provides traceTextStyle(g).copy(color = TraceMuted)) {
        Column(
            modifier = modifier
                .padding(top = g.contentTop.dp, bottom = g.contentBottom.dp)
                .fillMaxWidth()
                .drawBehind {
                    val x = g.railInset.dp.toPx()
                    drawRect(
                        color = DesignTokens.Palette.Border,
                        topLeft = Offset(x, 0f),
                        size = androidx.compose.ui.geometry.Size(g.railWidth.dp.toPx(), size.height),
                    )
                }
                .padding(start = g.contentInset.dp, end = g.contentTrailing.dp),
            content = content,
        )
    }
}

// ── Shimmer ──────────────────────────────────────────────────────────────────

/** The design-tokens shimmer colours for a text colour on the dark surface
 *  (packages/exponential-ui `textShimmer`, dark branch): the highlight =
 *  OKLab lightness max(min, l + boost) at alpha + boost; mid = their
 *  alpha-weighted 50% mix. */
internal data class ShimmerColors(val base: Color, val mid: Color, val highlight: Color)

private fun toLinear(c: Float): Double = if (c <= 0.04045f) c / 12.92 else ((c + 0.055) / 1.055).pow(2.4)
private fun toGamma(c: Double): Float {
    val x = c.coerceIn(0.0, 1.0)
    return (if (x <= 0.0031308) 12.92 * x else 1.055 * x.pow(1 / 2.4) - 0.055).toFloat()
}

private fun oklab(color: Color): DoubleArray {
    val r = toLinear(color.red)
    val g = toLinear(color.green)
    val b = toLinear(color.blue)
    val l = cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b)
    val m = cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b)
    val s = cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b)
    return doubleArrayOf(
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    )
}

private fun fromOklab(lab: DoubleArray, alpha: Float): Color {
    val l = (lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2]).pow(3)
    val m = (lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2]).pow(3)
    val s = (lab[0] - 0.0894841775 * lab[1] - 1.291485548 * lab[2]).pow(3)
    return Color(
        red = toGamma(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        green = toGamma(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        blue = toGamma(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
        alpha = alpha.coerceIn(0f, 1f),
    )
}

internal fun shimmerColors(base: Color): ShimmerColors {
    val t = DesignTokens.Motion.Shimmer
    val lab = oklab(base)
    val high = doubleArrayOf(
        min(1.0, max(t.HighlightLightnessMin.toDouble(), lab[0] + t.HighlightLightnessBoost)),
        lab[1],
        lab[2],
    )
    val highAlpha = min(1f, base.alpha + t.HighlightAlphaBoost)
    val sum = base.alpha + highAlpha
    val mid = if (sum > 0f) {
        DoubleArray(3) { (lab[it] * base.alpha + high[it] * highAlpha) / sum }
    } else {
        lab
    }
    return ShimmerColors(base = base, mid = fromOklab(mid, sum / 2f), highlight = fromOklab(high, highAlpha))
}

/**
 * The running title's shimmer (design-tokens `motion.shimmer`): the glyphs
 * filled with a linear gradient at 90° + `angleDeg` whose band (base → mid →
 * highlight → mid → base over ± `spreadPx`) sits at the centre of a gradient
 * 200% of the text width + 2 spread wide, sliding linearly from left of the
 * text to right of it over `durationMs`, forever. Reduce motion = plain.
 */
@Composable
fun Modifier.traceShimmer(base: Color, enabled: Boolean): Modifier {
    if (!enabled || LocalReduceMotion.current) return this
    val t = DesignTokens.Motion.Shimmer
    val colors = remember(base) { shimmerColors(base) }
    val progress by rememberInfiniteTransition(label = "trace-shimmer").animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(t.DurationMs.toInt(), easing = LinearEasing), RepeatMode.Restart),
        label = "trace-shimmer-progress",
    )
    return this
        .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
        .drawWithContent {
            drawContent()
            val w = size.width
            val h = size.height
            val spread = t.SpreadPx.dp.toPx()
            // Highlight centre: -spread → w + spread (CSS background-position
            // 100% → 0% over a 2w + 2·spread wide image).
            val cx = -spread + progress * (w + 2 * spread)
            val angle = Math.toRadians(90.0 + t.AngleDeg)
            val dx = sin(angle).toFloat()
            val dy = -cos(angle).toFloat()
            val imageWidth = 2 * w + 2 * spread
            val length = imageWidth * abs(dx) + h * abs(dy)
            val half = length / 2f
            fun stop(px: Float) = (0.5f + px / length).coerceIn(0f, 1f)
            val brush = Brush.linearGradient(
                0f to colors.base,
                stop(-spread) to colors.base,
                stop(-spread / 2f) to colors.mid,
                0.5f to colors.highlight,
                stop(spread / 2f) to colors.mid,
                stop(spread) to colors.base,
                1f to colors.base,
                start = Offset(cx - dx * half, h / 2f - dy * half),
                end = Offset(cx + dx * half, h / 2f + dy * half),
            )
            drawRect(brush = brush, blendMode = BlendMode.SrcIn)
        }
}

// ── The Queue (task-queue.json) ──────────────────────────────────────────────

/**
 * EXP-1321/1324: THE Queue — a `sm` Trace of two collapsibles: "n to do"
 * (open by default) listing the running work (shimmering) then the pending
 * tasks (at pendingOpacity), then "n completed" (collapsed) whose items sit
 * at pendingOpacity. The open state is the caller's (it sticks for the
 * session).
 */
@Composable
fun TaskQueueView(
    queue: TaskQueue,
    todoOpen: Boolean,
    onTodoOpenChange: (Boolean) -> Unit,
    completedOpen: Boolean,
    onCompletedOpenChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Trace(modifier = modifier.testTag(TaskQueueSpec.TEST_ID_QUEUE), size = TraceSize.Sm) {
        val sections = listOfNotNull(
            Triple(queue.todo, todoOpen to onTodoOpenChange, TaskQueueSpec.TEST_ID_TODO),
            queue.completed?.let { Triple(it, completedOpen to onCompletedOpenChange, TaskQueueSpec.TEST_ID_COMPLETED) },
        )
        for ((section, state, tag) in sections) {
            val (open, onChange) = state
            TraceCollapsible(
                open = open,
                onOpenChange = onChange,
                modifier = Modifier.testTag(tag),
                trigger = { isOpen, toggle ->
                    TraceLine(
                        title = section.title,
                        icon = traceIcon(section.icon),
                        open = isOpen,
                        onClick = toggle,
                    )
                },
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(TraceSize.Sm.geometry.lineGap.dp)) {
                    for (item in section.items) {
                        TraceLine(
                            title = item.label,
                            icon = traceIcon(item.icon),
                            description = item.description,
                            status = when (item.state) {
                                TaskQueueItemState.Running -> TraceStatus.Running
                                TaskQueueItemState.Pending, TaskQueueItemState.Done -> TraceStatus.Pending
                            },
                        )
                    }
                }
            }
        }
    }
}

/** A 1s clock while [ticking] (the Queue's background durations). */
@Composable
fun rememberTickingNow(ticking: Boolean): Long {
    val now = remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(ticking) {
        now.longValue = System.currentTimeMillis()
        while (ticking) {
            delay(1_000)
            now.longValue = System.currentTimeMillis()
        }
    }
    return now.longValue
}

/** Hugs the line's own width (w-fit) inside a full-width column. */
fun Modifier.traceFit(): Modifier = this.wrapContentWidth(Alignment.Start)
