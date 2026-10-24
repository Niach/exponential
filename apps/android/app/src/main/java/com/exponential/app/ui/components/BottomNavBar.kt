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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.unit.lerp
import androidx.compose.ui.unit.sp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.motionDurationMs
import com.exponential.app.ui.theme.motionEasing
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

// Thread 58cb32df → #EXP-1379/#EXP-1384 (chats.json `shell`): the phone's
// bottom STRIP = the glass tab capsule — the look-alike of iOS's Liquid Glass
// tab bar on the FAB chrome recipe (the shared OPAQUE glass fill + the strong
// hairline, EXP-698) — with `shell.tabs` in order, Board · Chats · Apps ·
// Team, each an icon over its label, the selected one on the glass-active
// pill; beside it, unchanged, the two-armed new chat / new issue capsule.
// `shell.bar`: the capsule TAKES THE WIDTH beside the launcher; each tab
// starts at its label's width and the tabs share the rest evenly (a label is
// cut only when the labels alone do not fit, [Chats.Shell.Bar.tabWidths]).
// The Board tab = the active board's glyph + name (`tabIcons.board` + "Board"
// while none); on the board view it carries the caret and its tap opens the
// BOARD MENU: the whole strip MORPHS into one glass panel (`shell.boardMenu`).
// DRAG (`shell.bar.drag`): a press lifts the selection LENS under the finger,
// a drag moves it along the bar, the lift selects the tab under it (a lift
// far above / below cancels). The Team tab = the active team's avatar + name;
// a dot on Chats. Overlaid above the NavHost; AppNavHost shows it only on the
// top-level routes.

// Bottom contentPadding for scrollable content on screens the floating bar
// overlays (the bar stack is ~68dp above the system nav inset — 42dp tab +
// 5dp pill padding ×2 + 8dp vertical padding ×2 — plus breathing room). Every
// LazyColumn under the bar must use this so its last row scrolls clear of the
// pill; keep them in sync here instead of hardcoding 96.dp per screen (EXP-36).
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

/** The board menu's content (`shell.boardMenu`): the active team's boards in
 *  sidebar order, the active one, and New board for a viewer who may create
 *  boards (null = no row). */
data class PhoneBoardMenu(
    val boards: List<BoardEntity>,
    val activeBoardId: String?,
    val onSelect: (BoardEntity) -> Unit,
    val onNewBoard: (() -> Unit)?,
)

/** One tab of the capsule, as the bar draws it. */
private class PhoneTab(
    val key: String,
    val label: String,
    val testTag: String,
    val glyph: @Composable () -> Unit,
    val caret: Boolean = false,
    val dotColor: Color? = null,
)

