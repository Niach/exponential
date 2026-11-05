package com.exponential.app.ui.plan

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.DockSheetScope
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.LocalSpecimenPicture
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.LocalInlineCodeStyle
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.LocalMarkdownImageClick
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.markdown.attachmentIdFromUrl
import com.exponential.app.ui.session.LocalAskCardPlain
import com.exponential.app.ui.session.TranscriptBodyStyle
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.thread.CardSurfaceHandle
import com.exponential.app.ui.thread.CardSurfaceView
import com.exponential.app.ui.thread.PlanAskCard
import com.exponential.app.ui.thread.ThreadPlanDecision
import com.exponential.app.ui.work.PictureViewer
import com.exponential.app.ui.work.PictureViewerItem
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/** What THE plan tab draws: the plan card as THE thread row the questions
 *  answer, [card] raw (its probed picture sizes). */
data class PlanPageState(
    val row: SessionTree.ThreadRow.Plan,
    val card: JsonObject? = null,
)

/**
 * #EXP-1493 → #EXP-1662 THE PLAN TAB, phone form (fixtures/plan-page.json; web
 * packages/ui plan-page.tsx, iOS PlanPageScreen.swift): the BODY of a thread's
 * plan tab, under the thread's own header + strip, on its ground; no own header,
 * no composer. ONE scroll: the blocks ([PlanPage.blocks]): the big title, prose
 * through THE chat markdown renderer, a section = its h2 (the outline's anchor),
 * a picture group = THE carousel. A press on a picture opens THE lightbox
 * (`‹ i / n ›`) over every plan picture in document order. The `ui-menu` ghost
 * button at the document's top right opens the outline in a bottom sheet (THE
 * eUI Drawer's phone form). THE questions float over the document's foot
 * ([PlanQuestionsPanel]).
 */
@Composable
fun PlanPageContent(
    state: PlanPageState,
    onDecidePlan: ThreadPlanDecision,
    modifier: Modifier = Modifier,
    /** The header band over the body / the bar under it. */
    topInset: Dp = 0.dp,
    bottomInset: Dp = 0.dp,
    /** The specimen's still: this outline row active. */
    activeSection: String? = null,
) {
    val row = state.row
    val plan = row.plan
    val blocks = remember(plan) { PlanPage.blocks(plan) }
    val outline = remember(plan) { PlanPage.outline(plan) }
    val pictures = remember(plan) { PlanPage.pictures(plan) }
    val hasOutline = outline.isNotEmpty() || row.questions.isNotEmpty()
    var outlineOpen by remember { mutableStateOf(false) }
    var lightbox by remember { mutableStateOf<Int?>(null) }
    var panelHeightPx by remember { mutableIntStateOf(0) }
    val panelHeight = with(LocalDensity.current) { panelHeightPx.toDp() }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val gap = PlanPage.Panel.GAP.dp
    val openPicture: (String) -> Unit = { src ->
        lightbox = pictures.indexOfFirst { it.src == src }.coerceAtLeast(0)
    }

    BoxWithConstraints(
        modifier
            .fillMaxSize()
            // The bar's inset is the panel's own; the keyboard lifts it.
            .consumeWindowInsets(PaddingValues(bottom = bottomInset))
            .imePadding()
            .testTag(PlanPage.TestIds.PAGE),
    ) {
        PlanDocument(
            state = state,
            blocks = blocks,
            hasOutline = hasOutline,
            listState = listState,
            topInset = topInset,
            // The document scrolls clear of the floating panel.
            bottomPadding = panelHeight + gap * 2 + bottomInset,
            onOpenPicture = openPicture,
        )
        if (hasOutline) {
            CircleIconButton(
                ExpIcons.uiMenu,
                PlanPage.Copy.OUTLINE,
                onClick = { outlineOpen = true },
                borderless = true,
                // Web: the ghost icon-sm button's 16 glyph (never a circle).
                glyphSize = 16.dp,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(top = topInset + 8.dp, end = 8.dp)
                    .testTag(PlanPage.TestIds.OUTLINE_TOGGLE),
            )
        }
        PlanQuestionsPanel(
            row = row,
            onDecidePlan = onDecidePlan,
            maxHeight = (maxHeight - topInset) * PlanPage.Panel.MAX_HEIGHT_FRACTION,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(start = gap, end = gap, bottom = gap + bottomInset)
                .onSizeChanged { if (it.height != panelHeightPx) panelHeightPx = it.height },
        )
    }
    if (outlineOpen) {
        // Scroll-spy: the last section whose heading passed the top.
        val active = activeSection ?: activeOutlineRow(blocks, listState)
        GlassSheet(title = null, onDismiss = { outlineOpen = false }) {
            PlanOutlineList(
                sections = outline,
                active = active,
                questions = row.questions.size,
                onSelect = { id ->
                    outlineOpen = false
                    val index = blocks.indexOfFirst { it is PlanPage.Block.Section && it.id == id }
                    if (index >= 0) scope.launch { listState.animateScrollToItem(index) }
                },
                modifier = Modifier.padding(start = 10.dp, end = 10.dp, bottom = 12.dp),
            )
        }
    }
    lightbox?.let { start ->
        PlanLightbox(pictures = pictures, start = start, onDismiss = { lightbox = null })
    }
    BackHandler(enabled = lightbox != null) { lightbox = null }
}

