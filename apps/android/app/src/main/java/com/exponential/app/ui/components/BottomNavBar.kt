package com.exponential.app.ui.components

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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.CallMerge
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.SmartToy
import androidx.compose.material.icons.filled.SupportAgent
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// Linear-style floating bottom navigation: a dark pill with the top-level
// destinations (Issues, My Work — the merged Inbox + My Issues personal tab,
// with an unread dot — Support — the team helpdesk inbox, present only while
// the active team's helpdesk flag is on (EXP-180) — Agents — with a green
// live dot — Reviews — its own entry per EXP-147, ordered after Agents per
// EXP-152 — and Search; base order per EXP-81) plus a detached circular
// compose button on the right.
// Overlaid above the NavHost; AppNavHost shows it only on the top-level routes.
// (Compose has no cheap backdrop blur, so the pill uses a near-opaque dark fill
// instead of the iOS material.)
private val PillFill = Color(0xF2151518)

// Bottom contentPadding for scrollable content on screens the floating bar
// overlays (the bar stack is ~68dp above the system nav inset — 42dp tab +
// 5dp pill padding ×2 + 8dp vertical padding ×2 — plus breathing room). Every
// LazyColumn under the bar must use this so its last row scrolls clear of the
// pill; keep them in sync here instead of hardcoding 96.dp per screen (EXP-36).
val BottomBarInset = 96.dp

// Live-session green, matching the steer UI's "Coding now" badge.
private val AgentsLiveGreen = Color(0xFF34D399)

@Composable
fun BottomNavBar(
    issuesActive: Boolean,
    searchActive: Boolean,
    agentsActive: Boolean,
    personalActive: Boolean,
    reviewsActive: Boolean,
    supportActive: Boolean,
    unreadCount: Int,
    agentsRunning: Boolean,
    showsSupport: Boolean,
    showsCompose: Boolean,
    onIssues: () -> Unit,
    onSearch: () -> Unit,
    onAgents: () -> Unit,
    onPersonal: () -> Unit,
    onReviews: () -> Unit,
    onSupport: () -> Unit,
    onCompose: () -> Unit,
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
                .background(PillFill)
                .border(GlassTokens.Hairline, Color.White.copy(alpha = 0.12f), RoundedCornerShape(percent = 50))
                .padding(5.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TabItem(
                icon = Icons.AutoMirrored.Filled.List,
                contentDescription = "Issues",
                active = issuesActive,
                width = tabWidth,
                onClick = onIssues,
            )
            TabItem(
                icon = Icons.Filled.Inbox,
                contentDescription = "My Work",
                active = personalActive,
                width = tabWidth,
                showDot = unreadCount > 0,
                onClick = onPersonal,
            )
            // Support (EXP-180): the team helpdesk inbox — the same headset
            // glyph its rows use. Present only while the active team's synced
            // helpdesk flag is on.
            if (showsSupport) {
                TabItem(
                    icon = Icons.Filled.SupportAgent,
                    contentDescription = "Support",
                    active = supportActive,
                    width = tabWidth,
                    onClick = onSupport,
                )
            }
            TabItem(
                icon = Icons.Filled.SmartToy,
                contentDescription = "Agents",
                active = agentsActive,
                width = tabWidth,
                showDot = agentsRunning,
                dotColor = AgentsLiveGreen,
                onClick = onAgents,
            )
            // Reviews sits beside Agents (EXP-147/EXP-152) — the same open-PR
            // glyph the Reviews rows use.
            TabItem(
                icon = Icons.AutoMirrored.Filled.CallMerge,
                contentDescription = "Reviews",
                active = reviewsActive,
                width = tabWidth,
                onClick = onReviews,
            )
            TabItem(
                icon = Icons.Filled.Search,
                contentDescription = "Search",
                active = searchActive,
                width = tabWidth,
                onClick = onSearch,
            )
        }

        Spacer(Modifier.weight(1f))

        if (showsCompose) {
            Box(
                modifier = Modifier
                    .size(52.dp)
                    .clip(CircleShape)
                    .background(PillFill)
                    .border(GlassTokens.Hairline, Color.White.copy(alpha = 0.12f), CircleShape)
                    .clickable(onClick = onCompose),
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    Icons.Filled.Edit,
                    contentDescription = "New issue",
                    modifier = Modifier.size(20.dp),
                    tint = Color.White,
                )
            }
        }
    }
}

@Composable
private fun TabItem(
    icon: ImageVector,
    contentDescription: String,
    active: Boolean,
    width: Dp,
    showDot: Boolean = false,
    dotColor: Color? = null,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            // 48dp (Material minimum touch width) instead of the old 56dp —
            // 44dp while all six tabs show (Support present): the row + the
            // compose circle must fit a 360dp screen.
            .width(width)
            .height(42.dp)
            .clip(RoundedCornerShape(percent = 50))
            .background(if (active) Color.White.copy(alpha = 0.12f) else Color.Transparent)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = contentDescription,
            modifier = Modifier.size(20.dp),
            tint = Color.White.copy(alpha = if (active) 1f else TextEmphasis.Secondary),
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
