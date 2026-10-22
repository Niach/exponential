package com.exponential.app.ui.components

import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import com.exponential.app.domain.Dock
import com.exponential.app.domain.ToastStack
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// N-12: THE Dock ×Android (fixtures/dock.json ×4; web packages/ui dock.tsx,
// desktop dock.rs, iOS Dock.swift): the DockCards (a waiting plan or
// question, THE Queue, the queued messages, a status banner) piled like the
// toasts ([ToastStack.stackLayout] with [Dock.STACK], anchored at the
// bottom: the front card last, the older ones peeking out above it, `visible`
// at most; a tap fans them `gap` apart, the next tap opens a card) over THE
// composer, which never leaves. The host draws the pile inside its floating
// band (the band's measured height pads the transcript and lifts the jump
// button). A tap on a card opens its sheet ([DockSheet]: a pinned header,
// ONE scrolling body ([DockSheetBody]), a pinned footer ([DockSheetFooter]),
// at most `sheet.maxHeightFraction` of the screen).

/** The tint of a card's glyph and label ×4 (web `TINT_CLASS`). */
@Composable
internal fun dockTint(tint: Dock.Tint): Color = when (tint) {
    Dock.Tint.Plan -> MaterialTheme.colorScheme.primary
    Dock.Tint.Question -> DesignTokens.Semantic.Yellow
    Dock.Tint.Muted -> MaterialTheme.colorScheme.onSurfaceVariant
    // Web `text-amber-400`.
    Dock.Tint.Warning -> DockWarning
}

private val DockWarning = Color(0xFFFBBF24)

/** A card's concept icon (dock.json `icons` + the hosts' banner concepts). */
internal fun dockIcon(concept: String): ImageVector = when (concept) {
    Dock.Icons.PLAN -> ExpIcons.codingPlan
    Dock.Icons.QUESTION -> ExpIcons.uiHelp
    Dock.Icons.TASKS -> ExpIcons.traceTodo
    Dock.Icons.QUEUED -> ExpIcons.uiQueued
    "ui-usage" -> ExpIcons.uiUsage
    "coding-compact" -> ExpIcons.codingCompact
    "ui-device-offline" -> ExpIcons.uiDeviceOffline
    "ui-refresh" -> ExpIcons.uiRefresh
    "ui-loading" -> ExpIcons.uiLoading
    else -> ExpIcons.uiInfo
}

private val Muted: Color
    @Composable get() = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

/**
 * The cards of THE Dock: the pile (or the single card) and, while [openId]
 * names an expandable card, its sheet. [sheet] draws an open card's content
 * (a [DockSheetBody] then an optional [DockSheetFooter]); [action] a banner's
 * button. Draws nothing without rows.
 */
@Composable
fun DockCards(
    rows: List<Dock.Row>,
    openId: String?,
    onOpenChange: (String?) -> Unit,
    modifier: Modifier = Modifier,
    action: (@Composable (Dock.Row) -> Unit)? = null,
    initialExpanded: Boolean = false,
    sheet: @Composable ColumnScope.(row: Dock.Row, scope: DockSheetScope, close: () -> Unit) -> Unit,
) {
    var expanded by remember { mutableStateOf(initialExpanded) }
    val view = Dock.dock(rows, expanded, openId)
    if (view.mode != Dock.Mode.None) {
        DockPile(
            rows = view.rows,
            expanded = view.mode == Dock.Mode.Fanned,
            single = view.mode == Dock.Mode.Single,
            onExpandedChange = { expanded = it },
            onOpen = { row ->
                if (row.expandable) {
                    expanded = false
                    onOpenChange(row.id)
                }
            },
            action = action,
            modifier = modifier,
        )
    }
    val open = view.openId?.let { id -> rows.firstOrNull { it.id == id } }
    if (open != null) {
        val close = { onOpenChange(null) }
        DockSheet(row = open, onDismiss = close) { scope -> sheet(open, scope, close) }
    }
}

/**
 * The pile: every card placed by [ToastStack.stackLayout] (measured heights),
 * the front one last; collapsed = the older cards peek out above it, their
 * words hidden; a tap on it fans it (the next tap on a card opens the card).
 * Motion `pile` (slow, standard easing); reduce motion = no movement.
 */
