package com.exponential.app.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Dock
import com.exponential.app.domain.Stories
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// S-3 STORIES ×4 (fixtures/stories.json `ring` + `progress`, styleguide
// `story-ring` / `story-viewer`; web, desktop and iOS draw the same two
// primitives): THE story ring around an avatar and THE story progress bar on
// top of the viewer. Every number comes from [Stories.Ring] /
// [Stories.Progress].

/** A story's arc colour: waiting = the ask dock's plan/question tint, unseen
 *  = the foreground, seen = the glass-stroke hairline. */
@Composable
fun storyArcColor(story: Stories.Story): Color = when {
    story.waiting -> dockTint(if (story.kind == Stories.Kind.Question) Dock.Tint.Question else Dock.Tint.Plan)
    story.seen -> GlassTokens.StrokeCard
    else -> MaterialTheme.colorScheme.onSurface
}

/** The background gap between the ring and the avatar (the bubbles' 3dp). */
private val StoryRingInset: Dp = Stories.Ring.WIDTH.dp + 3.dp

/**
 * An avatar inside a ring split into one arc per story, [Stories.Ring.GAP]
 * degrees apart (one story = a closed circle), [Stories.Ring.WIDTH] wide,
 * starting at twelve o'clock and running clockwise in story order.
 */
@Composable
fun StoryRing(
    stories: List<Stories.Story>,
    size: Dp,
    modifier: Modifier = Modifier,
    avatar: @Composable () -> Unit,
) {
    StoryRingArcs(colors = stories.map { storyArcColor(it) }, size = size, modifier = modifier, avatar = avatar)
}

/** The ring by its arcs' colours in order. */
@Composable
fun StoryRingArcs(
    colors: List<Color>,
    size: Dp,
    modifier: Modifier = Modifier,
    avatar: @Composable () -> Unit,
) {
    Box(
        modifier = modifier.size(size).testTag(Stories.TestIds.RING),
        contentAlignment = Alignment.Center,
    ) {
        Canvas(Modifier.fillMaxSize()) {
            val stroke = Stories.Ring.WIDTH.dp.toPx()
            val inset = stroke / 2
            val arcSize = Size(this.size.width - stroke, this.size.height - stroke)
            val count = colors.size
            if (count == 0) return@Canvas
            if (count == 1) {
                drawArc(colors[0], -90f, 360f, false, Offset(inset, inset), arcSize, style = Stroke(stroke))
                return@Canvas
            }
            val gap = Stories.Ring.GAP
            val sweep = (360f - gap * count) / count
            colors.forEachIndexed { index, color ->
                val start = -90f + gap / 2 + index * (sweep + gap)
                drawArc(color, start, sweep, false, Offset(inset, inset), arcSize, style = Stroke(stroke))
            }
        }
        Box(
            modifier = Modifier.size(size - StoryRingInset * 2).clip(CircleShape),
            contentAlignment = Alignment.Center,
        ) { avatar() }
    }
}

/**
 * THE story progress: ONE segment per STORY of the ring (#EXP-1493),
 * [Stories.Progress.GAP] apart, [Stories.Progress.HEIGHT] tall; done =
 * foreground, the current = its fill [fraction], todo = the glass-stroke
 * track.
 */
@Composable
fun StoryProgress(
    count: Int,
    index: Int,
    fraction: Float,
    modifier: Modifier = Modifier,
) {
    val foreground = MaterialTheme.colorScheme.onSurface
    val shape = RoundedCornerShape(50)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(Stories.Progress.HEIGHT.dp)
            .semantics { contentDescription = "${minOf(index + 1, count)} / $count" }
            .testTag(Stories.TestIds.PROGRESS),
        horizontalArrangement = Arrangement.spacedBy(Stories.Progress.GAP.dp),
    ) {
        repeat(count.coerceAtLeast(0)) { segment ->
            val fill = when {
                segment < index -> 1f
                segment == index -> fraction.coerceIn(0f, 1f)
                else -> 0f
            }
            BoxWithConstraints(
                modifier = Modifier
                    .weight(1f)
                    .height(Stories.Progress.HEIGHT.dp)
                    .clip(shape)
                    .background(GlassTokens.StrokeCard, shape),
            ) {
                Box(Modifier.width(maxWidth * fill).height(Stories.Progress.HEIGHT.dp).background(foreground, shape))
            }
        }
    }
}

/** A kind's tint (stories.json `badge`, the ask-dock tints): plan = the
 *  primary, question = the yellow, result = the closed-check green. */
