package com.exponential.app.ui.stories

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryProgress
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.StoryAnswerFooter
import com.exponential.app.ui.thread.SurfaceCard
import com.exponential.app.ui.thread.ThreadAnswer
import com.exponential.app.ui.thread.ThreadPlanDecision
import kotlin.math.abs
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val TICK_MS = 50L
private const val HOLD_MS = 250L
private val SWIPE = 60.dp
private val AVATAR = 36.dp

/**
 * S-3 STORY VIEWER (stories.json, styleguide `story-viewer`): full screen on
 * the phone. Top: THE [StoryProgress] over the ring's stories, then the
 * header (the ring's avatar, the story's session name, its meta muted, the
 * close ghost glyph; a tap on the avatar or name opens that session). Body:
 * the one-line title, then the pictures (fitted, a pager when several, the
 * caption under it) and/or the surface through THE surface card renderer
 * ([SurfaceCard], chats.json `surfaceCard`), else the plan's markdown.
 * Footer per `footer` ([StoryAnswerFooter]: THE card's answer controls and
 * `sessions.answer`). NAVIGATION: the right third / a swipe left = next
 * (past the ring's last = the next ring, past the last ring = close), the
 * left third / a swipe right = previous, a swipe down / back = close, a held
 * press pauses; only a result advances on its own (`autoAdvanceMs`). The
 * order is the rings' as the viewer OPENED (marking a story seen reorders the
 * live fold); an answered story drops out and the viewer moves on. Each
 * story is reported SHOWN ([onSeen]) once.
 */
@Composable
fun StoryViewer(
    /** The live rings (the fold recomputes from sync while the viewer is up). */
    rings: List<Stories.StoryRing>,
    startKey: String,
    /** Each story's card as THE thread row (body surface + footer). */
    rows: Map<String, SessionTree.ThreadRow>,
    avatar: (Stories.StoryRing) -> StoryAvatar,
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    onSeen: (Stories.Story) -> Unit = {},
    onOpenSession: (Stories.Story) -> Unit = {},
    /** The styleguide's still: the timer never runs, the progress stays at 0. */
    frozen: Boolean = false,
) {
    data class Entry(val ringKey: String, val storyId: String)

    val order = remember {
        rings.flatMap { ring -> ring.stories.map { Entry(ring.key, it.id) } }
    }
    var position by remember { mutableIntStateOf(order.indexOfFirst { it.ringKey == startKey }.coerceAtLeast(0)) }
    var elapsed by remember { mutableFloatStateOf(0f) }
    var paused by remember { mutableStateOf(false) }
    val shown = remember { HashSet<String>() }
    val close by rememberUpdatedState(onClose)

    val live: Map<String, Pair<Stories.StoryRing, Stories.Story>> = remember(rings) {
        buildMap { rings.forEach { ring -> ring.stories.forEach { put(it.id, ring to it) } } }
    }
    // The timer's and the gestures' closures outlive a recomposition: they
    // read the latest fold through this.
    val latest by rememberUpdatedState(live)
    fun present(index: Int) = index in order.indices && latest[order[index].storyId] != null
    fun go(index: Int) {
        position = index
        elapsed = 0f
    }
    fun next() {
        val target = order.indices.firstOrNull { it > position && present(it) }
        if (target != null) go(target) else close()
    }
    fun previous() {
        val target = order.indices.lastOrNull { it < position && present(it) }
        if (target != null) go(target) else elapsed = 0f
    }

    // Nothing to show, or the current story left the fold (answered): the
    // next one still there, none = close.
    LaunchedEffect(live) {
        if (order.isEmpty()) close() else if (!present(position)) next()
    }
    BackHandler(onBack = onClose)

    val current = order.getOrNull(position)?.let { live[it.storyId] }
    AppBackground(
        modifier
            .testTag(Stories.TestIds.VIEWER)
            .semantics {
                customActions = listOf(
                    CustomAccessibilityAction(Stories.Copy.NEXT) { next(); true },
                    CustomAccessibilityAction(Stories.Copy.PREVIOUS) { previous(); true },
                )
            },
    ) {
        if (current == null) return@AppBackground
        val (ring, story) = current
        val segments = order.indices.filter { order[it].ringKey == ring.key && present(it) }
        LaunchedEffect(story.id) {
            if (shown.add(story.id)) onSeen(story)
            val duration = story.autoAdvanceMs ?: return@LaunchedEffect
            if (frozen) return@LaunchedEffect
            while (true) {
                delay(TICK_MS)
                if (paused) continue
                elapsed += TICK_MS
                if (elapsed >= duration) {
                    next()
                    return@LaunchedEffect
                }
            }
        }
        Column(
            Modifier.fillMaxSize().systemBarsPadding().padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            StoryProgress(
                count = segments.size,
                index = segments.indexOf(position).coerceAtLeast(0),
                // The styleguide's still freezes it at 0 (stories.json `specimen.viewer`).
                fraction = if (frozen) 0f else story.autoAdvanceMs?.let { elapsed / it } ?: 1f,
            )
            StoryHeader(ring, story, avatar(ring), onOpen = { onOpenSession(story) }, onClose = onClose)
            StoryNavigable(
                story = story,
                onNext = ::next,
                onPrevious = ::previous,
                onClose = onClose,
                onPause = { paused = it },
                modifier = Modifier.weight(1f),
            ) {
                StoryBody(story, rows[story.id])
            }
            androidx.compose.runtime.key(story.id) {
                StoryAnswerFooter(rows[story.id], story.footer, onAnswer, onDecidePlan)
            }
        }
    }
}

