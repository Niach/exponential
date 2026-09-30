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
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
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
import androidx.compose.runtime.mutableIntStateOf
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
 * AppNavHost and drawn by ONE [ToastWindow] (its own window, above every
 * sheet and dialog); screens call `LocalToaster.current.error("…")` and never
 * host a snackbar of their own. Geometry = [ToastStack.geometry]
 * (fixture-locked), motion = [Motion].
 *
 * Everything a window re-creation must keep (the rendered list, measured
 * heights, enter/exit states, timers, expansion) lives HERE, not in the
 * window's composition: every [show] bumps [generation], which re-raises the
 * window above whatever opened since.
 */
@Stable
class Toaster(private val clock: () -> Long = { System.nanoTime() / 1_000_000 }) {
    /** Live toasts, OLDEST first — the newest draws in front. */
    val toasts: SnapshotStateList<ToastItem> = mutableStateListOf()

    /** Tap expands the stack; the auto-dismiss clocks pause while it is. */
    var expanded: Boolean by mutableStateOf(false)

    /**
     * Bumped by every [show] (and [raise]): the toast window re-creates
     * itself on top of whatever window opened since.
     */
    var generation: Int by mutableIntStateOf(0)
        private set

    /**
     * What the stack must clear above the navigation bar: the shell writes
     * its bottom nav bar + banner height here (0 while signed out).
     */
    var bottomInset: Dp by mutableStateOf(0.dp)

    /** Live + the ones still animating out; the window shows while non-empty. */
    val rendered: SnapshotStateList<ToastItem> = mutableStateListOf()

    // Remaining auto-dismiss time per toast, so a pause resumes rather than restarts.
    internal val remainingMs = HashMap<String, Long>()
    internal val heights = mutableStateMapOf<String, Double>()
    internal val lastGeometry = HashMap<String, ToastStack.ItemGeometry>()
    internal val visibility = HashMap<String, MutableTransitionState<Boolean>>()
    private val shownAt = HashMap<String, Long>()
    private var counter = 0L

    fun show(item: ToastItem): String {
        toasts.removeAll { it.id == item.id }
        rendered.removeAll { it.id == item.id }
        remainingMs.remove(item.id)
        visibility.remove(item.id)
        toasts.add(item)
        rendered.add(item)
        shownAt[item.id] = clock()
        generation++
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

    /** Re-raises the window over a sheet/dialog that opened above a showing toast. */
    fun raise() {
        if (rendered.isNotEmpty()) generation++
    }

    fun dismiss(id: String) {
        toasts.removeAll { it.id == id }
        remainingMs.remove(id)
        if (toasts.isEmpty()) expanded = false
    }

    /**
     * Whether [id] is still inside its enter animation. A re-created window
     * animates only these in; older toasts appear where they already were.
     */
    fun isEntering(id: String): Boolean =
        clock() - (shownAt[id] ?: Long.MIN_VALUE / 2) < ToastDefaults.ENTER_MS

    /** The enter/exit state for [id], shared by every window that draws it. */
    internal fun visibilityOf(id: String): MutableTransitionState<Boolean> =
        visibility.getOrPut(id) { MutableTransitionState(!isEntering(id)).apply { targetState = true } }

    /** Drops an exited toast's leftovers (called once its exit animation ends). */
    internal fun forget(id: String) {
        if (toasts.any { it.id == id }) return
        rendered.removeAll { it.id == id }
        visibility.remove(id)
        heights.remove(id)
        lastGeometry.remove(id)
        shownAt.remove(id)
    }
}

/** The app's one [Toaster]; AppNavHost provides it above both nav graphs. */
val LocalToaster = staticCompositionLocalOf<Toaster> {
    error("LocalToaster is not provided — AppNavHost provides it and mounts the ToastWindow")
}

/** The host/card numbers, every one derived from [ToastStack.Constants] or a token. */
object ToastDefaults {
    /** Phones: full width minus this on each side (`mobileViewportOffset`). */
    val HorizontalInset: Dp = ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp

