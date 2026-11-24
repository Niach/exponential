package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NavBadges
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// #EXP-2109 (chats.json `shell.dock`): THE phone's bottom bar. DOCKED, the full
// width, an opaque `popover` fill over the safe area with one hairline on top;
// `shell.tabs` in five equal cells, an icon over its label, muted, the
// selected one in the foreground colour. The Assistant's cell has no label: the
// Exponential mark on a round card-fill disc raised over the bar's top edge,
// in a ring of the page's background; selected = a soft foreground glow.

/** How far the disc stands over the bar's top edge, its ring included. */
private val DiscTop: Dp = (Chats.Shell.Dock.RAISE + Chats.Shell.Dock.RING).dp

/** The bar's height over the system nav inset, the raised disc included. */
val PhoneDockHeight: Dp = Chats.Shell.Dock.HEIGHT.dp + DiscTop

/** The `shell.tabs` glyph of a tab (`tabIcons`), by concept. */
private fun tabIcon(tab: String): ImageVector =
    Chats.Shell.TAB_ICONS[tab]?.let { ExpIcons.byConcept(it) } ?: ExpIcons.navChats

private fun tabTestTag(tab: String): String = when (tab) {
    "chats" -> Chats.TestIds.TAB_CHATS
    "teams" -> Chats.TestIds.TAB_TEAMS
    "assistant" -> Chats.TestIds.TAB_ASSISTANT
    "new" -> Chats.TestIds.TAB_NEW
    else -> Chats.TestIds.TAB_SETTINGS
}

@Composable
fun PhoneDock(
    /** The lit tab (`shell.pages`), null while none is. */
    selected: String?,
    /** The New popup is up: its cell lights and wears `openIcon`. */
    newOpen: Boolean,
    /** THE count badge on Chats, over every team. */
    chatsBadge: NavBadges.Badge,
    onSelect: (tab: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val dock = Chats.Shell.Dock
    Box(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .matchParentSize()
                .padding(top = DiscTop)
                .background(DesignTokens.Palette.Popover)
                .drawBehind {
                    drawLine(GlassTokens.StrokeCard, Offset.Zero, Offset(size.width, 0f), GlassTokens.Hairline.toPx().coerceAtLeast(1f))
                }
                // One surface: a press between the cells never reaches the page.
                .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } },
        )
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(top = DiscTop, start = 6.dp, end = 6.dp)
                .height(dock.HEIGHT.dp),
        ) {
            Chats.Shell.TABS.forEach { tab ->
                if (tab == "assistant") {
                    Spacer(Modifier.weight(1f))
                } else {
                    DockCell(
                        tab = tab,
                        icon = if (tab == "new" && newOpen) ExpIcons.byConcept(Chats.Shell.NewMenu.OPEN_ICON) ?: ExpIcons.uiClose else tabIcon(tab),
                        selected = if (tab == "new") newOpen else selected == tab,
                        badge = if (tab == "chats") chatsBadge else NavBadges.NONE,
                        onClick = { onSelect(tab) },
                    )
                }
            }
        }
        AssistantDisc(
            selected = selected == "assistant",
            onClick = { onSelect("assistant") },
            modifier = Modifier.align(Alignment.TopCenter),
        )
    }
}