@Composable
private fun StoryHeader(
    ring: Stories.StoryRing,
    story: Stories.Story,
    avatar: StoryAvatar,
    onOpen: () -> Unit,
    onClose: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().testTag(Stories.TestIds.HEADER),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(
            Modifier.weight(1f).clip(RoundedCornerShape(12.dp)).clickable(onClick = onOpen),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            StoryRing(stories = ring.stories, size = AVATAR) { StoryAvatarMark(avatar.icon, avatar.color, cli = avatar.cli) }
            Column(Modifier.weight(1f)) {
                Text(
                    story.sessionName,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    story.meta,
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Normal),
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                )
            }
        }
        CircleIconButton(ExpIcons.uiClose, Stories.Copy.CLOSE, onClick = onClose, borderless = true)
    }
}

/**
 * The title + body under the navigation: the tap thirds, the swipes and the
 * held press. A gesture a child took (a surface's button, the pictures'
 * pager, a scroll not at its top) never navigates.
 */
@Composable
private fun StoryNavigable(
    story: Stories.Story,
    onNext: () -> Unit,
    onPrevious: () -> Unit,
    onClose: () -> Unit,
    onPause: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val swipe = with(LocalDensity.current) { SWIPE.toPx() }
    val next by rememberUpdatedState(onNext)
    val previous by rememberUpdatedState(onPrevious)
    val close by rememberUpdatedState(onClose)
    val pause by rememberUpdatedState(onPause)
    Column(
        modifier
            .fillMaxWidth()
            .pointerInput(story.id) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val start = down.position
                    var last = start
                    var held = false
                    var taken = false
                    val hold: Job = scope.launch {
                        delay(HOLD_MS)
                        held = true
                        pause(true)
                    }
                    while (true) {
                        val event = awaitPointerEvent()
                        val change = event.changes.firstOrNull { it.id == down.id } ?: event.changes.first()
                        if (change.isConsumed) taken = true
                        last = change.position
                        if (event.changes.none { it.pressed }) break
                    }
                    hold.cancel()
                    if (held) pause(false)
                    if (taken) return@awaitEachGesture
                    val dx = last.x - start.x
                    val dy = last.y - start.y
                    when {
                        abs(dx) > swipe && abs(dx) > 2 * abs(dy) -> if (dx < 0) next() else previous()
                        dy > swipe && dy > 2 * abs(dx) -> close()
                        !held && abs(dx) < 10 && abs(dy) < 10 -> when {
                            start.x > size.width * 2f / 3f -> next()
                            start.x < size.width / 3f -> previous()
                        }
                    }
                }
            },
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // The plan text already begins with its title line (stories.json).
        story.title?.takeIf { story.text == null }?.let {
            Text(
                it,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Box(Modifier.fillMaxWidth().weight(1f)) { content() }
    }
}

