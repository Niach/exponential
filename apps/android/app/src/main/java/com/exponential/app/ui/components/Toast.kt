package com.exponential.app.ui.components

import android.os.SystemClock
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.clickable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.Animatable
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.ToastItem
import com.exponential.app.domain.ToastKind
import com.exponential.app.domain.ToastStack
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard
import kotlinx.coroutines.delay
import kotlin.math.abs
import kotlin.math.sign

/**
 * EXP-1031: THE toast (web `@exp/ui` toast, desktop `crates/ui` toast, iOS
 * ExpUI Toast). One [Toaster] per app, provided as [LocalToaster] by
 * AppNavHost and drawn by ONE [ToastHost] at the top of the screen (contract
 * `placement.touch` = top-center); screens call
 * `LocalToaster.current.error("…")` and never host a snackbar of their own.
 * Geometry = [ToastStack.geometry] (fixture-locked), motion = [Motion].
 */
@Stable
class Toaster {
    /** Live toasts, OLDEST first — the newest draws in front. */
    val toasts: SnapshotStateList<ToastItem> = mutableStateListOf()

    /** Tap expands the stack; the auto-dismiss clocks pause while it is. */
    var expanded: Boolean by mutableStateOf(false)

    /** Live + the ones still animating out. */
    val rendered: SnapshotStateList<ToastItem> = mutableStateListOf()

    // Remaining auto-dismiss time per toast, so a pause resumes rather than restarts.
    internal val remainingMs = HashMap<String, Long>()
    internal val heights = mutableStateMapOf<String, Double>()
    internal val lastGeometry = HashMap<String, ToastStack.ItemGeometry>()
    private var counter = 0L

    fun show(item: ToastItem): String {
        toasts.removeAll { it.id == item.id }
        rendered.removeAll { it.id == item.id }
        remainingMs.remove(item.id)
        toasts.add(item)
        rendered.add(item)
        return item.id
    }

    fun success(title: String, description: String? = null, action: ToastAction? = null): String =
        show(ToastKind.Success, title, description, action)

    fun error(title: String, description: String? = null, action: ToastAction? = null): String =
        show(ToastKind.Error, title, description, action)

    fun info(title: String, description: String? = null, action: ToastAction? = null): String =
        show(ToastKind.Info, title, description, action)

    fun warning(title: String, description: String? = null, action: ToastAction? = null): String =
        show(ToastKind.Warning, title, description, action)

    fun show(
        kind: ToastKind,
        title: String,
        description: String? = null,
        action: ToastAction? = null,
    ): String = show(ToastItem("toast-${++counter}", kind, title, description, action))

    fun dismiss(id: String) {
        toasts.removeAll { it.id == id }
        remainingMs.remove(id)
        if (toasts.isEmpty()) expanded = false
    }

    /** Drops an exited toast's leftovers (called once its exit animation ends). */
    internal fun forget(id: String) {
        if (toasts.any { it.id == id }) return
        rendered.removeAll { it.id == id }
        heights.remove(id)
        lastGeometry.remove(id)
    }
}

/** The app's one [Toaster]; AppNavHost provides it above both nav graphs. */
val LocalToaster = staticCompositionLocalOf<Toaster> {
    error("LocalToaster is not provided — AppNavHost provides it and mounts the ToastHost")
}

/** The host/card numbers, every one derived from [ToastStack.Constants] or a token. */
object ToastDefaults {
    /** Phones: full width minus this on each side (`mobileViewportOffset`). */
    val HorizontalInset: Dp = ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp

    /** The gap between the status bar and the stack's top. */
    val TopGap: Dp = ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp

    /** Tablets: the pointer-width card (`width`), still top-centre. */
    val Width: Dp = ToastStack.Constants.WIDTH.dp

    /** At or above this available width (`touchMaxWidth`) the card stops filling the row. */
    val TabletBreakpoint: Dp = ToastStack.Constants.TOUCH_MAX_WIDTH.dp