@Composable
fun DockPile(
    rows: List<Dock.Row>,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    onOpen: (Dock.Row) -> Unit,
    modifier: Modifier = Modifier,
    single: Boolean = false,
    action: (@Composable (Dock.Row) -> Unit)? = null,
) {
    val density = LocalDensity.current
    val reduce = LocalReduceMotion.current
    val heights = remember { mutableStateMapOf<String, Int>() }
    val fanned = expanded || single
    val layout = ToastStack.stackLayout(
        rows.map { row -> with(density) { (heights[row.id] ?: 0).toDp().value.toDouble() } },
        expanded = fanned,
        anchoredBottom = true,
        constants = Dock.STACK,
    )
    val spec = Motion.slow<Dp>(reduce)
    val boxHeight by animateDpAsState(layout.height.dp, spec, label = "dock-pile-height")
    val frontId = rows.lastOrNull()?.id
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(boxHeight)
            .testTag(Dock.TestIds.PILE),
    ) {
        rows.forEachIndexed { index, row ->
            val item = layout.items[index]
            val front = row.id == frontId
            val offset by animateDpAsState(item.offset.dp, spec, label = "dock-card-offset")
            val floatSpec = Motion.slow<Float>(reduce)
            val scale by animateFloatAsState(item.scale.toFloat(), floatSpec, label = "dock-card-scale")
            val alpha by animateFloatAsState(if (item.visible) 1f else 0f, floatSpec, label = "dock-card-alpha")
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .offset(y = offset)
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        this.alpha = alpha
                        transformOrigin = TransformOrigin(0.5f, 0f)
                    }
                    .onSizeChanged { size -> if (heights[row.id] != size.height) heights[row.id] = size.height },
            ) {
                DockCard(
                    row = row,
                    hidden = !fanned && !front,
                    action = action?.let { draw -> { draw(row) } },
                    onOpen = {
                        // A touch fans the pile first; the next touch opens.
                        if (!fanned) onExpandedChange(true) else onOpen(row)
                    },
                )
            }
        }
    }
}

/**
 * A DockCard collapsed: the AskCard chrome (the kind glyph, the tinted label,
 * the muted caption, the trailing meta) over the ONE title line and the open
 * chevron (an expandable card) or the banner's [action]. [hidden] = a card
 * behind the front one while piled (its chrome peeks, its words do not).
 */
@Composable
fun DockCard(
    row: Dock.Row,
    modifier: Modifier = Modifier,
    onOpen: (() -> Unit)? = null,
    action: (@Composable () -> Unit)? = null,
    hidden: Boolean = false,
) {
    val tint = dockTint(row.tint)
    val shape = RoundedCornerShape(GlassTokens.CardRadius)
    // A piled card behind the front one still takes the tap that fans.
    val pressable = onOpen != null && (row.expandable || hidden)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(GlassTokens.OpaqueCardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            .then(if (pressable) Modifier.clickable(role = Role.Button) { onOpen?.invoke() } else Modifier)
            .then(
                if (hidden) {
                    Modifier.clearAndSetSemantics {}
                } else if (pressable) {
                    Modifier.semantics(mergeDescendants = true) { contentDescription = "${row.label}: ${row.title}" }
                } else {
                    Modifier
                },
            )
            .padding(12.dp)
            .testTag(Dock.TestIds.CARD),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        val words = if (hidden) Modifier.alpha(0f) else Modifier
        Icon(
            dockIcon(row.icon),
            contentDescription = null,
            modifier = words.padding(top = 2.dp).size(14.dp),
            tint = tint,
        )
        Column(modifier = words.weight(1f)) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    row.label,
                    style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Medium),
                    color = tint,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                row.caption?.let {
                    Text(
                        it,
                        modifier = Modifier.weight(1f, fill = false),
                        style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
                        color = Muted,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                if (row.meta != null) {
                    Spacer(Modifier.weight(1f))
                    Text(
                        row.meta,
                        style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
                        color = Muted,
                        maxLines = 1,
                    )
                }
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    row.title,
                    modifier = Modifier.weight(1f),
                    style = MaterialTheme.typography.bodyMedium.copy(fontSize = 14.sp),
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.9f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (row.expandable) {
                    Icon(ExpIcons.uiChevronRight, contentDescription = null, modifier = Modifier.size(14.dp), tint = Muted)
                } else if (action != null) {
                    action()
                }
            }
        }
    }
}

/** What an open sheet's content may set: the header's trailing meta (the
 *  plan's "Question 2 of 3"). */
@Stable
class DockSheetScope {
    var meta: String? by mutableStateOf(null)
}

/**
 * THE sheet a DockCard opens into: THE [GlassSheet] (fitted, at most
 * `sheet.maxHeightFraction` of the screen, the grabber, drag to dismiss)
 * wearing the Dock's chrome ([DockSheetChrome]).
 */
