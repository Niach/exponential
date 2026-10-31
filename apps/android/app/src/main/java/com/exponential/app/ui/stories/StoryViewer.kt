package com.exponential.app.ui.stories

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
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
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.StoryAvatarMark
import com.exponential.app.ui.components.StoryKindBadge
import com.exponential.app.ui.components.StoryProgress
import com.exponential.app.ui.components.StoryRing
import com.exponential.app.ui.components.StoryThumbnailStrip
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.StoryAnswerFooter
import com.exponential.app.ui.thread.SurfaceCard
import com.exponential.app.ui.thread.ThreadAnswer
import kotlin.math.abs
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val TICK_MS = 50L
private const val HOLD_MS = 250L
private val SWIPE = 60.dp
private val AVATAR = 36.dp

/**
 * S-3 STORY VIEWER (stories.json, styleguide `story-viewer`; #EXP-1493 ONE
 * FRAME PER STORY): FULL SCREEN, Instagram-like. It steps through the
 * ring's stories ([Stories.storyFrames]): a PLAN = its cover (the first
 * picture fitted on black, a tap = Open plan; the counts line, up to 4
 * thumbnails + `+N`, ONE full-width `Open plan` opening THE plan page via
 * [onOpenPlan]); a GALLERY = the story's pictures one at a time (a tap on
 * the right third / a swipe steps the picture first, the "i / n" counter
 * top-right, the caption (else label) muted under it, the filmstrip over
 * the bottom scrim when n > 1); a SURFACE through THE renderer with no card
 * chrome ([SurfaceCard] `chrome` false), scrolling under the bands; a TITLE
 * = nothing. FLOATING over the top on a scrim: THE [StoryProgress] (one
 * segment per story), the header (the ring's avatar, the story's session
 * name, its meta muted, the close ghost glyph; a tap on the avatar or name
 * opens that session), THE kind badge leading the title, the description.
 * The footer ([StoryAnswerFooter], a plan's cover foot, a gallery's
 * filmstrip) floats over the bottom on the mirrored scrim. NAVIGATION: the
 * right third / a swipe left = next (a gallery's next picture first; past
 * the ring's last = the next ring, past the last ring = close), the left
 * third / a swipe right = previous, a swipe down / back = close, a held
 * press pauses; a gallery's timer steps its pictures every `autoAdvanceMs`
 * then the story (a decision's never). The order is the rings' as the
 * viewer OPENED; an answered story drops out and the viewer moves on. Each
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
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    onSeen: (Stories.Story) -> Unit = {},
    onOpenSession: (Stories.Story) -> Unit = {},
    /** #EXP-1493: a plan frame's Open plan (the host closes the viewer and
     *  opens THE plan page). */
    onOpenPlan: (Stories.Story) -> Unit = {},
    /** The styleguide's still: the timer never runs, the progress frozen at [frozenFraction]. */
    frozen: Boolean = false,
    frozenFraction: Float = 0f,
    /** STORIES TAB: open at this story of [startKey]'s ring (null = its first)… */
    startStoryId: String? = null,
    /** …with its gallery at this picture (a thumbnail's index). */
    startPicture: Int = 0,
) {
    data class Entry(val ringKey: String, val frame: Stories.Frame)

    val order = remember {
        rings.flatMap { ring -> Stories.storyFrames(ring.stories).map { Entry(ring.key, it) } }
    }
    var position by remember {
        val first = order.indexOfFirst { it.ringKey == startKey }
        val ringFrames = order.filter { it.ringKey == startKey }.map { it.frame }
        val at = startStoryId?.let { Stories.storyFrameIndex(ringFrames, it) } ?: -1
        mutableIntStateOf(if (first >= 0 && at >= 0) first + at else first.coerceAtLeast(0))
    }
    // A gallery's current picture (a thumbnail opens its story at it).
    var picture by remember {
        val frame = order.getOrNull(position)?.frame
        val opensAt = frame != null && frame.storyId == startStoryId && frame.kind == Stories.FrameKind.Gallery
        mutableIntStateOf(if (opensAt) startPicture.coerceIn(0, (frame!!.pictures - 1).coerceAtLeast(0)) else 0)
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
    /** The steps inside a story: a gallery's pictures, else one. */
    fun pictureCount(index: Int): Int =
        order.getOrNull(index)?.frame?.takeIf { it.kind == Stories.FrameKind.Gallery }?.pictures?.coerceAtLeast(1) ?: 1
    fun go(index: Int) {
        position = index
        picture = 0
        elapsed = 0f
    }
    fun next() {
        if (present(position) && picture + 1 < pictureCount(position)) {
            picture += 1
            elapsed = 0f
            return
        }
        val target = order.indices.firstOrNull { it > position && present(it) }
        if (target != null) go(target) else close()
    }
    fun previous() {
        if (picture > 0) {
            picture -= 1
            elapsed = 0f
            return
        }
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
        // The ring's stories still in the fold = the progress segments.
        val segments = order.indices.filter { order[it].ringKey == ring.key && present(it) }
        val gallery = frame.kind == Stories.FrameKind.Gallery && frame.pictures > 0
        val own = if (frozen) frozenFraction else frame.autoAdvanceMs?.let { elapsed / it } ?: 1f
        // A gallery's fill = (picture + its fraction) / n.
        val progress = if (gallery && !frozen) (picture + own.coerceIn(0f, 1f)) / frame.pictures else own
        LaunchedEffect(position, picture) {
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
            key = position to picture,
            onNext = ::next,
            onPrevious = ::previous,
            onClose = onClose,
            onPause = { paused = it },
            modifier = Modifier.fillMaxSize(),
        ) {
            androidx.compose.runtime.key(position, picture) {
                StoryFrameBody(
                    story = story,
                    frame = frame,
                    picture = picture,
                    row = rows[story.id],
                    top = with(density) { topPx.toDp() },
                    bottom = with(density) { bottomPx.toDp() },
                    onOpenPlan = { onOpenPlan(story) },
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
                    fraction = progress,
                )
                StoryHeader(ring, story, avatar(ring), onOpen = { onOpenSession(story) }, onClose = onClose)
                // THE kind badge leads the title line (#EXP-1493).
                Column(Modifier.fillMaxWidth().padding(horizontal = 2.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    StoryKindBadge(story.kind)
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
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
        // The footer over the mirrored scrim: a plan's cover foot, a
        // gallery's filmstrip, the story's answer footer.
        val filmstrip = gallery && story.pictures.size > 1
        val answers = frame.footer == Stories.Footer.Choices || frame.footer == Stories.Footer.Reply
        if (frame.kind == Stories.FrameKind.Plan || filmstrip || answers) {
            Box(
                Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .storyScrim(fadeTowardsBottom = false),
                contentAlignment = Alignment.BottomCenter,
            ) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .onSizeChanged { bottomPx = it.height }
                        // A press on the footer never steps the viewer.
                        .pointerInput(Unit) { awaitEachGesture { awaitFirstDown(requireUnconsumed = false) } }
                        .navigationBarsPadding()
                        .imePadding()
                        .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    if (frame.kind == Stories.FrameKind.Plan) {
                        StoryPlanFoot(story, onOpenPlan = { onOpenPlan(story) })
                    } else {
                        if (filmstrip) {
                            StoryThumbnailStrip(
                                count = story.pictures.size,
                                current = picture,
                                height = 52.dp,
                                onPick = { index ->
                                    picture = index
                                    elapsed = 0f
                                },
                                modifier = Modifier.align(Alignment.CenterHorizontally),
                            ) { index -> StoryThumb(story.pictures[index]) }
                        }
                        if (answers) {
                            androidx.compose.runtime.key(story.id) {
                                StoryAnswerFooter(rows[story.id], frame.footer, onAnswer)
                            }
                        }
                    }
                }
            }
        } else {
            SideEffect { if (bottomPx != 0) bottomPx = 0 }
        }
    }
}

/** A plan cover's foot: the counts line, up to 4 thumbnails (40dp tall,
 *  56dp wide) + `+N`, ONE full-width primary `Open plan`. */
@Composable
private fun StoryPlanFoot(story: Stories.Story, onOpenPlan: () -> Unit) {
    val line = story.counts?.let { PlanPage.countsLine(it) }.orEmpty()
    if (line.isNotEmpty()) {
        Text(
            line,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.testTag("story-plan-counts"),
        )
    }
    if (story.pictures.isNotEmpty()) {
        StoryThumbnailStrip(
            count = story.pictures.size,
            current = 0,
            height = 40.dp,
            width = 56.dp,
            limit = 4,
            onPick = { onOpenPlan() },
        ) { index -> StoryThumb(story.pictures[index]) }
    }
    GlassSubmitButton(
        label = PlanPage.Copy.OPEN_PLAN,
        onClick = onOpenPlan,
        modifier = Modifier.testTag(PlanPage.TestIds.OPEN_PLAN),
    )
}

/** A thumbnail: the picture FILLS its box (cropped). */
@Composable
private fun StoryThumb(picture: Stories.Picture) {
    AsyncImage(
        model = "/api/attachments/${picture.attachmentId}",
        contentDescription = picture.caption ?: picture.label,
        contentScale = ContentScale.Crop,
        modifier = Modifier.fillMaxSize(),
    )
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

/** One frame's body, filling the screen on black: a plan's cover (its
 *  first picture fitted, a tap = Open plan), a gallery's current picture
 *  fitted (the "i / n" counter, its caption under it), THE surface with no
 *  card chrome, or nothing (a title frame). [top] / [bottom] = the floating
 *  bands' heights (the scrolling body starts under the top one and clears
 *  the footer). */
@Composable
private fun StoryFrameBody(
    story: Stories.Story,
    frame: Stories.Frame,
    picture: Int,
    row: SessionTree.ThreadRow?,
    top: androidx.compose.ui.unit.Dp,
    bottom: androidx.compose.ui.unit.Dp,
    onOpenPlan: () -> Unit,
) {
    when (frame.kind) {
        Stories.FrameKind.Plan -> {
            val cover = story.pictures.firstOrNull() ?: return
            Box(
                Modifier
                    .fillMaxSize()
                    // A tap on the cover = Open plan (consumed: never steps).
                    .pointerInput(Unit) {
                        awaitEachGesture {
                            val down = awaitFirstDown(requireUnconsumed = false)
                            val up = waitForUpOrCancellation()
                            if (up != null && (up.position - down.position).getDistance() < 10f) {
                                up.consume()
                                onOpenPlan()
                            }
                        }
                    },
            ) { StoryPicture(cover) }
        }
        Stories.FrameKind.Gallery -> {
            val shown = story.pictures.getOrNull(picture) ?: return
            Box(Modifier.fillMaxSize()) {
                StoryPicture(shown)
                Column(Modifier.fillMaxSize().padding(top = top + 8.dp, bottom = bottom + 8.dp)) {
                    if (story.pictures.size > 1) {
                        val pill = RoundedCornerShape(50)
                        Text(
                            "${picture + 1} / ${story.pictures.size}",
                            style = MaterialTheme.typography.labelSmall.copy(fontFeatureSettings = "tnum"),
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier
                                .align(Alignment.End)
                                .padding(end = 16.dp)
                                .background(Color.Black.copy(alpha = 0.55f), pill)
                                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, pill)
                                .padding(horizontal = 7.dp, vertical = 2.dp)
                                .testTag("story-gallery-counter"),
                        )
                    }
                    Box(Modifier.weight(1f))
                    Text(
                        shown.caption?.takeIf { it.isNotEmpty() } ?: shown.label,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("story-gallery-caption"),
                    )
                }
            }
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
        Stories.FrameKind.Title -> Unit
    }
}

/** THE surface card's row for a story's surface (a result's own, or a
 *  plan's / question's inline surface drawn by the same renderer). */
internal fun storySurface(row: SessionTree.ThreadRow?): SessionTree.ThreadRow.Surface? = when (row) {
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
