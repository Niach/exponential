package com.exponential.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Chats
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

// #EXP-2109 (chats.json `shell.bar`): the floating glass tab CAPSULE with the
// drag lens, only a running app's own screens bar (apps.json); the phone's
// bottom bar is [PhoneDock].
// DRAG (`shell.bar.drag`): a press lifts the selection LENS under the finger,
// a drag moves it along the bar, the lift selects the tab under it (a lift
// far above / below cancels).

// Bottom contentPadding for scrollable content on screens the docked bar
// overlays (`shell.dock`: its height + the Assistant disc's raise + breathing
// room, above the system nav inset). Every LazyColumn under the bar must use
// this so its last row scrolls clear (EXP-36).
val BottomBarInset = 96.dp

/**
 * The nav bar's off switch (EXP-698 r5, Mechanism A — iOS `TabBarChrome`).
 *
 * A screen that puts a bar of its own in the tab bar's slot — today only the
 * issue list's multi-select bar — flips [suppressed] while it is up, and the
 * tab bar + its FAB slide out instead of stacking above it. Two floating
 * capsules in the same corner is the one thing the bottom of the screen has
 * no room for.
 */
class BottomBarSuppression {
    var suppressed by mutableStateOf(false)
}

/** Null outside the nav shell (previews, tests) — every read is optional. */
val LocalBottomBarSuppression = staticCompositionLocalOf<BottomBarSuppression?> { null }

/** One tab of the capsule, as the bar draws it. */
private class PhoneTab(
    val key: String,
    val label: String,
    val testTag: String,
    val glyph: @Composable () -> Unit,
)

private val TabGlyphSize = 20.dp

/** One tab of [PhoneTabCapsule]: an icon over its label. */
data class PhoneTabItem(val key: String, val label: String, val icon: ImageVector, val testTag: String)

/**
 * VAPP-218: the floating capsule over the caller's tabs: an app's screens
 * under the surface inside the preview frame. No inset.
 */
@Composable
fun PhoneTabCapsule(
    items: List<PhoneTabItem>,
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    TabCapsule(
        tabs = items.map { item -> PhoneTab(key = item.key, label = item.label, testTag = item.testTag, glyph = { TabIcon(item.icon) }) },
        selected = selected,
        onSelectTab = onSelect,
        modifier = modifier,
    )
}

@Composable
private fun TabIcon(icon: ImageVector) {
    Icon(icon, null, modifier = Modifier.size(TabGlyphSize), tint = LocalContentColor.current)
}

/** Where the tabs sit inside the capsule's content box (px), written at
 *  placement, read by the drag. */
private class TabGeometry {
    var lefts: FloatArray = FloatArray(0)
    var widths: FloatArray = FloatArray(0)
    var contentWidth: Float = 0f

    val count: Int get() = lefts.size

    /** The tab under [x] (content px), else the nearest one. */
    fun indexAt(x: Float): Int {
        var best = 0
        var bestDistance = Float.MAX_VALUE
        for (i in 0 until count) {
            val left = lefts[i]
            val right = left + widths[i]
            if (x in left..right) return i
            val distance = minOf(kotlin.math.abs(x - left), kotlin.math.abs(x - right))
            if (distance < bestDistance) {
                bestDistance = distance
                best = i
            }
        }
        return best
    }
}

/**
 * The tab capsule: the tabs laid out by [Chats.Shell.Bar.tabWidths] and THE
 * drag (`shell.bar.drag`). The tabs carry no click of their own (a clickable
 * child would eat the press the lens needs): one gesture loop on the capsule
 * owns the press, the drag and the lift; each tab's semantics click is the
 * same select, so accessibility and tests address the tabs as buttons.
 */
