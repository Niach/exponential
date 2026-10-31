package com.exponential.app.ui.plan

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.Dock
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.DockCard
import com.exponential.app.ui.components.DockSheet
import com.exponential.app.ui.components.DockSheetBody
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.StoryKindBadge
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.LocalInlineCodeStyle
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.LocalMarkdownImageClick
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.session.LocalAskCardPlain
import com.exponential.app.ui.session.TranscriptBodyStyle
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.PlanAskCard
import com.exponential.app.ui.thread.ThreadPlanDecision
import com.exponential.app.ui.work.ZoomableBox
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/**
 * #EXP-1493 THE PLAN PAGE, phone form (fixtures/plan-page.json; web
 * packages/ui plan-page.tsx, iOS PlanPageScreen.swift): THE plan card's
 * expanded form, a pushed page. ONE scroll under the page header (back,
 * `copy.title`, the `ui-menu` outline toggle when the outline has a row):
 * the kind badge + the h1 title + the counts line lead the column, then the
 * blocks ([PlanPage.blocks]): prose through THE chat markdown renderer, a
 * section = its h2 (the outline's anchor), a picture group = THE carousel.
 * A press on a picture opens THE lightbox (`‹ i / n ›`) over every plan
 * picture in document order. The outline opens in a bottom sheet (THE eUI
 * Drawer's phone form). The bottom = THE DockCard (dock.json, kind plan)
 * opening THE Dock sheet QUESTIONS-ONLY with THE decision (decided = the
 * closed card); no composer.
 */
@Composable
fun PlanPageScreen(
    onBack: () -> Unit,
    @Suppress("UNUSED_PARAMETER") onOpenSession: (String) -> Unit,
    viewModel: PlanPageViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    PlanPageContent(state = state, onBack = onBack, onDecidePlan = viewModel::decidePlan)
}