@Composable
fun BottomNavBar(
    /** The lit tab (`shell.tabs` key), null while none is (the Agent page). */
    selected: String?,
    /**
     * Lit while the plain Agent page (the `agent-tab` start destination) is
     * up: the chat arm wears the tab's active fill, and no pill tab is lit.
     */
    agentActive: Boolean,
    /** The ACTIVE board: the Board tab's glyph + label (null = `tabIcons.board`
     *  + `copy.tabs.board`). */
    board: BoardEntity?,
    /** The board view is up: the Board tab wears the caret and its tap opens
     *  the board menu (the caller's [onSelectTab] decides). */
    onBoardView: Boolean,
    /** The ACTIVE team: the Team tab's avatar + label (null = `copy.tabs.team`). */
    team: TeamEntity?,
    /** EXP-1327: a green dot on Chats while a chat runs. */
    chatsRunning: Boolean,
    /** An amber dot on Chats while a chat needs a person. */
    chatsNeedYou: Boolean,
    /** EXP-1323 / chats.json `chatsUnread`: a chat has unread news. */
    chatsUnread: Boolean = false,
    /** EXP-973: whether New issue has a board to file onto (it always shows). */
    composeEnabled: Boolean,
    /** A tab was tapped or a drag lifted on it (the same thing). */
    onSelectTab: (String) -> Unit,
    onCompose: () -> Unit,
    onChat: () -> Unit,
    modifier: Modifier = Modifier,
    /** The board menu's rows; [menuOpen] morphs the strip into it. */
    boardMenu: PhoneBoardMenu? = null,
    menuOpen: Boolean = false,
    onMenuOpenChange: (Boolean) -> Unit = {},
    /** STATIC drag state for the styleguide specimen (never set by the app):
     *  the lens over this tab, the finger [lensPreviewOffset] off its centre. */
    lensPreview: String? = null,
    lensPreviewOffset: Dp = 0.dp,
) {
    val unreadDot = MaterialTheme.colorScheme.primary
    val tabs = Chats.Shell.TABS.map { key ->
        when (key) {
            "board" -> PhoneTab(
                key = key,
                label = board?.name?.takeIf { it.isNotBlank() } ?: Chats.Copy.TABS.getValue(key),
                testTag = Chats.TestIds.TAB_BOARD,
                glyph = {
                    if (board != null) BoardIcon(board, size = TabGlyphSize)
                    else TabIcon(ExpIcons.navIssues)
                },
                caret = onBoardView && board != null,
            )
            "chats" -> PhoneTab(
                key = key,
                label = Chats.Copy.TABS.getValue(key),
                testTag = "tab-threads",
                glyph = { TabIcon(ExpIcons.navThreads) },
                // Amber, then green, then the unread dot (EXP-1323).
                dotColor = when {
                    chatsNeedYou -> ChatsNeedYouAmber
                    chatsRunning -> ChatsLiveGreen
                    chatsUnread -> unreadDot
                    else -> null
                },
            )
            "apps" -> PhoneTab(
                key = key,
                label = Chats.Copy.TABS.getValue(key),
                testTag = "tab-apps",
                glyph = { TabIcon(ExpIcons.navApps) },
            )
            else -> PhoneTab(
                key = key,
                label = team?.name?.takeIf { it.isNotBlank() } ?: Chats.Copy.TABS.getValue(key),
                testTag = Chats.TestIds.TAB_TEAM,
                glyph = { TeamAvatar(team, size = TabGlyphSize) },
            )
        }
    }

    val reduce = LocalReduceMotion.current
    // The morph (`shell.boardMenu.motion`): 0 = the strip's frame, 1 = the
    // menu's. `mounted` keeps the panel through its collapse.
    val morph = remember { Animatable(if (menuOpen) 1f else 0f) }
    var mounted by remember { mutableStateOf(menuOpen) }
    LaunchedEffect(menuOpen) {
        if (menuOpen) {
            mounted = true
            morph.animateTo(
                1f,
                tween(
                    motionDurationMs(Chats.Shell.BoardMenu.OPEN),
                    easing = motionEasing(Chats.Shell.BoardMenu.OPEN_EASE),
                ),
            )
        } else {
            morph.animateTo(
                0f,
                tween(
                    motionDurationMs(Chats.Shell.BoardMenu.CLOSE),
                    easing = motionEasing(Chats.Shell.BoardMenu.CLOSE_EASE),
                ),
            )
            mounted = false
        }
    }

    Box(
        modifier = modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = BottomNavDefaults.HorizontalInsetCompact, vertical = 8.dp),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .graphicsLayer { alpha = 1f - morph.value },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TabCapsule(
                tabs = tabs,
                selected = selected,
                onSelectTab = onSelectTab,
                lensPreview = lensPreview,
                lensPreviewOffset = lensPreviewOffset,
                modifier = Modifier.weight(1f),
            )

            Spacer(Modifier.width(Chats.Shell.Bar.GAP.dp))

            // The detached launcher beside the pill (EXP-827/973, unchanged): the
            // Chat arm opens the Agent page from every top-level surface, New
            // issue files onto the current board (dimmed with no board).
            LauncherCapsule(
                onChat = onChat,
                onCompose = onCompose,
                chatSelected = agentActive,
                composeEnabled = composeEnabled,
            )
        }
        if (mounted && boardMenu != null) {
            BoardMenuPanel(
                menu = boardMenu,
                progress = { morph.value },
                reduce = reduce,
                onClose = { onMenuOpenChange(false) },
            )
        }
    }
}

private val TabGlyphSize = 20.dp

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
    lensPreview: String?,
    lensPreviewOffset: Dp,
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
    val previewIndex = lensPreview?.let { keys.indexOf(it) }?.takeIf { it >= 0 }
    val showLifted = lifted || previewIndex != null
    val litIndex = if (previewIndex != null) previewIndex else candidate

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
                        lifted = showLifted,
                        candidate = index == litIndex,
                        lens = previewIndex == index,
                        lensOffset = lensPreviewOffset,
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

/** One tab: its glyph over its label (+ the caret), the selected one on the
 *  glass-active pill (cross-faded, EXP-523) unless a press lifted it, a dot
 *  on its glyph. */
