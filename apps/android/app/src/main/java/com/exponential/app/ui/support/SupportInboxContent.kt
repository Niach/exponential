package com.exponential.app.ui.support

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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.SupportThreadRow
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LoadingState
import com.exponential.app.ui.issue.relativeTime
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow

/**
 * The Support segment of the "My Work" tab (EXP-180): the active team's
 * support tickets, Open/Resolved filter pills over rows in the Inbox list's
 * visual language. Tap → the conversation (support/{threadId}).
 */
@Composable
fun SupportInboxContent(
    onOpenThread: (String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: SupportInboxViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    Column(modifier = modifier.fillMaxSize()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            SupportFilter.entries.forEach { filter ->
                FilterPill(
                    label = filter.label,
                    active = state.filter == filter,
                    onClick = { viewModel.setFilter(filter) },
                )
            }
        }
        Spacer(Modifier.padding(top = 8.dp))
        when {
            state.loading && state.threads.isEmpty() -> LoadingState()
            state.threads.isEmpty() -> EmptyState(
                message = state.error
                    ?: if (state.filter == SupportFilter.Resolved) {
                        "No resolved tickets."
                    } else {
                        "No open tickets."
                    },
                icon = Icons.Filled.SupportAgent,
            )
            else -> LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(
                    start = 16.dp,
                    end = 16.dp,
                    top = 4.dp,
                    bottom = BottomBarInset,
                ),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                items(state.threads, key = { it.id }) { thread ->
                    SupportThreadRowItem(thread) { onOpenThread(thread.id) }
                }
            }
        }
    }
}

@Composable
private fun FilterPill(label: String, active: Boolean, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (active) 1f else TextEmphasis.Secondary,
        ),
        modifier = Modifier
            .glassButton(active = active)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
    )
}

@Composable
private fun SupportThreadRowItem(thread: SupportThreadRow, onClick: () -> Unit) {
    val read = !thread.unread
    val reporter = thread.reporterName?.takeIf { it.isNotBlank() } ?: thread.reporterEmail
    Row(
        Modifier
            .fillMaxWidth()
            .alpha(if (read) 0.6f else 1f)
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(28.dp)
                .clip(CircleShape)
                .background(Color.White.copy(alpha = 0.08f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                Icons.Filled.SupportAgent,
                contentDescription = null,
                modifier = Modifier.size(14.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(
                thread.title,
                fontWeight = if (read) FontWeight.Normal else FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                thread.lastMessage?.let { "$reporter · ${it.body}" } ?: reporter,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.width(8.dp))
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                relativeTime(thread.lastMessage?.createdAt ?: thread.updatedAt),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            if (thread.unread) {
                Box(
                    Modifier
                        .size(8.dp)
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.primary)
                        .align(Alignment.End),
                )
            }
        }
    }
}
