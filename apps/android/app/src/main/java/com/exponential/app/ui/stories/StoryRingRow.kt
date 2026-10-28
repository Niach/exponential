package com.exponential.app.ui.stories

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.theme.TextEmphasis

/**
 * S-3 STORIES (stories.json): the Chats list's row of story rings — one per
 * agent and per root chat with stories ([Stories.rings], the active team's),
 * the ring's avatar over its title; a tap opens THE [StoryViewer] full
 * screen at that ring. Nothing (no row) while there is no ring. Interim: it
 * sits above the agent bubbles until the bubbles wear their own rings.
 * [onOpenSession] = the header's tap (the viewer closes first).
 */
@Composable
fun StoryRingRow(
    onOpenSession: (Stories.Story) -> Unit,
    viewModel: StoriesViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    var opened by remember { mutableStateOf<String?>(null) }

    if (state.rings.isNotEmpty()) {
        StoryRingStrip(
            rings = state.rings,
            avatar = state::avatar,
            onOpen = { opened = it },
            modifier = Modifier.padding(top = 4.dp, bottom = 6.dp),
        )
    }
    opened?.let { key ->
        Dialog(
            onDismissRequest = { opened = null },
            properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
        ) {
            StoryViewer(
                rings = state.rings,
                startKey = key,
                rows = state.rows,
                avatar = state::avatar,
                onAnswer = viewModel::answer,
                onDecidePlan = viewModel::decidePlan,
                onSeen = viewModel::markSeen,
                onOpenSession = { story ->
                    opened = null
                    onOpenSession(story)
                },
                onClose = { opened = null },
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** The rings side by side (the agent bubbles' metrics): the ring around its
 *  avatar, the title under it, waiting / unseen titles in full emphasis. */
@Composable
fun StoryRingStrip(
    rings: List<Stories.StoryRing>,
    avatar: (Stories.StoryRing) -> StoryAvatar,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val size = Chats.Shell.BUBBLE.dp
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        rings.forEach { ring ->
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier
                    .width(size + 8.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .clickable { onOpen(ring.key) }
                    .semantics { contentDescription = ring.title },
            ) {
                val mark = avatar(ring)
                StoryRing(stories = ring.stories, size = size) { StoryAvatarMark(mark.icon, mark.color) }
                Spacer(Modifier.height(6.dp))
                Text(
                    ring.title,
                    style = MaterialTheme.typography.labelLarge,
                    fontWeight = if (ring.unseen) FontWeight.SemiBold else FontWeight.Normal,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = if (ring.unseen) 1f else TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}
