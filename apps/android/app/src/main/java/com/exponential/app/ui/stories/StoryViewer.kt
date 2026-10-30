package com.exponential.app.ui.stories

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
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
 * S-3 STORY VIEWER (stories.json, styleguide `story-viewer`; #EXP-1432):
 * FULL SCREEN, Instagram-like. It steps through FRAMES ([Stories.storyFrames]:
 * one per picture, else the surface, else the plan text, else the bare
 * title), never through stories with an inner pager. The body fills the
 * screen on black: a picture FITTED edge to edge (behind the status bar), a
 * surface through THE renderer with no card chrome ([SurfaceCard] `chrome`
 * false), the plan's markdown; both scroll under the bands. FLOATING over
 * the top on a scrim: THE [StoryProgress] (one segment per frame, grouped
 * by story), the header (the ring's avatar, the story's session name, its
 * meta muted, the close ghost glyph; a tap on the avatar or name opens that
 * session), the frame's title and description. The footer (the story's
 * LAST frame only, [StoryAnswerFooter]) floats over the bottom on the
 * mirrored scrim. NAVIGATION: the right third / a swipe left = next frame
 * (past the ring's last = the next ring, past the last ring = close), the
 * left third / a swipe right = previous, a swipe down / back = close, a held
 * press pauses; every frame has its own timer (`autoAdvanceMs`: results).
 * The order is the rings' as the viewer OPENED; an answered story drops out
 * and the viewer moves on. Each story is reported SHOWN ([onSeen]) once.
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
    /** The styleguide's still: the timer never runs, the progress frozen at [frozenFraction]. */
    frozen: Boolean = false,
    frozenFraction: Float = 0f,
    /** STORIES TAB: open at this story of [startKey]'s ring (null = its first)… */
    startStoryId: String? = null,
    /** …on this picture's frame (a thumbnail's index; [Stories.storyFrameIndex]). */
    startPicture: Int = 0,
) {
    data class Entry(val ringKey: String, val frame: Stories.Frame)

    val order = remember {
        rings.flatMap { ring -> Stories.storyFrames(ring.stories).map { Entry(ring.key, it) } }
    }
    var position by remember {
        val first = order.indexOfFirst { it.ringKey == startKey }
        val ringFrames = order.filter { it.ringKey == startKey }.map { it.frame }
        val at = startStoryId?.let { Stories.storyFrameIndex(ringFrames, it, startPicture) } ?: -1
        mutableIntStateOf(if (first >= 0 && at >= 0) first + at else first.coerceAtLeast(0))
    }
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
    fun present(index: Int) = index in order.indices && latest[order[index].frame.storyId] != null
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
    // next frame still there, none = close.
    LaunchedEffect(live) {
        if (order.isEmpty()) close() else if (!present(position)) next()
    }
    BackHandler(onBack = onClose)

    val entry = order.getOrNull(position)
    val current = entry?.let { live[it.frame.storyId] }
    Box(
        modifier
            .fillMaxSize()
            .background(Color.Black)
            .testTag(Stories.TestIds.VIEWER)
            .semantics {
                customActions = listOf(
                    CustomAccessibilityAction(Stories.Copy.NEXT) { next(); true },
                    CustomAccessibilityAction(Stories.Copy.PREVIOUS) { previous(); true },
                )
            },
    ) {
        if (entry == null || current == null) return@Box
        val frame = entry.frame
        val (ring, story) = current
        // The ring's frames still in the fold = the progress segments.
        val segments = order.indices.filter { order[it].ringKey == ring.key && present(it) }
        LaunchedEffect(position) {
            if (shown.add(story.id)) onSeen(story)
            val duration = frame.autoAdvanceMs ?: return@LaunchedEffect
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
        var topPx by remember { mutableIntStateOf(0) }
        var bottomPx by remember { mutableIntStateOf(0) }
        val density = LocalDensity.current
        StoryNavigable(
            key = position,
            onNext = ::next,
            onPrevious = ::previous,
            onClose = onClose,
            onPause = { paused = it },
            modifier = Modifier.fillMaxSize(),
        ) {
            androidx.compose.runtime.key(position) {
                StoryFrameBody(
                    story = story,
                    frame = frame,
                    row = rows[story.id],
                    top = with(density) { topPx.toDp() },
                    bottom = with(density) { bottomPx.toDp() },
                )
            }
        }
        // The top band: progress, header, title, description over the scrim.
        Box(
            Modifier
                .align(Alignment.TopCenter)
                .fillMaxWidth()
                .storyScrim(fadeTowardsBottom = true),
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .onSizeChanged { topPx = it.height }
                    .statusBarsPadding()
                    .padding(start = 12.dp, end = 12.dp, top = 12.dp, bottom = 12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                StoryProgress(
                    count = segments.size,
                    index = segments.indexOf(position).coerceAtLeast(0),
                    // The styleguide's still freezes it (stories.json `specimen.viewer`).
                    fraction = if (frozen) frozenFraction else frame.autoAdvanceMs?.let { elapsed / it } ?: 1f,
                    groups = segments.map { order[it].frame.story },
                )
                StoryHeader(ring, story, avatar(ring), onOpen = { onOpenSession(story) }, onClose = onClose)
                if (frame.title != null || frame.description != null) {
                    Column(Modifier.fillMaxWidth().padding(horizontal = 2.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        frame.title?.let {
                            Text(
                                it,
                                style = MaterialTheme.typography.titleMedium,
                                fontWeight = FontWeight.SemiBold,
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.testTag("story-title"),
                            )
                        }
                        frame.description?.let {
                            Text(
                                it,
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                                maxLines = 3,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.testTag("story-description"),
                            )
                        }
                    }
                }
            }
        }
        // The footer (the story's last frame only) over the mirrored scrim.
        if (frame.footer != Stories.Footer.None) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .storyScrim(fadeTowardsBottom = false),
                contentAlignment = Alignment.BottomCenter,
            ) {
                Box(
                    Modifier
                        .fillMaxWidth()
                        .onSizeChanged { bottomPx = it.height }
                        // A press on the footer never steps the viewer.
                        .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } }
                        .navigationBarsPadding()
                        .imePadding()
                        .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 16.dp),
                ) {
                    androidx.compose.runtime.key(story.id) {
                        StoryAnswerFooter(rows[story.id], frame.footer, onAnswer, onDecidePlan)
                    }
                }
            }
        } else {
            SideEffect { if (bottomPx != 0) bottomPx = 0 }
        }
    }
}

