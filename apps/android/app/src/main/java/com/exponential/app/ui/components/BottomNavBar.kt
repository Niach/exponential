package com.exponential.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.IssueDraftPage
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// Linear-style floating bottom navigation: a dark pill with the top-level
// destinations (Issues, Inbox — the merged Inbox + My Issues personal tab,
// with an unread dot — Devices — the machines surface — and Threads; NEW-7:
// the Actions tab is gone, prompts live in the composer's `/` menu and
// Settings → Prompts; Settings = the Issues gear) plus a detached launcher on the right: the Chat arm that
// switches to the Agent page — the app's landing screen and a top-level
// destination like the tabs, so the arm wears the SELECTED state while it is
// up — joined by New issue in one capsule, on every tab since EXP-973. The
// live dot rides Threads (EXP-1327), like every other client. Search left the
// bar in EXP-686: it is a button in the board header now.
// Overlaid above the NavHost; AppNavHost shows it only on the top-level routes.
// (Compose has no cheap backdrop blur, so the bar takes the shared OPAQUE glass
// fill — GlassTokens.OpaqueCardFill, EXP-698 — instead of the iOS material and
// instead of the hand-mixed near-black it used to carry.)

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

// The Threads dot: amber while a thread needs a person (EXP-214), green while
// one runs (EXP-1327). EXP-699: the live dot shares the semantic green —
// every platform's live dot is the same color now.
private val ThreadsLiveGreen = DesignTokens.Semantic.Green
private val ThreadsNeedYouAmber = DesignTokens.Semantic.Yellow

@Composable
fun BottomNavBar(
    /**
     * Lit while the plain Agent page (the `agent-tab` start destination) is
     * up: the chat arm wears the tab's active fill, and no pill tab is lit.
     */
    agentActive: Boolean,
    issuesActive: Boolean,
    devicesActive: Boolean,
    personalActive: Boolean,
    /** EXP-1291: lit while the Threads tab (it replaced Reviews) is up. */
    threadsActive: Boolean,
    unreadCount: Int,
    /** EXP-1272: the caller's drafts; they light the Inbox dot too (`inboxDot`). */
    draftCount: Int = 0,
    /** EXP-1327: a green dot on Threads while a thread runs. */
    threadsRunning: Boolean,
    /** EXP-1291: an amber dot on Threads while a thread needs a person. */
    threadsNeedYou: Boolean,
    /** EXP-1323: a thread has news the viewer has not read (`threadsUnread`). */
    threadsUnread: Boolean = false,
    /** EXP-973: whether New issue has a board to file onto (it always shows). */
    composeEnabled: Boolean,
    onIssues: () -> Unit,
    onDevices: () -> Unit,
    onPersonal: () -> Unit,
    onThreads: () -> Unit,
    onCompose: () -> Unit,
    onChat: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Four fixed tabs (Issues, Inbox, Devices, Threads).
    val tabCount = 4
    // Every count must still fit a 360dp screen beside the compose circle:
    // the outer padding pulls in with the count. The tab itself is the shared
    // 44dp square on every count (EXP-698).
    Row(
        modifier = modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(
                horizontal = BottomNavDefaults.horizontalInset(tabCount),
                vertical = 8.dp,
            ),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(percent = 50))
                .background(GlassTokens.OpaqueCardFill)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(percent = 50))
                .padding(BottomNavDefaults.BarPadding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TabItem(
                icon = ExpIcons.navMyIssues,
                contentDescription = "Issues",
                testTag = "tab-issues",
                active = issuesActive,
                onClick = onIssues,
            )
            // EXP-58: Inbox + My Issues merged behind this one tab (SLOP-5:
            // called Inbox, like the web and desktop entry).
            TabItem(
                icon = ExpIcons.navInbox,
                contentDescription = "Inbox",
                testTag = "tab-mywork",
                active = personalActive,
                // EXP-1272: unread OR a draft, the same dot and colour.
                showDot = IssueDraftPage.inboxDotLit(unreadCount, draftCount),
                onClick = onPersonal,
            )
            // Devices (EXP-686, the renamed Agents surface): the machine list.
            // Its live dot moved to the Agent launcher with the sessions list.
            TabItem(
                icon = ExpIcons.navDevices,
                contentDescription = "Devices",
                testTag = "tab-devices",
                active = devicesActive,
                onClick = onDevices,
            )
            // EXP-1291: Threads (it replaced Reviews) — the thread list;
            // amber dot while one of them needs a person, green while one
            // runs (EXP-1327).
            TabItem(
                icon = ExpIcons.navThreads,
                contentDescription = "Threads",
                testTag = "tab-threads",
                active = threadsActive,
                // EXP-1323: unread news lights it too; amber, then green, win.
                showDot = threadsRunning || threadsNeedYou || threadsUnread,
                dotColor = when {
                    threadsNeedYou -> ThreadsNeedYouAmber
                    threadsRunning -> ThreadsLiveGreen
                    else -> null
                },
                onClick = onThreads,
            )
        }

        Spacer(Modifier.weight(1f))

        // The detached launcher beside the pill. The Chat circle opens the
        // Agent page from every top-level surface (EXP-631/694/827); no dot
        // since EXP-1327 (the live dot rides Threads). EXP-827: a board offers
        // New issue too, so the slot becomes one capsule with two arms
        // (chat | new issue).
        // EXP-973: the capsule rides EVERY tab now — New issue was a thing
        // the reader had to walk back to the board for. With no board in the
        // team at all its arm dims rather than vanishing, so the launcher
        // never changes shape under the thumb.
        LauncherCapsule(
            onChat = onChat,
            onCompose = onCompose,
            chatSelected = agentActive,
            composeEnabled = composeEnabled,
        )
    }
}

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
            .height(52.dp)
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