    /** The gap between the stack and whatever it sits above. */
    val BottomGap: Dp = ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp

    /** Tablets: the pointer-width card (`width`), bottom-centre. */
    val Width: Dp = ToastStack.Constants.WIDTH.dp

    /** At or above this available width the card stops filling the row. */
    val TabletBreakpoint: Dp = 600.dp

    val SwipeThreshold: Dp = ToastStack.Constants.SWIPE_THRESHOLD.dp
    val CornerRadius: Dp = DesignTokens.Radius.Lg
    val IconSize: Dp = 16.dp
    val CloseGlyphSize: Dp = 14.dp
    val CloseButtonSize: Dp = 28.dp

    /** Before a card is measured it stands in at this height. */
    const val FALLBACK_HEIGHT = 56.0

    /** A toast younger than this is still entering (the standard motion rung). */
    const val ENTER_MS: Long = DesignTokens.Motion.Duration.Standard.toLong()

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
 * The stack, bottom-centre, inside a full-width row. [bottomInset] = what it
 * must clear; the caller applies the system-bar insets. The app draws the
 * stack through [ToastWindow]; this padded form serves previews/styleguide.
 */
@Composable
fun ToastHost(toaster: Toaster, modifier: Modifier = Modifier, bottomInset: Dp = 0.dp) {
    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = ToastDefaults.HorizontalInset)
            .padding(bottom = bottomInset + ToastDefaults.BottomGap),
        contentAlignment = Alignment.BottomCenter,
    ) {
        ToastStackBox(toaster, Modifier.width(ToastDefaults.cardWidth(maxWidth)))
    }
}

/**
 * The bare stack: exactly as wide as [modifier] makes it, exactly as tall as
 * [ToastStack.geometry] says — [ToastWindow] sizes its window to it, so
 * nothing beside or above the cards swallows a touch.
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
        anchoredBottom = true,
    )
    live.forEachIndexed { i, item -> toaster.lastGeometry[item.id] = layout.items[i] }
    val boxHeight by animateFloatAsState(layout.height.toFloat(), Motion.slow(), label = "toast-stack-height")

    val density = LocalDensity.current
    val slow = Motion.slow<Float>()
    val standard = Motion.standard<Float>()

    // Auto-dismiss: 4 s per toast, paused (remaining kept) while expanded or
    // while the window is being re-created.
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
                val state = toaster.visibilityOf(item.id)
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
                // The swipe offset: written synchronously per drag delta,
                // animated only on release (a launched snapTo per delta
                // raced — and cancelled — the release animation).
                var dx by remember { mutableFloatStateOf(0f) }
                val thresholdPx = with(density) { ToastDefaults.SwipeThreshold.toPx() }
                AnimatedVisibility(
                    visibleState = state,
                    enter = fadeIn(standard) + slideInVertically(Motion.standard()) { it },
                    exit = fadeOut(standard) + slideOutVertically(Motion.standard()) { it / 2 },
                    modifier = Modifier
                        .fillMaxWidth()
                        .offset { IntOffset(0, with(density) { offset.dp.roundToPx() }) },
                ) {
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
                                scaleX = scale
                                scaleY = scale
                                this.alpha = alpha *
                                    (1f - (abs(dx) / cardWidthPx).coerceIn(0f, 1f))
                                transformOrigin = TransformOrigin(0.5f, 1f)
                            }
                            .draggable(
                                orientation = Orientation.Horizontal,
                                enabled = item.id in liveIds,
                                state = rememberDraggableState { delta -> dx += delta },
                                onDragStopped = {
                                    val past = abs(dx) > thresholdPx
                                    val target = if (past) sign(dx) * cardWidthPx else 0f
                                    try {
                                        animate(dx, target, animationSpec = standard) { v, _ -> dx = v }
                                    } finally {
                                        if (past) toaster.dismiss(item.id)
                                    }
                                },
                            ),
                    )
                }
            }
        }
    }
}
