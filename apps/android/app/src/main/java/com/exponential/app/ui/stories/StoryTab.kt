package com.exponential.app.ui.stories

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.Dock
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.StoryKindBadge
import com.exponential.app.ui.components.dockTint
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.plan.LocalOpenPlanPage
import com.exponential.app.ui.thread.SurfaceCard
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlin.math.roundToInt

/*
 * STORIES TAB ×Android (stories.json STORIES TAB; web `@exp/ui` story-tab,
 * desktop story_tab, iOS ExpUI StoryTab): the session screen's Stories tab
 * body, painting [Stories.storyTab]'s ring. Android = the `phone` form
 * factor ([Stories.Tab.Tile]).
 *
 *   StoryTab   the PR block slot ([lead]), `leadGap`, then the stories
 *              INLINE in fold order `listGap` apart (#EXP-1462: the Guide's
 *              way, never a list of title rows), else THE EmptyState
 *   StoryTile  one story's section: line 1 = THE kind badge (#EXP-1493),
 *              the title (one line; none = the kind word) with `meta` muted
 *              at the end (a waiting one leads with the plan/question tint
 *              dot and tints its meta), the description (muted, wrapping),
 *              then its body in place: a PLAN = its counts line muted, then
 *              its pictures row (a tap anywhere on a plan section opens THE
 *              plan page, never the viewer); else ONE sideways row of
 *              pictures (a muted caption-or-label under each), else THE
 *              surface card
 */

private val TILE_SHAPE = RoundedCornerShape(Stories.Tab.Tile.RADIUS.dp)

/** A thumbnail's width (dp): by the picture's aspect at `thumbHeight`,
 *  capped at `thumbMaxWidth`; an unknown size = 4:3. */
fun storyThumbWidth(picture: Stories.Picture): Int {
    val w = picture.width
    val h = picture.height
    val aspect = if (w != null && h != null && w > 0 && h > 0) w / h else 4.0 / 3.0
    return minOf(Stories.Tab.Tile.THUMB_MAX_WIDTH.toInt(), (Stories.Tab.Tile.THUMB_HEIGHT * aspect).roundToInt())
}

@Composable
fun StoryTile(
    story: Stories.Story,
    /** A tap on the line (picture 0) or a picture (its index). */
    onOpen: (Stories.Story, Int) -> Unit,
    modifier: Modifier = Modifier,
    /** The story's card as THE thread row (its surface); null = none. */
    row: SessionTree.ThreadRow? = null,
    /** An inline surface's reply press (THE chat's), to the story's session. */
    onReply: ((Stories.Story, String) -> Unit)? = null,
) {
    // The frame rule: a plan = its counts + pictures; else pictures win,
    // else the surface.
    val plan = story.kind == Stories.Kind.Plan
    val surface = if (!plan && story.pictures.isEmpty() && story.surface) storySurface(row) else null
    val counts = if (plan) story.counts?.let { PlanPage.countsLine(it) }?.takeIf { it.isNotEmpty() } else null
    // #EXP-1493: a tap anywhere on a plan section opens THE plan page.
    val openPlan = LocalOpenPlanPage.current
    val open: (Int) -> Unit = { index ->
        if (plan) openPlan.open(story.sessionId, story.id) else onOpen(story, index)
    }
    val tint = if (story.waiting) {
        dockTint(if (story.kind == Stories.Kind.Question) Dock.Tint.Question else Dock.Tint.Plan)
    } else {
        null
    }
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(
        modifier.fillMaxWidth().testTag(Stories.Tab.TestIds.TILE),
        verticalArrangement = Arrangement.spacedBy(Stories.Tab.Tile.GAP.dp),
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(6.dp))
                .clickable { open(0) },
            verticalArrangement = Arrangement.spacedBy(Stories.Tab.Tile.TEXT_GAP.dp),
        ) {
            Row(
                Modifier.fillMaxWidth().heightIn(min = 24.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                StoryKindBadge(story.kind)
                if (tint != null) Box(Modifier.size(6.dp).background(tint, CircleShape))
                Text(
                    story.title ?: Stories.Copy.kind(story.kind),
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    story.meta,
                    style = MaterialTheme.typography.bodySmall,
                    color = tint ?: muted,
                    maxLines = 1,
                )
            }
            story.description?.let { description ->
                Text(
                    description,
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                    modifier = Modifier.testTag("story-tile-description"),
                )
            }
        }
        if (counts != null) {
            Text(
                counts,
                style = MaterialTheme.typography.bodySmall,
                color = muted,
                maxLines = 1,
                modifier = Modifier.clickable { open(0) }.testTag("story-tile-counts"),
            )
        }
        if (story.pictures.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(Stories.Tab.Tile.GAP.dp),
            ) {
                story.pictures.forEachIndexed { index, picture ->
                    val width = storyThumbWidth(picture).dp
                    Column(
                        Modifier
                            .width(width)
                            .clickable { open(index) }
                            .testTag("story-tile-picture"),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Box(
                            Modifier
                                .width(width)
                                .height(Stories.Tab.Tile.THUMB_HEIGHT.dp)
                                .clip(TILE_SHAPE)
                                .background(DesignTokens.Palette.Muted, TILE_SHAPE)
                                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, TILE_SHAPE),
                        ) {
                            AsyncImage(
                                model = "/api/attachments/${picture.attachmentId}",
                                contentDescription = picture.caption ?: picture.label,
                                contentScale = ContentScale.Crop,
                                alignment = Alignment.TopCenter,
                                modifier = Modifier.fillMaxWidth().height(Stories.Tab.Tile.THUMB_HEIGHT.dp),
                            )
                        }
                        Text(
                            picture.caption ?: picture.label,
                            style = MaterialTheme.typography.bodySmall,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
        if (surface != null) {
            Box(Modifier.fillMaxWidth().testTag("story-tile-surface")) {
                SurfaceCard(
                    row = surface,
                    animate = false,
                    onReply = { reply -> onReply?.invoke(story, reply) },
                    showTitle = false,
                )
            }
        }
    }
}

/** THE Stories tab body: [lead] (the PR block), then the tiles of [ring]
 *  (null / no story = THE EmptyState under the lead). Not scrolling itself:
 *  the host's page scrolls it. */
@Composable
fun StoryTab(
    ring: Stories.StoryRing?,
    onOpen: (Stories.Story, Int) -> Unit,
    modifier: Modifier = Modifier,
    lead: (@Composable () -> Unit)? = null,
    /** Each story's card as THE thread row, by story id (its surface). */
    rows: Map<String, SessionTree.ThreadRow> = emptyMap(),
    onReply: ((Stories.Story, String) -> Unit)? = null,
) {
    val stories = ring?.stories.orEmpty()
    Column(
        modifier.fillMaxWidth().testTag(Stories.Tab.TestIds.LIST),
        verticalArrangement = Arrangement.spacedBy(Stories.Tab.Tile.LEAD_GAP.dp),
    ) {
        lead?.invoke()
        if (stories.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(Stories.Tab.Tile.LIST_GAP.dp)) {
                stories.forEach { story -> StoryTile(story, onOpen, row = rows[story.id], onReply = onReply) }
            }
        } else {
            EmptyState(
                title = Stories.Tab.Copy.EMPTY,
                icon = ExpIcons.threadStories,
                modifier = Modifier.padding(top = 48.dp).heightIn(min = 240.dp),
            )
        }
    }
}