@Composable
private fun TabItem(
    tab: PhoneTab,
    active: Boolean,
    lifted: Boolean,
    candidate: Boolean,
    lens: Boolean,
    lensOffset: Dp,
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
        if (lens) {
            // The specimen's static lens (the app's is the capsule's own).
            Box(
                Modifier
                    .matchParentSize()
                    .offset(x = lensOffset)
                    .graphicsLayer {
                        scaleX = Chats.Shell.Bar.Drag.LENS_SCALE
                        scaleY = Chats.Shell.Bar.Drag.LENS_SCALE
                    }
                    .background(GlassTokens.RowFillActive, RoundedCornerShape(percent = 50)),
            )
        }
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
            Box {
                CompositionLocalProvider(LocalContentColor provides tint) { tab.glyph() }
                if (tab.dotColor != null) {
                    Box(
                        modifier = Modifier
                            .align(Alignment.TopEnd)
                            .offset(x = 4.dp, y = (-2).dp)
                            .size(8.dp)
                            .clip(CircleShape)
                            .background(tab.dotColor),
                    )
                }
            }
            Spacer(Modifier.height(4.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    tab.label,
                    style = MaterialTheme.typography.labelSmall.copy(fontSize = 10.sp, lineHeight = 12.sp, letterSpacing = 0.sp),
                    color = tint,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (tab.caret) {
                    Spacer(Modifier.width(2.dp))
                    Icon(
                        ExpIcons.navTeamSwitcher,
                        contentDescription = null,
                        modifier = Modifier.size(Chats.Shell.BoardTab.CARET_SIZE.dp),
                        tint = tint,
                    )
                }
            }
        }
    }
}

/**
 * THE BOARD MENU (`shell.boardMenu`): the strip's frame grown into one glass
 * panel — the same left, right and bottom edges, the height from the strip's
 * to the rows' (up to `maxVisibleRows`, then they scroll), the corner radius
 * from the strip's to `radius` — while the rows fade in. Reduce motion = the
 * panel at its open frame, cross-faded. The surface is the glass menu's
 * ([GlassMenuDefaults]: the opaque fill, the menu text colours).
 */
@Composable
private fun BoardMenuPanel(
    menu: PhoneBoardMenu,
    progress: () -> Float,
    reduce: Boolean,
    onClose: () -> Unit,
) {
    val rowCount = menu.boards.size + if (menu.onNewBoard != null) 1 else 0
    val stripHeight = BottomNavDefaults.BarHeight
    val openHeight = (
        minOf(rowCount, Chats.Shell.BoardMenu.MAX_VISIBLE_ROWS) * Chats.Shell.BoardMenu.ROW_HEIGHT +
            Chats.Shell.Bar.PADDING * 2
        ).dp
    val p = progress()
    val height = if (reduce) openHeight else lerp(stripHeight, openHeight, p)
    val radius = if (reduce) Chats.Shell.BoardMenu.RADIUS.dp else lerp(stripHeight / 2, Chats.Shell.BoardMenu.RADIUS.dp, p)
    val shape = RoundedCornerShape(radius)
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(height)
            .graphicsLayer { alpha = if (reduce) progress() else 1f }
            .clip(shape)
            .background(GlassMenuDefaults.ContainerColor, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            // The panel is one surface: a press on it never reaches the strip
            // under it.
            .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } }
            .testTag(Chats.TestIds.BOARD_MENU),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .requiredHeight(openHeight)
                .align(Alignment.TopCenter)
                .graphicsLayer { alpha = progress() }
                .verticalScroll(rememberScrollState())
                .padding(Chats.Shell.Bar.PADDING.dp),
        ) {
            menu.boards.forEach { board ->
                val active = board.id == menu.activeBoardId
                BoardMenuRow(
                    label = board.name,
                    active = active,
                    muted = false,
                    glyph = { BoardIcon(board, size = TabGlyphSize) },
                    onClick = {
                        menu.onSelect(board)
                        onClose()
                    },
                )
            }
            menu.onNewBoard?.let { onNewBoard ->
                BoardMenuRow(
                    label = Chats.Copy.BOARD_MENU_NEW_BOARD,
                    active = false,
                    muted = true,
                    glyph = {
                        Icon(
                            ExpIcons.uiAdd,
                            contentDescription = null,
                            modifier = Modifier.size(GlassMenuDefaults.IconSize),
                            tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
                        )
                    },
                    onClick = {
                        onClose()
                        onNewBoard()
                    },
                )
            }
        }
    }
}