    /** Android is a touch surface: contract `placement.touch`. */
    const val PLACEMENT: String = ToastStack.Constants.PLACEMENT_TOUCH

    /** Where [ToastHost] sits on screen — top-centre, never the bottom. */
    val HostAlignment: Alignment = Alignment.TopCenter

    /** What [ToastStackBox] passes to [ToastStack.geometry]: a TOP stack. */
    const val ANCHORED_BOTTOM: Boolean = false

    val SwipeThreshold: Dp = ToastStack.Constants.SWIPE_THRESHOLD.dp
    val CornerRadius: Dp = DesignTokens.Radius.Lg
    val IconSize: Dp = 16.dp
    val CloseGlyphSize: Dp = 14.dp
    val CloseButtonSize: Dp = 28.dp

    /** Before a card is measured it stands in at this height. */
    const val FALLBACK_HEIGHT = 56.0

    /** The card width for [available] (the row minus the side insets). */
    fun cardWidth(available: Dp): Dp = if (available >= TabletBreakpoint) Width else available

    fun color(kind: ToastKind): Color = when (kind) {
        ToastKind.Success -> DesignTokens.Semantic.Green
        ToastKind.Error -> DesignTokens.Semantic.Red
        ToastKind.Info -> DesignTokens.Semantic.Blue
        ToastKind.Warning -> DesignTokens.Semantic.Yellow
    }

    fun icon(kind: ToastKind): ImageVector = when (kind) {
        ToastKind.Success -> ExpIcons.uiSuccess
        ToastKind.Error -> ExpIcons.uiError
        ToastKind.Info -> ExpIcons.uiInfo
        ToastKind.Warning -> ExpIcons.uiWarning
    }
}

/**
 * One toast card: kind colour on the icon ONLY, opaque glass card, no shadow.
 * [height] forces the card's height (a collapsed BACK card wears the front
 * card's, sonner's rule) while the content keeps its natural height, reported
 * through [onNaturalHeight]; [contentAlpha] fades the content (0 on back cards).
 */
@Composable
fun ToastCard(
    item: ToastItem,
    onDismiss: () -> Unit,
    onExpandToggle: () -> Unit,
    modifier: Modifier = Modifier,
    height: Dp? = null,
    contentAlpha: Float = 1f,
    onNaturalHeight: (Dp) -> Unit = {},
) {
    val onSurface = MaterialTheme.colorScheme.onSurface
    val density = LocalDensity.current
    Box(
        contentAlignment = Alignment.TopCenter,
        modifier = modifier
            .then(if (height != null) Modifier.height(height) else Modifier)
            .glassCard(opaque = true, cornerRadius = ToastDefaults.CornerRadius)
            .clipToBounds()
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onExpandToggle,
            )
            .testTag("toast-${item.kind.wire}"),
    ) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier
            .fillMaxWidth()
            .wrapContentHeight(Alignment.Top, unbounded = true)
            .onSizeChanged { onNaturalHeight(with(density) { it.height.toDp() }) }
            .graphicsLayer { alpha = contentAlpha }
            .padding(start = 14.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
    ) {
        Icon(
            ToastDefaults.icon(item.kind),
            contentDescription = null,
            tint = ToastDefaults.color(item.kind),
            modifier = Modifier.size(ToastDefaults.IconSize),
        )
        Column(Modifier.weight(1f).padding(vertical = 4.dp)) {
            Text(
                item.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.SemiBold,
                color = onSurface,
            )
            item.description?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    color = onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
        }
        item.action?.let { action ->
            TextButton(
                onClick = {
                    action.onClick()
                    onDismiss()
                },
            ) {
                Text(action.label, style = MaterialTheme.typography.labelMedium, color = onSurface)
            }
        }
        IconButton(onClick = onDismiss, modifier = Modifier.size(ToastDefaults.CloseButtonSize)) {
            Icon(
                ExpIcons.uiClose,
                contentDescription = "Dismiss",
                tint = onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.size(ToastDefaults.CloseGlyphSize),
            )
        }
    }
    }
}

