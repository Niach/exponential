package com.exponential.app.ui.stories

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Chats
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.theme.TextEmphasis

/** A ring's avatar (stories.json AVATARS): an agent's icon in its colour,
 *  else ([color] null, a chat's root) the chat row's bare brand mark of the
 *  root's [cli]. */
data class StoryAvatar(val icon: String?, val color: String?, val cli: String? = null)

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