@Composable
private fun BoardMenuRow(
    label: String,
    active: Boolean,
    muted: Boolean,
    glyph: @Composable () -> Unit,
    onClick: () -> Unit,
) {
    val pill = RoundedCornerShape(percent = 50)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(Chats.Shell.BoardMenu.ROW_HEIGHT.dp)
            .clip(pill)
            .then(if (active) Modifier.background(GlassTokens.RowFillActive, pill) else Modifier)
            .clickable(onClick = onClick)
            .testTag(Chats.TestIds.BOARD_MENU_ROW)
            .padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(Modifier.size(TabGlyphSize), contentAlignment = Alignment.Center) { glyph() }
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge.copy(fontSize = 15.sp),
            color = if (muted) Color.White.copy(alpha = TextEmphasis.Tertiary) else GlassMenuDefaults.TextColor,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (active) {
            Icon(
                ExpIcons.uiCheck,
                contentDescription = "Selected",
                modifier = Modifier.size(GlassMenuDefaults.IconSize),
                tint = GlassMenuDefaults.TextColor,
            )
        }
    }
}

// The Chats dot: amber while a chat needs a person (EXP-214), green while
// one runs (EXP-1327; the shared semantic green).
private val ChatsLiveGreen = DesignTokens.Semantic.Green
private val ChatsNeedYouAmber = DesignTokens.Semantic.Yellow

/**
 * EXP-827: the merged launcher on a board — the circle's 52dp height, two
 * 52dp arms split by a hairline, each its own button with the labels and
 * tags the single circles carry.
 */
@Composable
private fun LauncherCapsule(
    onChat: () -> Unit,
    onCompose: () -> Unit,
    /** The Agent page is the current destination: the arm reads selected. */
    chatSelected: Boolean,
    /** EXP-973: false with no board to file onto — the arm dims and no-ops. */
    composeEnabled: Boolean,
) {
    Row(
        modifier = Modifier
            .height(BottomNavDefaults.BarHeight)
            .clip(RoundedCornerShape(percent = 50))
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(percent = 50)),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LauncherArm(
            icon = ExpIcons.actionChat,
            contentDescription = "Start chat",
            testTag = "chat-button",
            onClick = onChat,
            selected = chatSelected,
        )
        Box(
            Modifier
                .width(GlassTokens.Hairline)
                .height(28.dp)
                .background(GlassTokens.StrokeStrong),
        )
        LauncherArm(
            icon = ExpIcons.navCreateIssue,
            contentDescription = if (composeEnabled) "New issue" else "New issue (no board)",
            testTag = "compose-button",
            onClick = onCompose,
            enabled = composeEnabled,
        )
    }
}

/** One arm of [LauncherCapsule]: a 52dp square hit area, no chrome of its own. */
@Composable
private fun LauncherArm(
    icon: ImageVector,
    contentDescription: String,
    testTag: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
    /**
     * The chat arm's selected state: the tab pill's active fill as a 44dp
     * circle behind the glyph, cross-faded like a tab (EXP-523).
     */
    selected: Boolean = false,
) {
    val selectedFill by animateColorAsState(
        targetValue = if (selected) GlassTokens.RowFillActive else Color.Transparent,
        animationSpec = Motion.standard(),
        label = "launcher-arm-pill",
    )
    Box(
        modifier = Modifier
            .size(52.dp)
            .testTag(testTag)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(BottomNavDefaults.ItemSize)
                .clip(CircleShape)
                .background(selectedFill, CircleShape),
        )
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(20.dp),
            tint = if (enabled) Color.White else Color.White.copy(alpha = TextEmphasis.Quaternary),
        )
    }
}

/**
 * The floating bar's own numbers (EXP-698). The bar is the app's most-seen
 * chrome and its geometry used to live as literals inside [BottomNavBar] and
 * be re-typed by the issue detail's twin bar; both read it here now.
 */
object BottomNavDefaults {
    /** Thread 58cb32df: the tab capsule's (and the launcher's) height — a
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

    /** Six tabs plus the compose circle need the tighter gutter to fit 360dp. */
    val HorizontalInsetCompact: Dp = 12.dp

    /** The gutter for a bar showing [tabCount] tabs (4..6, EXP-1105). */
    fun horizontalInset(tabCount: Int): Dp =
        if (tabCount >= 6) HorizontalInsetCompact else HorizontalInset
}
