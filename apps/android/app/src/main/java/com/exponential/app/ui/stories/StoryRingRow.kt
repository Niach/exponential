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
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.AgentBubbles
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.theme.TextEmphasis

/**
 * S-3 STORIES (stories.json): THE [StoryViewer] the Chats list's story rings
 * open, full screen at ring [opened] (null = closed; phones only, so
 * [Stories.storyViewerSize] never applies). The rings themselves ride THE
 * story row ([AgentBubbles], [Stories.storyRow]); nothing here paints a row
 * of its own. [onOpenSession] = the header's tap (the viewer closes
 * first).
 */
@Composable
fun ChatStoryViewer(
    state: StoriesState,
    opened: String?,
    onClose: () -> Unit,
    onOpenSession: (Stories.Story) -> Unit,
    viewModel: StoriesViewModel,
) {
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    val key = opened ?: return
    Dialog(
        onDismissRequest = onClose,
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
                onClose()
                onOpenSession(story)
            },
            onClose = onClose,
            modifier = Modifier.fillMaxSize(),
        )
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
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 2.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        rings.forEach { ring -> StoryRingButton(ring = ring, avatar = avatar(ring), onOpen = onOpen) }
    }
}

/** One ring button of THE story row (`row`: exactly `row.itemWidth` wide,
 *  the title `row.labelGap` under the ring at the bubble size), waiting /
 *  unseen titles in full emphasis. */
@Composable
fun StoryRingButton(
    ring: Stories.StoryRing,
    avatar: StoryAvatar,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val size = Chats.Shell.BUBBLE.dp
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = modifier
            .width(Stories.Row.ITEM_WIDTH.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable { onOpen(ring.key) }
            .semantics { contentDescription = ring.title },
    ) {
        StoryRing(stories = ring.stories, size = size) { StoryAvatarMark(avatar.icon, avatar.color, cli = avatar.cli) }
        Spacer(Modifier.height(Stories.Row.LABEL_GAP.dp))
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