@Composable
private fun TabCapsule(
    tabs: List<PhoneTab>,
    selected: String?,
    onSelectTab: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduce = LocalReduceMotion.current
    val scope = rememberCoroutineScope()
    val geometry = remember { TabGeometry() }
    val lensLeft = remember { Animatable(0f) }
    val lensWidth = remember { Animatable(0f) }
    val lensScale = remember { Animatable(1f) }
    // A press is up: the selected tab's pill lifts off, the candidate lights.
    var lifted by remember { mutableStateOf(false) }
    var candidate by remember { mutableIntStateOf(-1) }
    var settleJob by remember { mutableStateOf<Job?>(null) }
    val keys = tabs.map { it.key }
    val selectedIndex = keys.indexOf(selected)
    val latestSelect by rememberUpdatedState(onSelectTab)
    val latestSelectedIndex by rememberUpdatedState(selectedIndex)

    fun settleOn(index: Int) {
        settleJob?.cancel()
        settleJob = scope.launch {
            if (index in 0 until geometry.count) {
                val spec = tween<Float>(DesignTokens.Motion.Duration.Standard, easing = DesignTokens.Motion.Ease.Decelerate)
                coroutineScope {
                    launch { lensLeft.animateTo(geometry.lefts[index], spec) }
                    launch { lensWidth.animateTo(geometry.widths[index], spec) }
                    launch { lensScale.animateTo(1f, spec) }
                }
            }
            lifted = false
            candidate = -1
        }
    }

    Box(
        modifier = modifier
            .height(BottomNavDefaults.BarHeight)
            .clip(RoundedCornerShape(percent = 50))
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(percent = 50))
            .pointerInput(keys, reduce) {
                val padPx = Chats.Shell.Bar.PADDING.dp.toPx()
                val cancelPx = Chats.Shell.Bar.Drag.CANCEL_DISTANCE.dp.toPx()
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    if (geometry.count == 0) return@awaitEachGesture
                    val pressed = geometry.indexAt(down.position.x - padPx)
                    settleJob?.cancel()
                    candidate = pressed
                    lifted = true
                    scope.launch {
                        lensLeft.snapTo(geometry.lefts[pressed])
                        lensWidth.snapTo(geometry.widths[pressed])
                        lensScale.snapTo(if (reduce) 1f else Chats.Shell.Bar.Drag.LENS_SCALE)
                    }
                    var dragging = false
                    var lifting: PointerInputChange? = null
                    try {
                        while (true) {
                            val event = awaitPointerEvent()
                            val change = event.changes.firstOrNull { it.id == down.id } ?: break
                            if (!change.pressed) {
                                lifting = change
                                break
                            }
                            if (!dragging && (change.position - down.position).getDistance() > viewConfiguration.touchSlop) {
                                dragging = true
                            }
                            if (dragging) {
                                // The lens follows the finger (clamped to the
                                // bar, no animation); the tab under its centre
                                // is the candidate.
                                val x = change.position.x.coerceIn(padPx, size.width - padPx) - padPx
                                val index = geometry.indexAt(x)
                                val width = geometry.widths[index]
                                val left = (x - width / 2f).coerceIn(0f, (geometry.contentWidth - width).coerceAtLeast(0f))
                                candidate = index
                                scope.launch {
                                    lensLeft.snapTo(left)
                                    lensWidth.snapTo(width)
                                }
                                change.consume()
                            }
                        }
                    } finally {
                        val up = lifting
                        when {
                            // Cancelled (the gesture left us): back to the
                            // selected tab.
                            up == null -> settleOn(latestSelectedIndex)
                            // A plain tap selects at once.
                            !dragging -> {
                                settleOn(pressed)
                                latestSelect(keys[pressed])
                            }
                            else -> {
                                up.consume()
                                val y = up.position.y
                                val off = when {
                                    y < 0f -> -y
                                    y > size.height -> y - size.height
                                    else -> 0f
                                }
                                if (off > cancelPx) {
                                    settleOn(latestSelectedIndex)
                                } else {
                                    val target = candidate.takeIf { it in keys.indices } ?: pressed
                                    settleOn(target)
                                    latestSelect(keys[target])
                                }
                            }
                        }
                    }
                }
            }
            .padding(Chats.Shell.Bar.PADDING.dp),
    ) {
        // THE LENS: the glass-active pill under the finger, scaled while lifted.
        if (lifted) {
            Box(
                Modifier
                    .fillMaxHeight()
                    .layout { measurable, constraints ->
                        val width = lensWidth.value.roundToInt().coerceAtLeast(0)
                        val placeable = measurable.measure(Constraints.fixed(width, constraints.maxHeight))
                        layout(width, constraints.maxHeight) {
                            placeable.place(lensLeft.value.roundToInt(), 0)
                        }
                    }
                    .graphicsLayer {
                        scaleX = lensScale.value
                        scaleY = lensScale.value
                    }
                    .background(GlassTokens.RowFillActive, RoundedCornerShape(percent = 50)),
            )
        }
        Layout(
            content = {
                tabs.forEachIndexed { index, tab ->
                    TabItem(
                        tab = tab,
                        active = index == selectedIndex,
                        lifted = lifted,
                        candidate = index == candidate,
                        onSelect = { onSelectTab(tab.key) },
                    )
                }
            },
            modifier = Modifier.fillMaxHeight(),
        ) { measurables, constraints ->
            val height = constraints.maxHeight
            val available = constraints.maxWidth.toFloat()
            val preferred = measurables.map { it.maxIntrinsicWidth(height).toFloat() }
            val widths = Chats.Shell.Bar.tabWidths(preferred, available)
            val placeables = measurables.mapIndexed { index, measurable ->
                measurable.measure(Constraints.fixed(widths[index].roundToInt().coerceAtLeast(0), height))
            }
            val lefts = FloatArray(placeables.size)
            var x = 0
            placeables.forEachIndexed { index, placeable ->
                lefts[index] = x.toFloat()
                x += placeable.width
            }
            geometry.lefts = lefts
            geometry.widths = FloatArray(placeables.size) { placeables[it].width.toFloat() }
            geometry.contentWidth = constraints.maxWidth.toFloat()
            layout(constraints.maxWidth, height) {
                placeables.forEachIndexed { index, placeable -> placeable.place(lefts[index].roundToInt(), 0) }
            }
        }
    }
}

