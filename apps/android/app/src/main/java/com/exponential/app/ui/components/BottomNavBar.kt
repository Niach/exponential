package com.exponential.app.ui.components

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.material3.Text
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.Chats
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
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis

// Thread 58cb32df (chats.json `shell`): the phone's bottom bar = the glass tab
// capsule — the look-alike of iOS's Liquid Glass tab bar on the FAB chrome
// recipe (the shared OPAQUE glass fill + the strong hairline, EXP-698) — with
// `shell.tabs` in order, Board · Devices · Chats · Team, each an icon over its
// label (`copy.tabs`), the selected one on the glass-active pill; the Team
// tab's icon is the active team's avatar and its label the team name; a dot
// on Chats (amber while a chat needs you, green while one runs, else the
// unread dot). Beside it, unchanged, the two-armed new chat / new issue
// capsule (the Chat arm switches to the Agent page, the app's landing root).
// Inbox left the bar: it is a page the Chats header's inbox circle pushes.
// Overlaid above the NavHost; AppNavHost shows it only on the top-level routes.

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


@Composable
fun BottomNavBar(
    /**
     * Lit while the plain Agent page (the `agent-tab` start destination) is
     * up: the chat arm wears the tab's active fill, and no pill tab is lit.
     */
    agentActive: Boolean,
    boardActive: Boolean,
    devicesActive: Boolean,
    chatsActive: Boolean,
    teamActive: Boolean,
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
    onBoard: () -> Unit,
    onDevices: () -> Unit,
    onChats: () -> Unit,
    onTeam: () -> Unit,
    onCompose: () -> Unit,
    onChat: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = BottomNavDefaults.HorizontalInsetCompact, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .weight(1f)
                .height(BottomNavDefaults.BarHeight)
                .clip(RoundedCornerShape(percent = 50))
                .background(GlassTokens.OpaqueCardFill)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(percent = 50))
                .padding(BottomNavDefaults.BarPadding),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Chats.Shell.TABS.forEach { tab ->
                when (tab) {
                    "board" -> TabItem(
                        label = Chats.Copy.TABS.getValue(tab),
                        testTag = "tab-issues",
                        active = boardActive,
                        onClick = onBoard,
                        glyph = { tint -> Icon(ExpIcons.navIssues, null, modifier = Modifier.size(22.dp), tint = tint) },
                    )
                    "devices" -> TabItem(
                        label = Chats.Copy.TABS.getValue(tab),
                        testTag = "tab-devices",
                        active = devicesActive,
                        onClick = onDevices,
                        glyph = { tint -> Icon(ExpIcons.navDevices, null, modifier = Modifier.size(22.dp), tint = tint) },
                    )
                    "chats" -> TabItem(
                        label = Chats.Copy.TABS.getValue(tab),
                        testTag = "tab-threads",
                        active = chatsActive,
                        // Amber, then green, then the unread dot (EXP-1323).
                        showDot = chatsRunning || chatsNeedYou || chatsUnread,
                        dotColor = when {
                            chatsNeedYou -> ChatsNeedYouAmber
                            chatsRunning -> ChatsLiveGreen
                            else -> null
                        },
                        onClick = onChats,
                        glyph = { tint -> Icon(ExpIcons.navThreads, null, modifier = Modifier.size(22.dp), tint = tint) },
                    )
                    "team" -> TabItem(
                        label = team?.name?.takeIf { it.isNotBlank() } ?: Chats.Copy.TABS.getValue(tab),
                        testTag = Chats.TestIds.TAB_TEAM,
                        active = teamActive,
                        onClick = onTeam,
                        glyph = { _ -> TeamAvatar(team, size = 22.dp) },
                    )
                }
            }
        }

        Spacer(Modifier.width(10.dp))

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

/** One tab of the capsule: its glyph over its label, the selected one on
 *  the glass-active pill (cross-faded, EXP-523), a dot on its glyph. */
@Composable
private fun RowScope.TabItem(
    label: String,
    testTag: String,
    active: Boolean,
    onClick: () -> Unit,
    glyph: @Composable (tint: Color) -> Unit,
    showDot: Boolean = false,
    dotColor: Color? = null,
) {
    val pillFill by animateColorAsState(
        targetValue = if (active) GlassTokens.RowFillActive else Color.Transparent,
        animationSpec = Motion.standard(),
        label = "nav-tab-pill",
    )
    val glyphAlpha by animateFloatAsState(
        targetValue = if (active) 1f else TextEmphasis.Secondary,
        animationSpec = Motion.standard(),
        label = "nav-tab-glyph",
    )
    Column(
        modifier = Modifier
            .weight(1f)
            .fillMaxHeight()
            .testTag(testTag)
            .clip(RoundedCornerShape(percent = 50))
            .background(pillFill, RoundedCornerShape(percent = 50))
            .clickable(onClick = onClick)
            .semantics { contentDescription = label },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Box {
            glyph(Color.White.copy(alpha = glyphAlpha))
            if (showDot) {
                Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .offset(x = 4.dp, y = (-2).dp)
                        .size(8.dp)
                        .clip(CircleShape)
                        .background(dotColor ?: MaterialTheme.colorScheme.primary),
                )
            }
        }
        Spacer(Modifier.height(2.dp))
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = Color.White.copy(alpha = glyphAlpha),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = 4.dp),
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