/**
 * A floating band's scrim (stories.json `viewer.scrimHeight` / `scrimFade`):
 * at least the height tall, black 0.88 at the screen's edge, 0.8 behind the
 * band's text, fading to clear over the last `scrimFade` dp past it, so a
 * picture's own text never collides with the title. [fadeTowardsBottom] =
 * the top band (it fades downwards); false = the footer's mirror.
 */
private fun Modifier.storyScrim(fadeTowardsBottom: Boolean): Modifier = this
    .heightIn(min = Stories.Viewer.SCRIM_HEIGHT.dp)
    .drawBehind {
        val fade = Stories.Viewer.SCRIM_FADE.dp.toPx()
        val h = size.height.coerceAtLeast(1f)
        val solid = Color.Black.copy(alpha = 0.88f)
        val text = Color.Black.copy(alpha = 0.8f)
        val brush = if (fadeTowardsBottom) {
            Brush.verticalGradient(
                0f to solid,
                ((h - fade) / h).coerceIn(0f, 1f) to text,
                1f to Color.Transparent,
            )
        } else {
            Brush.verticalGradient(
                0f to Color.Transparent,
                (fade / h).coerceIn(0f, 1f) to text,
                1f to solid,
            )
        }
        drawRect(brush)
    }
    .padding(
        top = if (fadeTowardsBottom) 0.dp else Stories.Viewer.SCRIM_FADE.dp,
        bottom = if (fadeTowardsBottom) Stories.Viewer.SCRIM_FADE.dp else 0.dp,
    )

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
 * The whole screen under the navigation: the tap thirds, the swipes and the
 * held press. A gesture a child took (a surface's button, the footer, a
 * scroll) never navigates.
 */
@Composable
private fun StoryNavigable(
    key: Any,
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
    Box(
        modifier.pointerInput(key) {
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
    ) {
        content()
    }
}

/** One frame's body, filling the screen on black: a picture fitted, THE
 *  surface with no card chrome, the plan text, or nothing (a title frame).
 *  [top] / [bottom] = the floating bands' heights (the scrolling body starts
 *  under the top one and clears the footer). */
@Composable
private fun StoryFrameBody(
    story: Stories.Story,
    frame: Stories.Frame,
    row: SessionTree.ThreadRow?,
    top: androidx.compose.ui.unit.Dp,
    bottom: androidx.compose.ui.unit.Dp,
) {
    when (frame.kind) {
        Stories.FrameKind.Picture -> {
            val picture = frame.picture?.let { story.pictures.getOrNull(it) } ?: return
            StoryPicture(picture)
        }
        Stories.FrameKind.Surface -> {
            val surface = storySurface(row) ?: return
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(start = 16.dp, end = 16.dp, top = top + 8.dp, bottom = bottom + 16.dp),
            ) {
                SurfaceCard(surface, animate = false, onReply = {}, showTitle = false, chrome = false)
            }
        }
        Stories.FrameKind.Text -> {
            val text = story.text ?: return
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(start = 16.dp, end = 16.dp, top = top + 8.dp, bottom = bottom + 16.dp),
            ) {
                MarkdownView(PlanCard.withoutPendingUploads(text), softBreaksAsNewlines = true)
            }
        }
        Stories.FrameKind.Title -> Unit
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

/** One picture through the app's attachment image loader, FITTED on black,
 *  edge to edge (the muted glyph shows while it loads). */
@Composable
private fun StoryPicture(picture: Stories.Picture) {
    Box(Modifier.fillMaxSize().testTag("story-picture"), contentAlignment = Alignment.Center) {
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
