package com.exponential.app.ui.components

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

// Linear-style floating bottom navigation: a dark pill with the top-level
// destinations (Issues, My Work — the merged Inbox + My Issues personal tab,
// with an unread dot — Support — the team helpdesk inbox, present only while
// the active team's helpdesk flag is on (EXP-180) — Devices — the machines +
// live sessions surface, with a green live dot — Actions — the team's action
// prompts, its own entry since EXP-686 — and Reviews) plus a detached circular
// compose button on the right. Search left the bar in EXP-686: it is a button
// in the board header now.
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

// EXP-214 dot colors: the Devices dot escalates to amber while a session
// waits on a plan approval / question; the Reviews dot is the review green
// (the in_review issue-status tint). EXP-699: Devices shares the semantic
// green — every platform's live dot is the same color now.
private val AgentsLiveGreen = DesignTokens.Semantic.Green
private val AgentsNeedsInputAmber = DesignTokens.Semantic.Yellow
private val ReviewsGreen = DesignTokens.Semantic.Green

@Composable
fun BottomNavBar(
    issuesActive: Boolean,
    devicesActive: Boolean,
    actionsActive: Boolean,
    personalActive: Boolean,
    reviewsActive: Boolean,
    supportActive: Boolean,
    unreadCount: Int,
    agentsRunning: Boolean,
    agentsNeedInput: Boolean,
    reviewsOpen: Boolean,
    showsSupport: Boolean,
    supportUnread: Boolean,
    showsCompose: Boolean,
    // EXP-631: the Devices surface puts a Chat launcher in the compose slot —
    // composing an issue is board-scoped and hidden there anyway.
    showsChat: Boolean,
    onIssues: () -> Unit,
    onDevices: () -> Unit,
    onActions: () -> Unit,
    onPersonal: () -> Unit,
    onReviews: () -> Unit,
    onSupport: () -> Unit,
    onCompose: () -> Unit,
    onChat: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Six tabs (helpdesk on) must still fit a 360dp screen beside the compose
    // circle: narrow the tabs and pull the outer padding in.
    val tabWidth = if (showsSupport) 44.dp else 48.dp
    Row(
        modifier = modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = if (showsSupport) 12.dp else 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(percent = 50))
                .background(GlassTokens.OpaqueCardFill)
                .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, RoundedCornerShape(percent = 50))
                .padding(5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TabItem(
                icon = ExpIcons.navMyIssues,
                contentDescription = "Issues",
                testTag = "tab-issues",
                active = issuesActive,
                width = tabWidth,
                onClick = onIssues,
            )
            TabItem(
                icon = ExpIcons.navInbox,
                contentDescription = "My Work",
                testTag = "tab-mywork",
                active = personalActive,
                width = tabWidth,
                showDot = unreadCount > 0,
                onClick = onPersonal,
            )
            // Support (EXP-180): the team helpdesk inbox — the same life-buoy
            // glyph its rows use. Present only while the active team's synced
            // helpdesk flag is on.
            if (showsSupport) {
                TabItem(
                    icon = ExpIcons.navSupport,
                    contentDescription = "Support",
                    testTag = "tab-support",
                    active = supportActive,
                    width = tabWidth,
                    showDot = supportUnread,
                    onClick = onSupport,
                )
            }
            // Devices (EXP-686, the renamed Agents surface): the machine list
            // plus its sessions.
            TabItem(
                icon = ExpIcons.navDevices,
                contentDescription = "Devices",
                testTag = "tab-devices",
                active = devicesActive,
                width = tabWidth,
                showDot = agentsRunning,
                // Amber while any session waits on a plan approval / question
                // (EXP-214), live green otherwise.
                dotColor = if (agentsNeedInput) AgentsNeedsInputAmber else AgentsLiveGreen,
                onClick = onDevices,
            )
            // Actions (EXP-686): actions / automations / suggestions, no longer
            // a push off the Devices header.
            TabItem(
                icon = ExpIcons.navActions,
                contentDescription = "Actions",
                testTag = "tab-actions",
                active = actionsActive,
                width = tabWidth,
                onClick = onActions,
            )
            // Reviews sits last (EXP-147/EXP-152/EXP-686) — the same open-PR
            // glyph the Reviews rows use. Green dot while open PRs await
            // review (EXP-214).
            TabItem(
                icon = ExpIcons.navReviews,
                contentDescription = "Reviews",
                testTag = "tab-reviews",
                active = reviewsActive,
                width = tabWidth,
                showDot = reviewsOpen,
                dotColor = ReviewsGreen,
                onClick = onReviews,
            )
        }

        Spacer(Modifier.weight(1f))

        // The detached circular button beside the pill — one slot, whatever
        // the active surface puts in it (compose an issue, start a chat).
        if (showsCompose) {
            Fab(icon = ExpIcons.navCreateIssue, contentDescription = "New issue", onClick = onCompose)
        } else if (showsChat) {
            Fab(icon = ExpIcons.actionChat, contentDescription = "Start chat", onClick = onChat)
        }
    }
}

@Composable
private fun Fab(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(52.dp)
            .clip(CircleShape)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(20.dp),
            tint = Color.White,
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
    width: Dp,
    showDot: Boolean = false,
    dotColor: Color? = null,
    onClick: () -> Unit,
) {
    // EXP-523: the pill and the glyph fade between states instead of cutting.
    // A travelling pill (iOS uses matchedGeometryEffect) would need
    // SharedTransitionLayout and per-tab position measurement for a bar whose
    // tab COUNT changes with the helpdesk flag — a cross-fade reads as
    // deliberate here and costs one animated float per tab. Both collapse to
    // an instant change when the OS has animations off (Motion -> snap()).
    val pillAlpha by animateFloatAsState(
        targetValue = if (active) 0.12f else 0f,
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
            // 48dp (Material minimum touch width) instead of the old 56dp —
            // 44dp while all six tabs show (Support present): the row + the
            // compose circle must fit a 360dp screen.
            .width(width)
            .height(42.dp)
            .testTag(testTag)
            .clip(RoundedCornerShape(percent = 50))
            .background(Color.White.copy(alpha = pillAlpha))
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