@Composable
fun storyKindTint(kind: Stories.Kind): Color = when (kind) {
    Stories.Kind.Plan -> dockTint(Dock.Tint.Plan)
    Stories.Kind.Question -> dockTint(Dock.Tint.Question)
    Stories.Kind.Result -> DesignTokens.Semantic.Green
}

/** The word a waiting plan adds on the plan page (` · waiting`). */
const val STORY_BADGE_WAITING = "waiting"

/**
 * #EXP-1493 THE kind badge ×4 (stories.json `badge` = THE Pill, sm,
 * readonly): the concept icon ([Stories.Badge.icon]) + the kind word in the
 * tint, the tint at [Stories.Badge.FILL] behind and at
 * [Stories.Badge.STROKE] as its border, a capsule. [waiting] = the plan
 * page's ` · waiting`.
 */
@Composable
fun StoryKindBadge(kind: Stories.Kind, waiting: Boolean = false, modifier: Modifier = Modifier) {
    val tint = storyKindTint(kind)
    val shape = RoundedCornerShape(50)
    val word = Stories.Copy.kind(kind)
    Row(
        modifier = modifier
            .clip(shape)
            .background(tint.copy(alpha = Stories.Badge.FILL), shape)
            .border(1.dp, tint.copy(alpha = Stories.Badge.STROKE), shape)
            .padding(horizontal = 8.dp, vertical = 3.dp)
            .testTag("story-badge"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Icon(
            ExpIcons.byConcept(Stories.Badge.icon(kind)) ?: ExpIcons.codingPlan,
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(11.dp),
        )
        Text(
            if (waiting) "$word · $STORY_BADGE_WAITING" else word,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Medium,
            color = tint,
            maxLines = 1,
        )
    }
}

/**
 * The story viewer's thumbnail row (#EXP-1493): the plan cover's (40dp
 * tall, 56dp wide, up to 4 + `+N` muted) and the gallery filmstrip (52dp
 * squares: [width] null); the [current] = a 2dp foreground ring, the rest
 * the glass-stroke hairline. A tap = [onPick] (index).
 */
@Composable
fun StoryThumbnailStrip(
    count: Int,
    current: Int?,
    height: Dp,
    width: Dp? = null,
    limit: Int? = null,
    onPick: (Int) -> Unit,
    modifier: Modifier = Modifier,
    thumb: @Composable (Int) -> Unit,
) {
    val shown = minOf(count, limit ?: count).coerceAtLeast(0)
    val shape = RoundedCornerShape((Stories.Tab.Tile.RADIUS * 0.8f).dp)
    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        repeat(shown) { index ->
            val on = index == current
            Box(
                Modifier
                    .size(width = width ?: height, height = height)
                    .clip(shape)
                    .background(DesignTokens.Palette.Muted, shape)
                    .clickable { onPick(index) }
                    .testTag("story-thumbnail"),
            ) {
                thumb(index)
                Box(
                    Modifier
                        .fillMaxSize()
                        .border(
                            if (on) 2.dp else GlassTokens.Hairline,
                            if (on) MaterialTheme.colorScheme.onSurface else GlassTokens.StrokeCard,
                            shape,
                        ),
                )
            }
        }
        if (shown < count) {
            Text(
                "+${count - shown}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

/** A ring's avatar (stories.json AVATARS ×4): an agent ring = its mark
 *  ([AgentAvatar]: its glyph in its colour on the muted disc, the bubble's
 *  look); a root chat ring ([color] null) = the BARE brand mark of [cli]
 *  (the root's CLI; claude when unknown) on the same disc, no badge, no PR
 *  glyph, as web, iOS and the IDE draw it. Fills the ring's inner circle. */
@Composable
fun StoryAvatarMark(icon: String?, color: String?, modifier: Modifier = Modifier, cli: String? = null) {
    BoxWithConstraints(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        val side = maxWidth
        if (color != null) {
            AgentAvatar(icon = icon, color = color, size = side)
        } else {
            val id = cli?.trim()?.lowercase()?.ifEmpty { null } ?: "claude"
            Box(
                Modifier.size(side).clip(CircleShape).background(DesignTokens.Palette.Muted, CircleShape),
                contentAlignment = Alignment.Center,
            ) {
                Icon(agentIconPainter(id), contentDescription = null, tint = agentIconTint(id), modifier = Modifier.size(side * 0.5f))
            }
        }
    }
}