@Composable
fun DockSheet(
    row: Dock.Row,
    onDismiss: () -> Unit,
    content: @Composable ColumnScope.(DockSheetScope) -> Unit,
) {
    GlassSheet(title = null, onDismiss = onDismiss, modifier = Modifier.testTag(Dock.TestIds.SHEET)) {
        DockSheetChrome(row = row, onClose = onDismiss, content = content)
    }
}

/** The sheet's face: the pinned header (the card's glyph, label, caption, the
 *  content's meta, the close glyph) over the content. The live sheet and the
 *  styleguide's static frame share it. */
@Composable
fun ColumnScope.DockSheetChrome(
    row: Dock.Row,
    onClose: (() -> Unit)?,
    scope: DockSheetScope = remember(row.id) { DockSheetScope() },
    content: @Composable ColumnScope.(DockSheetScope) -> Unit,
) {
    val tint = dockTint(row.tint)
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 12.dp, top = 4.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(dockIcon(row.icon), contentDescription = null, modifier = Modifier.size(14.dp), tint = tint)
        Text(
            row.label,
            style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Medium),
            color = tint,
            maxLines = 1,
        )
        row.caption?.let {
            Text(
                it,
                modifier = Modifier.weight(1f, fill = false),
                style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
                color = Muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.weight(1f))
        scope.meta?.let {
            Text(
                it,
                modifier = Modifier.testTag("plan-card-step"),
                style = MaterialTheme.typography.labelSmall.copy(fontSize = 12.sp, fontWeight = FontWeight.Normal),
                color = Muted,
                maxLines = 1,
            )
        }
        // The ghost icon-xs close (24dp).
        Box(
            modifier = Modifier
                .size(24.dp)
                .clip(CircleShape)
                .then(if (onClose != null) Modifier.clickable(role = Role.Button, onClick = onClose) else Modifier)
                .semantics { contentDescription = Dock.Copy.CLOSE }
                .testTag(Dock.TestIds.CLOSE),
            contentAlignment = Alignment.Center,
        ) {
            Icon(ExpIcons.uiClose, contentDescription = null, modifier = Modifier.size(14.dp), tint = Muted)
        }
    }
    content(scope)
}

/** The sheet's ONE scrolling region. */
@Composable
fun ColumnScope.DockSheetBody(content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .weight(1f, fill = false)
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(start = 16.dp, end = 16.dp, bottom = 12.dp)
            .testTag(Dock.TestIds.SHEET_BODY),
        content = content,
    )
}

/** The sheet's pinned footer: the primary actions, a hairline above it. */
@Composable
fun DockSheetFooter(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxWidth().testTag(Dock.TestIds.SHEET_FOOTER)) {
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeCard)
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            content = content,
        )
    }
}

/** The styleguide's sheet at rest (no window): the same chrome in a box
 *  wearing the sheet's face, [height] tall. */
@Composable
fun DockSheetSpecimenFrame(
    row: Dock.Row,
    modifier: Modifier = Modifier,
    height: Dp = 360.dp,
    content: @Composable ColumnScope.(DockSheetScope) -> Unit,
) {
    val shape = RoundedCornerShape(topStart = Dock.SHEET_RADIUS.dp, topEnd = Dock.SHEET_RADIUS.dp)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .height(height)
            .clip(shape)
            .background(GlassSheetDefaults.ContainerColor, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
            .padding(top = 12.dp),
    ) {
        DockSheetChrome(row = row, onClose = null, content = content)
    }
}

/**
 * AUTO-OPEN (dock.json): the plan or question that just arrived opens its
 * sheet while the view is in front ([enabled] and the screen resumed); every
 * id seen is remembered so a card never re-opens itself, and what already
 * waited when the view opened stays collapsed.
 */
@Composable
fun DockAutoOpen(rows: List<Dock.Row>, enabled: Boolean, onOpen: (String) -> Unit) {
    val seen = remember { mutableSetOf<String>() }
    var primed by remember { mutableStateOf(false) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val open by rememberUpdatedState(onOpen)
    val key = rows.map { it.id to it.kind }
    LaunchedEffect(key) {
        if (!primed) {
            primed = true
            seen += key.map { it.first }
            return@LaunchedEffect
        }
        val next = Dock.dockAutoOpen(seen, key)
        seen += key.map { it.first }
        if (next != null && enabled && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) open(next)
    }
}