/**
 * THE questions (plan-page.json `panel`): the menu surface fill + hairline +
 * `panel.radius`, hugging its content up to [maxHeight], scrolling inside past
 * that, the footer pinned. Inside = plan-card `stepper`: the step label top right
 * (more than one question), one question at a time, Back / Next bottom right, the
 * decision on the last step, the feedback field after the reject pick; a plan
 * without questions = the decision alone; decided or superseded = the closed card.
 */
@Composable
private fun PlanQuestionsPanel(
    row: SessionTree.ThreadRow.Plan,
    onDecidePlan: ThreadPlanDecision,
    maxHeight: Dp,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(PlanPage.Panel.RADIUS.dp)
    val steps = remember(row.id) { DockSheetScope() }
    Column(
        modifier
            .widthIn(max = PlanPage.Panel.MAX_WIDTH.dp)
            .fillMaxWidth()
            .heightIn(max = maxHeight)
            .shadow(12.dp, shape)
            .clip(shape)
            .background(GlassTokens.OpaqueCardFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
            // The document under it never takes a press on the panel.
            .pointerInput(Unit) {}
            .padding(PlanPage.Panel.PADDING.dp)
            .testTag(PlanPage.TestIds.RAIL),
    ) {
        CompositionLocalProvider(LocalAskCardPlain provides true) {
            if (row.decision == null) {
                steps.meta?.let { meta ->
                    Text(
                        meta,
                        style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Normal),
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        maxLines = 1,
                        modifier = Modifier.align(Alignment.End).padding(bottom = 12.dp).testTag("plan-card-step"),
                    )
                }
                PlanAskCard(
                    row = row,
                    onAnswer = { _, _, settled -> settled(false) },
                    onDecidePlan = onDecidePlan,
                    sheet = steps,
                    questionsOnly = true,
                    panel = true,
                )
            } else {
                Column(Modifier.weight(1f, fill = false).fillMaxWidth().verticalScroll(rememberScrollState())) {
                    PlanAskCard(
                        row = row,
                        onAnswer = { _, _, settled -> settled(false) },
                        onDecidePlan = onDecidePlan,
                        questionsOnly = true,
                    )
                }
            }
        }
    }
}