/** A story's body: the pictures and/or the surface, else the plan text. */
@Composable
private fun StoryBody(story: Stories.Story, row: SessionTree.ThreadRow?) {
    val surface = if (story.surface) storySurface(row) else null
    Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        when {
            story.pictures.isNotEmpty() -> {
                StoryPictures(story.pictures, Modifier.fillMaxWidth().weight(1f))
                surface?.let { SurfaceCard(it, animate = false, onReply = {}) }
            }
            surface != null -> Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
                SurfaceCard(surface, animate = false, onReply = {})
            }
            story.text != null -> Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
                MarkdownView(PlanCard.withoutPendingUploads(story.text), softBreaksAsNewlines = true)
            }
        }
    }
}

/** THE surface card's row for a story's surface (a result's own, or a
 *  plan's / question's inline surface drawn by the same renderer). */
private fun storySurface(row: SessionTree.ThreadRow?): SessionTree.ThreadRow.Surface? = when (row) {
    is SessionTree.ThreadRow.Surface -> row
    is SessionTree.ThreadRow.Plan -> row.surface?.let { SessionTree.ThreadRow.Surface(row.id, row.sessionId, null, it) }
    is SessionTree.ThreadRow.Question -> row.surface?.let { SessionTree.ThreadRow.Surface(row.id, row.sessionId, null, it) }
    else -> null
}

/** The pictures, fitted: one alone, several in a horizontal pager; the
 *  current one's caption under it. */
@Composable
private fun StoryPictures(pictures: List<Stories.Picture>, modifier: Modifier = Modifier) {
    val pager = rememberPagerState { pictures.size }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.fillMaxWidth().weight(1f), contentAlignment = Alignment.Center) {
            if (pictures.size == 1) {
                StoryPicture(pictures[0])
            } else {
                HorizontalPager(pager, Modifier.fillMaxSize(), pageSpacing = 12.dp) { page ->
                    StoryPicture(pictures[page])
                }
            }
        }
        if (pictures.size > 1) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                repeat(pictures.size) { index ->
                    Box(
                        Modifier
                            .padding(horizontal = 3.dp)
                            .size(6.dp)
                            .clip(RoundedCornerShape(50))
                            .background(
                                if (index == pager.currentPage) {
                                    MaterialTheme.colorScheme.onSurface
                                } else {
                                    GlassTokens.StrokeCard
                                },
                            ),
                    )
                }
            }
        }
        pictures[pager.currentPage.coerceIn(0, pictures.lastIndex)].caption?.takeIf { it.isNotBlank() }?.let {
            Text(
                it,
                modifier = Modifier.fillMaxWidth(),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                textAlign = TextAlign.Center,
            )
        }
    }
}

/** One picture through the app's attachment image loader, FITTED, over the
 *  muted frame (what shows while it loads). */
@Composable
private fun StoryPicture(picture: Stories.Picture) {
    val shape = RoundedCornerShape(GlassTokens.GroupRadius)
    val aspect = picture.width?.let { w -> picture.height?.takeIf { it > 0 && w > 0 }?.let { (w / it).toFloat() } } ?: (4f / 3f)
    Box(
        Modifier
            .fillMaxWidth()
            .aspectRatio(aspect, matchHeightConstraintsFirst = true)
            .clip(shape)
            .background(GlassTokens.CardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.editorImage,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            modifier = Modifier.size(28.dp),
        )
        AsyncImage(
            model = "/api/attachments/${picture.attachmentId}",
            contentDescription = picture.caption ?: picture.label,
            contentScale = ContentScale.Fit,
            modifier = Modifier.fillMaxSize(),
        )
    }
}
