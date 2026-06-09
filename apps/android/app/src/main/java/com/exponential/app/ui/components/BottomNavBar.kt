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
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.GridView
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// Linear-style floating bottom navigation: a dark pill with the top-level
// destinations (Projects, Inbox — with an unread dot) plus a detached circular
// compose button on the right. Overlaid above the NavHost; AppNavHost shows it
// only on the top-level routes. (Compose has no cheap backdrop blur, so the
// pill uses a near-opaque dark fill instead of the iOS material.)
private val PillFill = Color(0xF2151518)

@Composable
fun BottomNavBar(
    homeActive: Boolean,
    inboxActive: Boolean,
    unreadCount: Int,
    showCompose: Boolean,
    onHome: () -> Unit,
    onInbox: () -> Unit,
    onCompose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = 20.dp, vertical = 8.dp),
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
                icon = Icons.Filled.GridView,
                contentDescription = "Projects",
                active = homeActive,
                onClick = onHome,
            )
            TabItem(
                icon = Icons.Filled.Inbox,
                contentDescription = "Inbox",
                active = inboxActive,
                showDot = unreadCount > 0,
                onClick = onInbox,
            )
        }

        Spacer(Modifier.weight(1f))

        if (showCompose) {
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
    showDot: Boolean = false,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .width(56.dp)
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
                    .background(MaterialTheme.colorScheme.primary),
            )
        }
    }
}
