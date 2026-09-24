package com.exponential.app.ui.components

import androidx.activity.compose.BackHandler
import androidx.activity.compose.LocalOnBackPressedDispatcherOwner
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1029 contract, implemented by EXP-1043 — sub-shell navigation for the
 * settings shell (`OptionGroup` rows, EXP-994). Used for "Workflow settings"
 * inside the device settings sheet.
 *
 * A [SubShell] is a ROW ENTRY inside a card. Opening it slides a child page
 * in place of the WHOLE card — not a nested card, not a pushed screen —
 * with a back button on top; the child page is the same shell (its own
 * `OptionGroup`s of rows), so a sub-shell may hold another sub-shell.
 * [SubShellHost] is the card boundary the page replaces: it renders its
 * card at rest and, once a row inside opened, that row's page.
 *
 * How it works: the host owns ONE page stack ([SubShellNavigation]) and
 * draws every level — the card, then each open page — into the same box.
 * Only the TOP level is measured; the ones underneath stay COMPOSED and
 * simply take no room, which is what keeps an open page live: the row that
 * opened it is still composing, so its `page` lambda keeps receiving the
 * surface's current state instead of a snapshot frozen at tap time. A
 * deeper page hides its parent's rows AND its parent's back header, and
 * back (the header button or the system gesture) returns ONE level.
 *
 * Siblings (same names): web `packages/ui/src/sub-shell.tsx`, IDE
 * `ui::sub_shell`, iOS `ExpUI/Sources/SubShell.swift`.
 */

/** One page on a host's stack: which row opened it, its title, its body. */
class SubShellPageEntry(
    val id: Any,
    val title: String,
    val content: (@Composable () -> Unit)?,
)

/**
 * A host's page stack. Plain state, no composition of its own, so the
 * navigation rules (open, return ONE level, a disabled row never opens) are
 * testable without a compose rule — `SubShellContractTest` drives THIS.
 */
class SubShellNavigation {

    private var stack by mutableStateOf<List<SubShellPageEntry>>(emptyList())

    /** The open pages, outermost first; empty = the card at rest. */
    val pages: List<SubShellPageEntry> get() = stack

    /** Whether a page is open right now. */
    val isOpen: Boolean get() = stack.isNotEmpty()

    /** The open page's title, null at rest. */
    val current: String? get() = stack.lastOrNull()?.title

    val depth: Int get() = stack.size

    /**
     * Which way the last move went: a deeper page slides in from the end
     * edge, a page returned to from the start edge.
     */
    var forward: Boolean by mutableStateOf(true)
        private set

    /**
     * Slide [title]'s page in place of the card. [enabled] false is the
     * DISABLED row: it never opens, and the rule lives here rather than in
     * the click modifier alone so it holds however the row is reached.
     */
    fun push(
        title: String,
        id: Any = Any(),
        enabled: Boolean = true,
        content: (@Composable () -> Unit)? = null,
    ) {
        if (!enabled) return
        forward = true
        stack = stack + SubShellPageEntry(id, title, content)
    }

    /** Return ONE level: to the page underneath, or to the card. */
    fun back() {
        if (stack.isEmpty()) return
        forward = false
        stack = stack.dropLast(1)
    }
}

/** The stack a [SubShell] row pushes onto; null outside a [SubShellHost]. */
internal val LocalSubShellNavigation = staticCompositionLocalOf<SubShellNavigation?> { null }

/**
 * The card boundary a sub-shell page replaces: the card at rest, the open
 * row's page instead of it.
 */
@Composable
fun SubShellHost(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    val nav = remember { SubShellNavigation() }
    val reduceMotion = LocalReduceMotion.current
    // Predictive back returns one level while a page is open. Guarded: a
    // host inside a window that carries no dispatcher keeps its header
    // button rather than crashing the surface it sits in.
    if (LocalOnBackPressedDispatcherOwner.current != null) {
        BackHandler(enabled = nav.isOpen) { nav.back() }
    }
    CompositionLocalProvider(LocalSubShellNavigation provides nav) {
        Box(
            modifier = modifier
                .fillMaxWidth()
                // A level slides in from outside the host's own bounds.
                .clipToBounds()
                .testTag("sub-shell-host"),
        ) {
            val pages = nav.pages
            SubShellLevel(
                visible = pages.isEmpty(),
                forward = nav.forward,
                reduceMotion = reduceMotion,
                // The card is simply THERE when the surface opens; only a
                // page slides.
                animateOnEnter = false,
                content = content,
            )
            pages.forEachIndexed { index, page ->
                key(page.id) {
                    SubShellLevel(
                        visible = index == pages.lastIndex,
                        forward = nav.forward,
                        reduceMotion = reduceMotion,
                        animateOnEnter = true,
                        tag = "sub-shell-page",
                    ) {
                        Column(modifier = Modifier.fillMaxWidth()) {
                            SubShellBackHeader(title = page.title, onBack = nav::back)
                            page.content?.invoke()
                        }
                    }
                }
            }
        }
    }
}