/** The document: the blocks, ONE scroll. */
@Composable
private fun PlanDocument(
    state: PlanPageState,
    blocks: List<PlanPage.Block>,
    hasOutline: Boolean,
    listState: LazyListState,
    topInset: Dp,
    bottomPadding: Dp,
    onOpenPicture: (String) -> Unit,
) {
    val padding = PlanPage.Layout.COLUMN_PADDING_PHONE.dp
    val surface = state.row.surface
    CompositionLocalProvider(
        LocalMarkdownBodyStyle provides TranscriptBodyStyle,
        LocalInlineCodeStyle provides MdStyle.Chat,
        LocalMarkdownImageClick provides onOpenPicture,
    ) {
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize().detailHazeSource().testTag(PlanPage.TestIds.DOCUMENT),
            contentPadding = PaddingValues(start = padding, end = padding, top = topInset + padding, bottom = bottomPadding),
            verticalArrangement = Arrangement.spacedBy(PlanPage.Layout.BLOCK_GAP.dp),
        ) {
            blocks.forEachIndexed { index, block ->
                item(key = (block as? PlanPage.Block.Section)?.id ?: "block-$index") {
                    Box(Modifier.widthIn(max = PlanPage.Layout.COLUMN_MAX_WIDTH.dp)) {
                        when (block) {
                            is PlanPage.Block.Title -> Text(
                                block.text,
                                style = MaterialTheme.typography.titleLarge.copy(
                                    fontSize = PlanPage.Layout.TITLE_SIZE.sp,
                                    lineHeight = (PlanPage.Layout.TITLE_SIZE * 1.25f).sp,
                                    fontWeight = FontWeight.SemiBold,
                                ),
                                color = MaterialTheme.colorScheme.onSurface,
                                // Clear of the outline's ghost button.
                                modifier = Modifier.padding(end = if (hasOutline) 32.dp else 0.dp),
                            )
                            is PlanPage.Block.Markdown -> MarkdownView(block.text)
                            is PlanPage.Block.Section -> MarkdownView("## ${block.title}")
                            is PlanPage.Block.Pictures -> PlanPictureCarousel(
                                pictures = block.pictures,
                                aspect = groupAspect(block.pictures.firstOrNull(), state.card),
                                onOpen = { onOpenPicture(block.pictures[it].src) },
                            )
                        }
                    }
                }
            }
            if (surface != null) {
                item(key = "surface") {
                    val handle = remember { CardSurfaceHandle() }
                    CardSurfaceView(surface, handle, inert = true)
                }
            }
        }
    }
}

/** Scroll-spy: the last section whose heading passed the top; none yet = null. */
@Composable
private fun activeOutlineRow(blocks: List<PlanPage.Block>, listState: LazyListState): String? {
    val active by remember(blocks) {
        derivedStateOf {
            val top = listState.firstVisibleItemIndex
            blocks.withIndex()
                .lastOrNull { (at, block) -> block is PlanPage.Block.Section && at <= top }
                ?.let { (it.value as PlanPage.Block.Section).id }
        }
    }
    return active
}

/** The group's box = the first picture's probed aspect (unknown = 16:10). */
private fun groupAspect(picture: PlanPage.Picture?, card: JsonObject?): Float {
    val id = picture?.src?.substringBefore('?')?.substringAfterLast('/') ?: return 1.6f
    val entry = (card?.get("pictures") as? JsonArray).orEmpty().firstOrNull {
        ((it as? JsonObject)?.get("attachmentId") as? JsonPrimitive)?.content == id
    } as? JsonObject ?: return 1.6f
    val width = (entry["width"] as? JsonPrimitive)?.doubleOrNull
    val height = (entry["height"] as? JsonPrimitive)?.doubleOrNull
    if (width == null || height == null || width <= 0 || height <= 0) return 1.6f
    return (width / height).toFloat().coerceIn(0.5f, 2.4f)
}

/** One plan picture fitted (contain) on black; the muted fill while it loads
 *  (a styleguide specimen paints its sample, [LocalSpecimenPicture]). */
@Composable
internal fun PlanPictureImage(src: String, modifier: Modifier = Modifier, contentScale: ContentScale = ContentScale.Fit) {
    var loaded by remember(src) { mutableStateOf(false) }
    Box(modifier.background(if (loaded) Color.Black else DesignTokens.Palette.Muted)) {
        AsyncImage(
            model = LocalSpecimenPicture.current?.invoke(src) ?: src,
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
 * background + the leading bar), then a separator and the questions row,
 * which scrolls nothing (the panel already shows them).
 */
@Composable
fun PlanOutlineList(
    sections: List<PlanPage.Section>,
    active: String?,
    questions: Int,
    onSelect: (String) -> Unit,
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
                    .padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(PlanPage.Copy.QUESTIONS, style = MaterialTheme.typography.bodyMedium, color = secondary)
                Text(" · $questions", style = MaterialTheme.typography.bodyMedium, color = tertiary)
            }
        }
    }
}

/** THE lightbox: every plan picture in document order in THE picture
 *  viewer (EXP-1586): zoomable, `‹ i / n ›`; the close glyph or Back closes. */
@Composable
private fun PlanLightbox(pictures: List<PlanPage.Picture>, start: Int, onDismiss: () -> Unit) {
    val items = remember(pictures) {
        pictures.map { PictureViewerItem(id = attachmentIdFromUrl(it.src) ?: it.src, src = it.src, label = it.alt) }
    }
    PictureViewer(items = items, start = start, onClose = onDismiss)
}