/**
 * The stack, TOP-centre (contract `placement.touch`): `mobileViewportOffset`
 * below the status bar, the row minus that inset on each side. Only the cards
 * take touches; the rest of the row passes them to the screen underneath.
 */
@Composable
fun ToastHost(toaster: Toaster, modifier: Modifier = Modifier) {
    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .padding(top = ToastDefaults.TopGap)
            .padding(horizontal = ToastDefaults.HorizontalInset),
        contentAlignment = ToastDefaults.HostAlignment,
    ) {
        ToastStackBox(toaster, Modifier.width(ToastDefaults.cardWidth(maxWidth)))
    }
}

/**
 * The bare stack: exactly as wide as [modifier] makes it, exactly as tall as
 * [ToastStack.geometry] says (a TOP stack: older cards peek out BELOW the
 * front one, expanded ones grow downward).
 */
@Composable
fun ToastStackBox(toaster: Toaster, modifier: Modifier = Modifier) {
    val live = toaster.toasts
    val rendered = toaster.rendered
    val heights = toaster.heights
    val liveIds = live.map { it.id }.toSet()

    val expanded = toaster.expanded
    // Sonner's collapsed rule: every back card wears the FRONT card's height
    // (content hidden), so a short older card is never swallowed and a tall
    // one never bleeds; expanded, every card is its own height again.
    val frontHeight = live.lastOrNull()?.let { heights[it.id] } ?: ToastDefaults.FALLBACK_HEIGHT
    val layout = ToastStack.geometry(
        heights = live.map {
            if (expanded) heights[it.id] ?: ToastDefaults.FALLBACK_HEIGHT else frontHeight
        },
        expanded = expanded,
        anchoredBottom = ToastDefaults.ANCHORED_BOTTOM,
    )
    live.forEachIndexed { i, item -> toaster.lastGeometry[item.id] = layout.items[i] }
    val boxHeight by animateFloatAsState(layout.height.toFloat(), Motion.slow(), label = "toast-stack-height")

    val density = LocalDensity.current
    val slow = Motion.slow<Float>()
    val standard = Motion.standard<Float>()

    // Auto-dismiss: 4 s per toast, paused (remaining kept) while expanded.
    for (item in live) {
        key(item.id) {
            LaunchedEffect(item.id, expanded) {
                if (expanded) return@LaunchedEffect
                val remaining = toaster.remainingMs[item.id] ?: ToastStack.Constants.DURATION_MS
                val started = SystemClock.uptimeMillis()
                try {
                    delay(remaining)
                    toaster.dismiss(item.id)
                } finally {
                    if (toaster.toasts.any { it.id == item.id }) {
                        toaster.remainingMs[item.id] =
                            (remaining - (SystemClock.uptimeMillis() - started)).coerceAtLeast(0)
                    }
                }
            }
        }
    }

    BoxWithConstraints(modifier.height(boxHeight.coerceAtLeast(0f).dp)) {
        val cardWidthPx = with(density) { maxWidth.toPx() }
        for (item in rendered.toList()) {
            key(item.id) {
                val state = remember { MutableTransitionState(false) }
                state.targetState = item.id in liveIds
                if (state.isIdle && !state.currentState && !state.targetState) {
                    LaunchedEffect(Unit) { toaster.forget(item.id) }
                }
                val geo = toaster.lastGeometry[item.id] ?: ToastStack.ItemGeometry(0.0, 1.0, true)
                val liveIndex = live.indexOfFirst { it.id == item.id }
                val back = !expanded && liveIndex in 0 until live.size - 1
                // The card height: the front's while back, its own otherwise
                // (animated between the two); unforced until first measured.
                val targetHeight = when {
                    liveIndex < 0 -> null
                    back -> frontHeight.toFloat()
                    else -> heights[item.id]?.toFloat()
                }
                var heightAnim by remember { mutableStateOf<Animatable<Float, AnimationVector1D>?>(null) }
                LaunchedEffect(targetHeight) {
                    val target = targetHeight ?: return@LaunchedEffect
                    val current = heightAnim
                    if (current == null) heightAnim = Animatable(target) else current.animateTo(target, slow)
                }
                val contentAlpha by animateFloatAsState(if (back) 0f else 1f, slow, label = "toast-content-alpha")
                val offset by animateFloatAsState(geo.offset.toFloat(), slow, label = "toast-offset")
                val scale by animateFloatAsState(geo.scale.toFloat(), slow, label = "toast-scale")
                val alpha by animateFloatAsState(if (geo.visible) 1f else 0f, slow, label = "toast-alpha")
                // The swipe: sideways either way, or UP (a downward drag is
                // clamped to 0). The axis locks on the first movement. Written
                // synchronously per delta, animated only on release.
                var dx by remember { mutableFloatStateOf(0f) }
                var dy by remember { mutableFloatStateOf(0f) }
                var axis by remember { mutableStateOf<Orientation?>(null) }
                val thresholdPx = with(density) { ToastDefaults.SwipeThreshold.toPx() }
                val isLive = item.id in liveIds
                AnimatedVisibility(
                    visibleState = state,
                    enter = fadeIn(standard) + slideInVertically(Motion.standard()) { -it },
                    exit = fadeOut(standard) + slideOutVertically(Motion.standard()) { -it / 2 },
                    modifier = Modifier
                        .fillMaxWidth()
                        .offset { IntOffset(0, with(density) { offset.dp.roundToPx() }) },
                ) {
                    val scope = rememberCoroutineScope()
                    ToastCard(
                        item = item,
                        onDismiss = { toaster.dismiss(item.id) },
                        onExpandToggle = { toaster.expanded = !toaster.expanded },
                        height = heightAnim?.value?.dp,
                        contentAlpha = contentAlpha,
                        onNaturalHeight = { heights[item.id] = it.value.toDouble() },
                        modifier = Modifier
                            .fillMaxWidth()
                            .graphicsLayer {
                                translationX = dx
                                translationY = dy
                                scaleX = scale
                                scaleY = scale
                                val travel = maxOf(abs(dx) / cardWidthPx, abs(dy) / size.height.coerceAtLeast(1f))
                                this.alpha = alpha * (1f - travel.coerceIn(0f, 1f))
                                // A top stack shrinks from its TOP edge.
                                transformOrigin = TransformOrigin(0.5f, 0f)
                            }
                            .pointerInput(isLive) {
                                if (!isLive) return@pointerInput
                                detectDragGestures(
                                    onDragStart = { axis = null },
                                    onDrag = { change, delta ->
                                        change.consume()
                                        val locked = axis ?: (
                                            if (abs(delta.x) >= abs(delta.y)) Orientation.Horizontal
                                            else Orientation.Vertical
                                            ).also { axis = it }
                                        if (locked == Orientation.Horizontal) dx += delta.x
                                        else dy = (dy + delta.y).coerceAtMost(0f)
                                    },
                                    onDragCancel = {
                                        scope.launch {
                                            launch { animate(dx, 0f, animationSpec = standard) { v, _ -> dx = v } }
                                            animate(dy, 0f, animationSpec = standard) { v, _ -> dy = v }
                                        }
                                    },
                                    onDragEnd = {
                                        val horizontal = axis == Orientation.Horizontal
                                        val travel = if (horizontal) dx else dy
                                        val past = abs(travel) > thresholdPx
                                        val far = if (horizontal) cardWidthPx else size.height.toFloat()
                                        val target = if (past) sign(travel) * far else 0f
                                        scope.launch {
                                            try {
                                                animate(travel, target, animationSpec = standard) { v, _ ->
                                                    if (horizontal) dx = v else dy = v
                                                }
                                            } finally {
                                                if (past) toaster.dismiss(item.id)
                                            }
                                        }
                                    },
                                )
                            },
                    )
                }
            }
        }
    }
}
