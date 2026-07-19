package com.exponential.app.ui.inbox

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.automirrored.filled.MergeType
import androidx.compose.material.icons.filled.AccountTree
import androidx.compose.material.icons.filled.Adjust
import androidx.compose.material.icons.filled.Feedback
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material.icons.filled.SupportAgent
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// Linear-style single activity stream: one row per issue, showing the latest
// notification's sentence. Notification titles are already full human
// sentences ("Danny merged the pull request for …"), so the second line is
// the title verbatim — no composition, no actor avatar (the rows carry no
// actor column; the leading element is a type-icon badge instead).
//
// Issue-less `support_reply` notifications (EXP-180 helpdesk) render as
// synthetic Support groups above the issue stream — one per team — mirroring
// the web inbox's "Support" group; tapping one opens the Support tab.
//
// Rendered as the Inbox segment of the "My Work" tab (PersonalScreen) since
// EXP-58 — no longer a routed screen of its own; mark-all-read lives in the
// host screen's header.

@Composable
fun InboxListContent(
    onOpenIssue: (String) -> Unit,
    onOpenSupport: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: InboxViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    if (state.groups.isEmpty() && state.supportGroups.isEmpty()) {
        EmptyState(
            message = "You're all caught up.",
            icon = Icons.Filled.Inbox,
            modifier = modifier,
        )
    } else {
        LazyColumn(
            modifier = modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            items(state.supportGroups, key = { "support-${it.teamId ?: "generic"}" }) { group ->
                SupportInboxRow(group) {
                    // Selects the group's team (when known) + marks read; the
                    // callback then lands on the Support tab.
                    viewModel.openSupportGroup(group)
                    onOpenSupport()
                }
            }
            items(state.groups, key = { it.issue.id }) { group ->
                InboxRow(group) {
                    viewModel.markGroupRead(group)
                    onOpenIssue(group.issue.id)
                }
            }
        }
    }
}

@Composable
private fun InboxRow(group: InboxGroup, onClick: () -> Unit) {
    val read = group.unread == 0
    Row(
        Modifier
            .fillMaxWidth()
            .alpha(if (read) 0.6f else 1f)
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // Type-icon badge: a circular muted container with the latest
        // notification's type icon.
        TypeIconBadge(notificationTypeIcon(group.latest.type))
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    group.issue.identifier,
                    fontFamily = FontFamily.Monospace,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    group.issue.title,
                    fontWeight = if (read) FontWeight.Normal else FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            Text(
                group.latest.title,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.width(8.dp))
        TrailingTimeAndDot(time = relativeTime(group.latest.createdAt), unread = group.unread)
    }
}

/**
 * Synthetic Support group row (EXP-180): team name (fallback "Support"),
 * the latest notification's sentence, and its body preview (the reporter's
 * words) when present. Same unread styling as issue rows.
 */
@Composable
private fun SupportInboxRow(group: SupportGroup, onClick: () -> Unit) {
    val read = group.unread == 0
    Row(
        Modifier
            .fillMaxWidth()
            .alpha(if (read) 0.6f else 1f)
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TypeIconBadge(Icons.Filled.SupportAgent)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                group.teamName ?: "Support",
                fontWeight = if (read) FontWeight.Normal else FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                group.latest.title,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val body = group.latest.body
            if (!body.isNullOrBlank()) {
                Text(
                    body,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        Spacer(Modifier.width(8.dp))
        TrailingTimeAndDot(time = relativeTime(group.latest.createdAt), unread = group.unread)
    }
}

/** Circular muted container with a small type icon (shared row leading). */
@Composable
private fun TypeIconBadge(icon: ImageVector) {
    Box(
        Modifier
            .size(28.dp)
            .clip(CircleShape)
            .background(Color.White.copy(alpha = 0.08f)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            icon,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

/** Relative time over the unread dot (shared row trailing). */
@Composable
private fun TrailingTimeAndDot(time: String, unread: Int) {
    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            time,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        if (unread > 0) {
            Box(
                Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(MaterialTheme.colorScheme.primary),
            )
        }
    }
}

// Locked type → icon mapping (closest Material glyphs to the shared table).
private fun notificationTypeIcon(type: String): ImageVector = when (type) {
    DomainContract.notificationTypeIssueAssigned -> Icons.Filled.PersonAdd
    DomainContract.notificationTypeIssueComment,
    DomainContract.notificationTypeIssueMention,
    -> Icons.AutoMirrored.Filled.Chat
    DomainContract.notificationTypeIssueStatusChanged -> Icons.Filled.Adjust
    DomainContract.notificationTypeIssueCreated -> Icons.Filled.Feedback
    DomainContract.notificationTypePrOpened -> Icons.Filled.AccountTree
    DomainContract.notificationTypePrMerged -> Icons.AutoMirrored.Filled.MergeType
    DomainContract.notificationTypeSupportReply -> Icons.Filled.SupportAgent
    else -> Icons.Filled.Notifications
}