/** One tab: its glyph over its label, the selected one on the
 *  glass-active pill (cross-faded, EXP-523) unless a press lifted it. */
@Composable
private fun TabItem(
    tab: PhoneTab,
    active: Boolean,
    lifted: Boolean,
    candidate: Boolean,
    onSelect: () -> Unit,
) {
    // The lens settles ON the active tab and vanishes the moment the lift
    // ends, so the pill must be back at once: a fade-in here would leave the
    // selected tab bare for one standard beat (review #1220).
    var wasLifted by remember { mutableStateOf(lifted) }
    val justSettled = wasLifted && !lifted
    SideEffect { wasLifted = lifted }
    val pillFill by animateColorAsState(
        targetValue = if (active && !lifted) GlassTokens.RowFillActive else Color.Transparent,
        animationSpec = if (justSettled) snap() else Motion.standard(),
        label = "nav-tab-pill",
    )
    val lit = if (lifted) candidate else active
    val glyphAlpha by animateFloatAsState(
        targetValue = if (lit) 1f else TextEmphasis.Secondary,
        animationSpec = Motion.fast(),
        label = "nav-tab-glyph",
    )
    val tint = Color.White.copy(alpha = glyphAlpha)
    Box(
        modifier = Modifier
            .fillMaxHeight()
            .testTag(tab.testTag)
            .semantics(mergeDescendants = true) {
                role = Role.Tab
                selected = active
                contentDescription = tab.label
                onClick { onSelect(); true }
            },
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .fillMaxHeight()
                .fillMaxWidth()
                .clip(RoundedCornerShape(percent = 50))
                .background(pillFill, RoundedCornerShape(percent = 50))
                .widthIn(min = Chats.Shell.Bar.TAB_MIN_WIDTH.dp)
                .padding(horizontal = Chats.Shell.Bar.TAB_PADDING_X.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            CompositionLocalProvider(LocalContentColor provides tint) { tab.glyph() }
            Spacer(Modifier.height(4.dp))
            Text(
                tab.label,
                style = MaterialTheme.typography.labelSmall.copy(fontSize = 10.sp, lineHeight = 12.sp, letterSpacing = 0.sp),
                color = tint,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

/**
 * The floating bar's own numbers (EXP-698). The bar is the app's most-seen
 * chrome and its geometry used to live as literals inside [BottomNavBar] and
 * be re-typed by the issue detail's twin bar; both read it here now.
 */
object BottomNavDefaults {
    /** Session 58cb32df: the tab capsule's (and the launcher's) height — a
     *  glyph over its label. */
    val BarHeight: Dp = 52.dp

    /** Inset between the capsule's edge and a tab — the pill's track. */
    val BarPadding: Dp = 4.dp

    /** One tab: a square, so its active pill is a circle and not a capsule. */
    val ItemSize: Dp = 44.dp

    /**
     * The gutter between the screen edge and the bar stack. Named because a
     * screen may put a bar of its own in this slot (the issue list's
     * multi-select bar, EXP-698 r5) and it has to land on the same x as the
     * pill it replaces — a hand-typed 16 sat it 4dp off.
     */
    val HorizontalInset: Dp = 20.dp
}