@Composable
private fun TabItem(
    icon: ImageVector,
    contentDescription: String,
    // EXP-686: the capture suites address a tab by tag — two of the six now
    // carry a label that also reads as ordinary content elsewhere.
    testTag: String,
    active: Boolean,
    showDot: Boolean = false,
    dotColor: Color? = null,
    onClick: () -> Unit,
) {
    // EXP-523: the pill and the glyph fade between states instead of cutting.
    // A travelling pill (iOS uses matchedGeometryEffect) would need
    // SharedTransitionLayout and per-tab position measurement for a bar whose
    // tab COUNT changes with the yolo flag — a cross-fade reads as
    // deliberate here and costs one animated float per tab. Both collapse to
    // an instant change when the OS has animations off (Motion -> snap()).
    val pillFill by animateColorAsState(
        // EXP-698: the shared ACTIVE fill, not a hand-typed `white.12` that
        // existed only here — the tab pill is the same on-state a segmented
        // segment or a pressed row takes.
        targetValue = if (active) GlassTokens.RowFillActive else Color.Transparent,
        animationSpec = Motion.standard(),
        label = "nav-tab-pill",
    )
    val glyphAlpha by animateFloatAsState(
        targetValue = if (active) 1f else TextEmphasis.Secondary,
        animationSpec = Motion.standard(),
        label = "nav-tab-glyph",
    )
    Box(
        modifier = Modifier
            // One 44dp square on every tab count (EXP-698) — above Material's
            // 48dp touch minimum in neither axis on its own, but the bar's own
            // padding and the row's height carry it there, and a SQUARE tab
            // takes a circular pill instead of a stretched capsule.
            .size(BottomNavDefaults.ItemSize)
            .testTag(testTag)
            .clip(CircleShape)
            .background(pillFill, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(20.dp),
            tint = Color.White.copy(alpha = glyphAlpha),
        )
        if (showDot) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .offset(x = (-14).dp, y = 8.dp)
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(dotColor ?: MaterialTheme.colorScheme.primary),
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