/** The page over its [state] (the live screen and the styleguide's specimen). */
@Composable
fun PlanPageContent(
    state: PlanPageState,
    onBack: () -> Unit,
    onDecidePlan: ThreadPlanDecision,
    modifier: Modifier = Modifier,
    /** The specimen's chat name (`specimen.chatName`) over the synced one. */
    chatName: String? = null,
    /** The specimen's still: this outline row active. */
    activeSection: String? = null,
) {
    val card = state.card
    val plan = (card?.get("plan") as? JsonPrimitive)?.takeIf { it.isString }?.content.orEmpty()
    val outline = remember(plan) { PlanPage.outline(plan) }
    var outlineOpen by remember { mutableStateOf(false) }
    var dockOpen by remember { mutableStateOf(false) }
    var lightbox by remember { mutableStateOf<Int?>(null) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()

    Box(modifier.fillMaxSize().testTag(PlanPage.TestIds.PAGE)) {
        Column(Modifier.fillMaxSize()) {
            PlanPageHeader(
                onBack = onBack,
                onOutline = if (outline.isNotEmpty()) ({ outlineOpen = true }) else null,
            )
            when {
                card != null && state.row != null -> Box(Modifier.weight(1f)) {
                    val blocks = remember(plan) { leadTrimmed(PlanPage.blocks(plan)) }
                    val pictures = remember(plan) { PlanPage.pictures(plan) }
                    val openPicture: (String) -> Unit = { src ->
                        lightbox = pictures.indexOfFirst { it.src == src }.coerceAtLeast(0)
                    }
                    PlanDocument(
                        plan = plan,
                        card = card,
                        blocks = blocks,
                        decided = state.row.decision != null,
                        listState = listState,
                        onOpenPicture = openPicture,
                    )
                    // THE DockCard (dock.json's plan card UNCHANGED) floating over the foot.
                    val dockRow = planDockRow(state, chatName)
                    DockCard(
                        row = dockRow,
                        onOpen = { dockOpen = true },
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .navigationBarsPadding()
                            .padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                    if (dockOpen) {
                        DockSheet(row = dockRow, onDismiss = { dockOpen = false }) { sheetScope ->
                            CompositionLocalProvider(LocalAskCardPlain provides true) {
                                if (state.row.decision == null) {
                                    PlanAskCard(
                                        row = state.row,
                                        onAnswer = { _, _, settled -> settled(false) },
                                        onDecidePlan = onDecidePlan,
                                        sheet = sheetScope,
                                        questionsOnly = true,
                                    )
                                } else {
                                    // Decided: the same sheet shows the closed card (no
                                    // "Question n of m" left over from the stepper).
                                    androidx.compose.runtime.SideEffect { sheetScope.meta = null }
                                    DockSheetBody {
                                        PlanAskCard(
                                            row = state.row,
                                            onAnswer = { _, _, settled -> settled(false) },
                                            onDecidePlan = onDecidePlan,
                                            questionsOnly = true,
                                        )
                                    }
                                }
                            }
                        }
                    }
                    lightbox?.let { start ->
                        PlanLightbox(pictures = pictures, start = start, onDismiss = { lightbox = null })
                    }
                }
                state.loading -> SkeletonRows(modifier = Modifier.padding(16.dp))
                else -> EmptyState(
                    title = "Not found",
                    description = "It may still be syncing, or it was deleted.",
                    icon = ExpIcons.codingPlan,
                )
            }
        }
        if (outlineOpen) {
            // Scroll-spy: the last section whose heading passed the top.
            val active = activeSection ?: activeOutlineRow(plan, listState)
            GlassSheet(title = null, onDismiss = { outlineOpen = false }) {
                PlanOutlineList(
                    sections = outline,
                    active = active,
                    questions = (card?.get("questions") as? JsonArray)?.size ?: 0,
                    onSelect = { id ->
                        outlineOpen = false
                        val index = sectionItemIndex(plan, id)
                        if (index >= 0) scope.launch { listState.animateScrollToItem(index) }
                    },
                    onQuestions = {
                        outlineOpen = false
                        dockOpen = true
                    },
                    modifier = Modifier.padding(start = 10.dp, end = 10.dp, bottom = 12.dp),
                )
            }
        }
    }
    BackHandler(enabled = lightbox != null) { lightbox = null }
}

/** The page header: the back chevron, `copy.title`, the outline toggle. */
@Composable
private fun PlanPageHeader(onBack: () -> Unit, onOutline: (() -> Unit)?) {
    Box(
        Modifier
            .fillMaxWidth()
            .statusBarsPadding()
            .height(PlanPage.Layout.HEADER_HEIGHT.dp + 8.dp)
            .padding(horizontal = 8.dp),
    ) {
        Box(Modifier.align(Alignment.CenterStart)) { TopBarBackButton(onClick = onBack) }
        TopBarTitle(PlanPage.Copy.TITLE, modifier = Modifier.align(Alignment.Center))
        if (onOutline != null) {
            CircleIconButton(
                ExpIcons.uiMenu,
                PlanPage.Copy.OUTLINE,
                onClick = onOutline,
                modifier = Modifier.align(Alignment.CenterEnd).testTag(PlanPage.TestIds.OUTLINE_TOGGLE),
            )
        }
    }
}

/** dock.json's plan DockCard UNCHANGED (`Plan ready`, the asking session's
 *  name, `n questions`, the plan's title). */
private fun planDockRow(state: PlanPageState, chatName: String?): Dock.Row {
    val row = state.row!!
    return Dock.dockRows(
        listOf(
            Dock.Source.Ask(
                id = state.messageId,
                kind = AskDock.Kind.Plan,
                text = row.plan,
                sessionName = chatName ?: state.sessionName,
                questions = row.questions.size,
                createdAt = 0L,
            ),
        ),
    ).first()
}

/** The document: the lead (badge, h1 title, counts) then the blocks, ONE scroll. */
@Composable
private fun PlanDocument(
    plan: String,
    card: JsonObject,
    blocks: List<PlanPage.Block>,
    decided: Boolean,
    listState: LazyListState,
    onOpenPicture: (String) -> Unit,
) {
    val padding = PlanPage.Layout.COLUMN_PADDING_PHONE.dp
    val counts = PlanPage.countsLine(PlanPage.counts(card))
    val title = AskDock.askDockTitle(plan).ifEmpty { PlanPage.Copy.TITLE }
    CompositionLocalProvider(
        LocalMarkdownBodyStyle provides TranscriptBodyStyle,
        LocalInlineCodeStyle provides MdStyle.Chat,
        LocalMarkdownImageClick provides onOpenPicture,
    ) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize().testTag(PlanPage.TestIds.DOCUMENT),
            contentPadding = PaddingValues(start = padding, end = padding, top = padding, bottom = 120.dp),
            verticalArrangement = Arrangement.spacedBy(PlanPage.Layout.BLOCK_GAP.dp),
        ) {
            item(key = "lead") {
                Column(
                    Modifier.widthIn(max = PlanPage.Layout.COLUMN_MAX_WIDTH.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    StoryKindBadge(kind = Stories.Kind.Plan, waiting = !decided)
                    Text(
                        title,
                        style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.SemiBold),
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    if (counts.isNotEmpty()) {
                        Text(
                            counts,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
            }
            blocks.forEachIndexed { index, block ->
                item(key = (block as? PlanPage.Block.Section)?.id ?: "block-$index") {
                    Box(Modifier.widthIn(max = PlanPage.Layout.COLUMN_MAX_WIDTH.dp)) {
                        when (block) {
                            is PlanPage.Block.Markdown -> MarkdownView(block.text)
                            is PlanPage.Block.Section -> MarkdownView("## ${block.title}")
                            is PlanPage.Block.Pictures -> PlanPictureCarousel(
                                pictures = block.pictures,
                                aspect = groupAspect(block.pictures.firstOrNull(), card),
                                onOpen = { onOpenPicture(block.pictures[it].src) },
                            )
                        }
                    }
                }
            }
        }
    }
}

/** The h1 the page already leads with ([AskDock.askDockTitle]) is not said twice. */
private fun leadTrimmed(blocks: List<PlanPage.Block>): List<PlanPage.Block> {
    val first = blocks.firstOrNull() as? PlanPage.Block.Markdown ?: return blocks
    val lines = first.text.split("\n")
    if (!lines.first().startsWith("# ")) return blocks
    val rest = lines.drop(1).joinToString("\n").trim()
    return (if (rest.isEmpty()) emptyList() else listOf(PlanPage.Block.Markdown(rest))) + blocks.drop(1)
}

/** A section's LazyColumn item index (the lead is item 0). */
private fun sectionItemIndex(plan: String, id: String): Int {
    val at = leadTrimmed(PlanPage.blocks(plan)).indexOfFirst { it is PlanPage.Block.Section && it.id == id }
    return if (at < 0) -1 else at + 1
}

/** Scroll-spy: the last section whose heading passed the top; none yet = null. */
@Composable
private fun activeOutlineRow(plan: String, listState: LazyListState): String? {
    val active by remember(plan) {
        derivedStateOf {
            val blocks = leadTrimmed(PlanPage.blocks(plan))
            val top = listState.firstVisibleItemIndex
            blocks.withIndex()
                .lastOrNull { (at, block) -> block is PlanPage.Block.Section && at + 1 <= top }
                ?.let { (it.value as PlanPage.Block.Section).id }
        }
    }
    return active
}

/** The group's box = the first picture's probed aspect (unknown = 16:10). */
private fun groupAspect(picture: PlanPage.Picture?, card: JsonObject): Float {
    val id = picture?.src?.substringBefore('?')?.substringAfterLast('/') ?: return 1.6f
    val entry = (card["pictures"] as? JsonArray).orEmpty().firstOrNull {
        ((it as? JsonObject)?.get("attachmentId") as? JsonPrimitive)?.content == id
    } as? JsonObject ?: return 1.6f
    val width = (entry["width"] as? JsonPrimitive)?.doubleOrNull
    val height = (entry["height"] as? JsonPrimitive)?.doubleOrNull
    if (width == null || height == null || width <= 0 || height <= 0) return 1.6f
    return (width / height).toFloat().coerceIn(0.5f, 2.4f)
}

/** One plan picture fitted (contain) on black; the muted fill while it loads
 *  (and on the specimen's placeholders). */
@Composable
internal fun PlanPictureImage(src: String, modifier: Modifier = Modifier, contentScale: ContentScale = ContentScale.Fit) {
    var loaded by remember(src) { mutableStateOf(false) }
    Box(modifier.background(if (loaded) Color.Black else DesignTokens.Palette.Muted)) {
        AsyncImage(
            model = src,
            contentDescription = null,
            contentScale = contentScale,
            onSuccess = { loaded = true },
            modifier = Modifier.fillMaxSize(),
        )
    }
}

/**
 * THE phone picture group (eUI Carousel): one slide per picture at the
 * group's aspect, the "i / n" counter top-left, the dots under it, swipe. A
 * tap on a slide = [onOpen] (the lightbox).
 */
@Composable
fun PlanPictureCarousel(pictures: List<PlanPage.Picture>, aspect: Float, onOpen: (Int) -> Unit, modifier: Modifier = Modifier) {
    val pager = rememberPagerState { pictures.size }
    val radius = PlanPage.Mosaic.RADIUS.dp
    val shape = RoundedCornerShape(radius)
    Column(
        modifier.fillMaxWidth().testTag(PlanPage.TestIds.CAROUSEL),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(Modifier.fillMaxWidth().aspectRatio(aspect)) {
            HorizontalPager(state = pager, pageSpacing = 8.dp, modifier = Modifier.fillMaxSize()) { index ->
                PlanPictureImage(
                    src = pictures[index].src,
                    modifier = Modifier
                        .fillMaxSize()
                        .clip(shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                        .clickable { onOpen(index) },
                )
            }
            Text(
                "${pager.currentPage + 1} / ${pictures.size}",
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.SemiBold, fontSize = 11.sp),
                color = Color.White,
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(8.dp)
                    .clip(CircleShape)
                    .background(Color.Black.copy(alpha = 0.55f))
                    .padding(horizontal = 7.dp, vertical = 2.dp)
                    .testTag("plan-carousel-counter"),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            repeat(pictures.size) { index ->
                Box(
                    Modifier
                        .size(5.dp)
                        .clip(CircleShape)
                        .background(if (index == pager.currentPage) Color.White else Color.White.copy(alpha = 0.3f)),
                )
            }
        }
    }
}

/**
 * THE outline (eUI `Outline`): the caption, one row per section (the title
 * one line, a muted `· n` with pictures; the active row = the accent
 * background + the leading bar), then a separator and the questions row.
 */
@Composable
fun PlanOutlineList(
    sections: List<PlanPage.Section>,
    active: String?,
    questions: Int,
    onSelect: (String) -> Unit,
    onQuestions: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val rowHeight = PlanPage.Layout.OUTLINE_ROW_HEIGHT.dp
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val rowShape = RoundedCornerShape(GlassTokens.RowRadius)
    Column(modifier.fillMaxWidth().testTag(PlanPage.TestIds.OUTLINE), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            PlanPage.Copy.OUTLINE.uppercase(),
            style = MaterialTheme.typography.labelSmall.copy(fontSize = 10.sp, fontWeight = FontWeight.Medium, letterSpacing = 0.6.sp),
            color = tertiary,
            modifier = Modifier.padding(start = 10.dp, end = 10.dp, bottom = 6.dp),
        )
        sections.forEach { section ->
            val on = section.id == active
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(rowHeight)
                    .clip(rowShape)
                    .background(if (on) GlassTokens.RowFillActive else Color.Transparent)
                    .clickable { onSelect(section.id) }
                    .testTag(PlanPage.TestIds.OUTLINE_ROW),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier
                        .width(PlanPage.Layout.OUTLINE_ACTIVE_BAR.dp)
                        .height(rowHeight - 12.dp)
                        .background(if (on) MaterialTheme.colorScheme.onSurface else Color.Transparent),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    section.title,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (on) MaterialTheme.colorScheme.onSurface else secondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (section.pictures > 0) {
                    Text(" · ${section.pictures}", style = MaterialTheme.typography.bodyMedium, color = tertiary, maxLines = 1)
                }
                Spacer(Modifier.width(10.dp))
            }
        }
        if (questions > 0) {
            HorizontalDivider(
                thickness = GlassTokens.Hairline,
                color = GlassTokens.StrokeCard,
                modifier = Modifier.padding(vertical = 6.dp),
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(rowHeight)
                    .clip(rowShape)
                    .clickable(onClick = onQuestions)
                    .padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(PlanPage.Copy.QUESTIONS, style = MaterialTheme.typography.bodyMedium, color = secondary)
                Text(" · $questions", style = MaterialTheme.typography.bodyMedium, color = tertiary)
            }
        }
    }
}

/** THE lightbox: every plan picture in document order, fitted on black,
 *  pinch-zoom, `‹ i / n ›`; a tap on the scrim, the close glyph or Back closes. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlanLightbox(pictures: List<PlanPage.Picture>, start: Int, onDismiss: () -> Unit) {
    if (pictures.isEmpty()) return
    val pager = rememberPagerState(initialPage = start.coerceIn(0, pictures.lastIndex)) { pictures.size }
    val scope = rememberCoroutineScope()
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.92f)).testTag("plan-lightbox")) {
            HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { index ->
                ZoomableBox(modifier = Modifier.fillMaxSize(), contentAspect = null, onTap = onDismiss) {
                    AsyncImage(
                        model = pictures[index].src,
                        contentDescription = pictures[index].alt,
                        contentScale = ContentScale.Fit,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            }
            CircleIconButton(
                ExpIcons.uiClose,
                "Close",
                onClick = onDismiss,
                modifier = Modifier.align(Alignment.TopStart).statusBarsPadding().padding(8.dp),
            )
            Row(
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CircleIconButton(
                    ExpIcons.uiChevronLeft,
                    "Previous",
                    enabled = pager.currentPage > 0,
                    onClick = { scope.launch { pager.animateScrollToPage(pager.currentPage - 1) } },
                )
                Text(
                    "${pager.currentPage + 1} / ${pictures.size}",
                    style = MaterialTheme.typography.labelMedium,
                    color = Color.White.copy(alpha = 0.9f),
                )
                CircleIconButton(
                    ExpIcons.uiChevronRight,
                    "Next",
                    enabled = pager.currentPage < pictures.lastIndex,
                    onClick = { scope.launch { pager.animateScrollToPage(pager.currentPage + 1) } },
                )
            }
        }
    }
}