/**
 * A row entry that slides its child page in place of the whole card.
 */
@Composable
fun SubShell(
    label: String,
    modifier: Modifier = Modifier,
    /** A muted second line under the label. */
    description: String? = null,
    /** A leading `ExpIcons` glyph. */
    icon: ImageVector? = null,
    /** A muted trailing summary (`Opus · Fable`). */
    value: String? = null,
    /** The child page's title; defaults to [label]. */
    title: String? = null,
    enabled: Boolean = true,
    /** The child page: the same shell — `OptionGroup`s of rows. */
    page: @Composable () -> Unit,
) {
    val nav = LocalSubShellNavigation.current
    // The row's identity on the stack: its own slot, so reopening the same
    // row reuses its page and two rows never collide.
    val id = remember { Any() }
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("sub-shell")
            .clickable(enabled = enabled && nav != null) {
                // The `page` lambda is the compose compiler's memoised one:
                // handing it over once is enough, because every
                // recomposition of this row updates its captures in place
                // (and this row keeps composing behind the open page).
                nav?.push(title = title ?: label, id = id, enabled = enabled, content = page)
            }
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .alpha(if (enabled) 1f else 0.5f),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(imageVector = icon, contentDescription = null, tint = muted)
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(label, color = LocalContentColor.current)
            if (description != null) {
                Text(description, style = MaterialTheme.typography.bodySmall, color = muted)
            }
        }
        if (value != null) {
            Text(value, color = muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Icon(imageVector = ExpIcons.uiChevronRight, contentDescription = null, tint = muted)
    }
}

/** The page's own header: the way back, then what this page is. */
@Composable
private fun SubShellBackHeader(title: String, onBack: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 4.dp, end = 16.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack, modifier = Modifier.testTag("sub-shell-back")) {
            Icon(
                ExpIcons.uiChevronLeft,
                contentDescription = "Back",
                modifier = Modifier.size(20.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Text(
            title,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * ONE level of the shell — the card, or a page. The top one is drawn; the
 * ones below stay composed and take no room (see [SubShellHost]).
 */
@Composable
private fun SubShellLevel(
    visible: Boolean,
    forward: Boolean,
    reduceMotion: Boolean,
    animateOnEnter: Boolean,
    tag: String? = null,
    content: @Composable () -> Unit,
) {
    // The slide, as a fraction of this level's own width: +1 = just off the
    // end edge (going deeper), -1 = off the start edge (coming back).
    val offset = remember { Animatable(0f) }
    var composed by remember { mutableStateOf(false) }
    LaunchedEffect(visible) {
        val first = !composed
        composed = true
        if (!visible) return@LaunchedEffect
        if (first && !animateOnEnter) return@LaunchedEffect
        offset.snapTo(if (forward) 1f else -1f)
        offset.animateTo(0f, Motion.standard(reduceMotion))
    }
    val levelModifier = Modifier
        .fillMaxWidth()
        .graphicsLayer { translationX = offset.value * size.width }
        .let { if (tag != null) it.testTag(tag) else it }
    KeptAlive(visible = visible, modifier = levelModifier, content = content)
}

/**
 * Composes [content] always, and measures it only while [visible]: a level
 * underneath the open page keeps its rows alive (their `page` lambdas stay
 * current) while taking neither room nor a place in the a11y tree.
 */
@Composable
private fun KeptAlive(
    visible: Boolean,
    modifier: Modifier,
    content: @Composable () -> Unit,
) {
    Layout(
        content = content,
        modifier = if (visible) modifier else modifier.clearAndSetSemantics {},
    ) { measurables, constraints ->
        if (!visible) return@Layout layout(0, 0) {}
        val placeables = measurables.map { it.measure(constraints) }
        val width = placeables.maxOfOrNull(Placeable::width) ?: 0
        val height = placeables.fold(0) { total, placeable -> total + placeable.height }
        layout(width, height) {
            var y = 0
            placeables.forEach { placeable ->
                placeable.place(0, y)
                y += placeable.height
            }
        }
    }
}