@Composable
private fun RowScope.DockCell(
    tab: String,
    icon: ImageVector,
    selected: Boolean,
    badge: NavBadges.Badge,
    onClick: () -> Unit,
) {
    val dock = Chats.Shell.Dock
    val label = Chats.Copy.TABS.getValue(tab)
    val tint = if (selected) MaterialTheme.colorScheme.onSurface else DesignTokens.Palette.MutedForeground
    Column(
        modifier = Modifier
            .weight(1f)
            .fillMaxHeight()
            .testTag(tabTestTag(tab))
            .semantics(mergeDescendants = true) {
                this.selected = selected
                contentDescription = label
            }
            .clickable(role = Role.Tab, interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClick),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box {
            Icon(icon, contentDescription = null, modifier = Modifier.size(dock.ICON.dp), tint = tint)
            CountBadge(
                badge,
                modifier = Modifier.align(Alignment.TopStart).offset(x = (dock.ICON / 2 + 4).dp, y = (-6).dp),
                testTag = "${tabTestTag(tab)}-badge",
            )
        }
        Spacer(Modifier.height(dock.GAP.dp))
        Text(
            label,
            style = MaterialTheme.typography.labelSmall.copy(
                fontSize = dock.LABEL.sp,
                lineHeight = (dock.LABEL + 3).sp,
                letterSpacing = 0.sp,
                fontWeight = FontWeight.Medium,
            ),
            color = tint,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun AssistantDisc(selected: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val dock = Chats.Shell.Dock
    val outer = (dock.DISC + dock.RING * 2).dp
    val ground = DesignTokens.Palette.Background
    val foreground = MaterialTheme.colorScheme.onSurface
    Box(
        modifier = modifier
            .size(outer)
            .drawBehind {
                if (selected) {
                    val ring = size.width / 2f
                    val glow = ring + dock.GLOW.dp.toPx()
                    drawCircle(
                        Brush.radialGradient(
                            0f to foreground.copy(alpha = 0.18f),
                            ring / glow to foreground.copy(alpha = 0.18f),
                            1f to Color.Transparent,
                            center = center,
                            radius = glow,
                        ),
                        radius = glow,
                    )
                }
                drawCircle(ground)
            }
            .padding(dock.RING.dp)
            .clip(CircleShape)
            .background(DesignTokens.Palette.Card)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape)
            .testTag(Chats.TestIds.TAB_ASSISTANT)
            .semantics {
                this.selected = selected
                contentDescription = Chats.Copy.TABS.getValue("assistant")
            }
            .clickable(role = Role.Tab, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        ExponentialMark(size = dock.MARK.dp, tint = foreground)
    }
}

/**
 * The New popup (`shell.newMenu`): the glass menu over the New cell, `gap` above
 * the bar, its items in order. [insetBottom] = what stands under the bar (the
 * system nav inset; null = read it).
 */
@Composable
fun PhoneNewMenu(
    /** False while the viewer has no board: Issue is dimmed and inert. */
    issueEnabled: Boolean,
    onPick: (item: String) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    insetBottom: Dp? = null,
) {
    val menu = Chats.Shell.NewMenu
    BoxWithConstraints(
        modifier = modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.6f))
            .pointerInput(Unit) {
                awaitEachGesture {
                    awaitFirstDown(requireUnconsumed = false).consume()
                    onDismiss()
                }
            },
    ) {
        val cell = maxWidth / Chats.Shell.TABS.size
        val centre = cell * Chats.Shell.TABS.indexOf("new") + cell / 2
        val left = (centre - menu.WIDTH.dp / 2).coerceIn(8.dp, (maxWidth - menu.WIDTH.dp - 8.dp).coerceAtLeast(8.dp))
        val shape = RoundedCornerShape(menu.RADIUS.dp)
        Column(
            modifier = Modifier
                .align(Alignment.BottomStart)
                .then(if (insetBottom == null) Modifier.navigationBarsPadding() else Modifier.padding(bottom = insetBottom))
                .padding(bottom = (Chats.Shell.Dock.HEIGHT + menu.GAP).dp)
                .offset(x = left)
                .width(menu.WIDTH.dp)
                .clip(shape)
                .background(GlassMenuDefaults.ContainerColor)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                .padding(6.dp)
                .testTag(Chats.TestIds.NEW_MENU),
        ) {
            menu.ITEMS.forEach { item ->
                val enabled = item != "issue" || issueEnabled
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(menu.ROW_HEIGHT.dp)
                        .alpha(if (enabled) 1f else TextEmphasis.Tertiary)
                        .testTag("${Chats.TestIds.NEW_MENU_ROW}-$item")
                        .clickable(enabled = enabled) { onPick(item) }
                        .padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(14.dp),
                ) {
                    Icon(
                        ExpIcons.byConcept(menu.ICONS.getValue(item)) ?: ExpIcons.uiAdd,
                        contentDescription = null,
                        modifier = Modifier.size(20.dp),
                        tint = GlassMenuDefaults.TextColor,
                    )
                    Text(
                        Chats.Copy.NEW_MENU.getValue(item),
                        style = MaterialTheme.typography.bodyLarge.copy(fontSize = 15.sp, letterSpacing = 0.sp),
                        color = GlassMenuDefaults.TextColor,
                    )
                }
            }
        }
    }
}

/**
 * A page the docked bar stands on keeps its content above it: [extra] on top
 * of the bar's height, beside the screen's own nav-bar inset, and nothing
 * while the keyboard is up (the bar hides).
 */
fun Modifier.aboveDock(extra: Dp = 0.dp): Modifier = composed {
    val ime = WindowInsets.ime
    val nav = WindowInsets.navigationBars
    layout { measurable, constraints ->
        val keyboard = (ime.getBottom(this) - nav.getBottom(this)).coerceAtLeast(0)
        val inset = ((Chats.Shell.Dock.HEIGHT.dp + extra).roundToPx() - keyboard).coerceAtLeast(0)
        val height = (constraints.maxHeight - inset).coerceAtLeast(0)
        val placeable = measurable.measure(constraints.copy(minHeight = minOf(constraints.minHeight, height), maxHeight = height))
        layout(placeable.width, placeable.height + inset) { placeable.place(0, 0) }
    }
}
