package com.exponential.app.ui.work

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.ui.platform.LocalLayoutDirection
import com.exponential.app.ui.components.detailHazeSource
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import coil3.compose.AsyncImage
import coil3.request.ImageRequest
import coil3.size.Size
import com.exponential.app.domain.Diff
import com.exponential.app.domain.GUIDE_CHANGES_TOPIC
import com.exponential.app.domain.GuideChangeSet
import com.exponential.app.domain.GuideSectionKey
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.PrStack
import com.exponential.app.domain.guideCoverage
import com.exponential.app.domain.guideFileCountLabel
import com.exponential.app.ui.components.StackRail
import com.exponential.app.ui.components.StackRailMember
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.guideSectionCaption
import com.exponential.app.domain.prDescriptionGroup
import com.exponential.app.domain.sessionResultsGuide
import com.exponential.app.domain.SessionResultGroup
import com.exponential.app.domain.SESSION_RESULTS_EARLIER_LABEL
import com.exponential.app.domain.sessionResultIsTall
import com.exponential.app.domain.sessionResultTileHeightFitting
import com.exponential.app.domain.sessionResultTileWidth
import com.exponential.app.domain.tallImageStripRanges
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBarCluster
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.issue.DiffCounts
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1251: the Work screen's GUIDE FACE (web `guide-face.tsx`, phones) —
// Changes + Results merged: [the Stack card when the PR sits in an open
// stack of 2+: band `Stack` (the `pr-stack` glyph, NO count) over the shared
// [StackRail], the current member washed, a tap swaps the subject in place, a
// long-press offers Merge through here] [the `Summary` lead] [numbered
// section bands: text, ONE `Changes · N files +A −D ›` row (the files the
// topic names, by path or rename source — `guideCoverage`), tiles, Earlier]
// [the automatic `Other changes` band] [the hairline `Show complete diff`
// row]. A Changes row opens its section page ([GuideSectionDiff]). An open PR
// with no run report leads with the PR's GitHub body; no report at all = one
// `Changes` section over the whole diff. The floating bar carries the white
// Merge capsule alone (`Merge stack` while stacked).

/** The horizontal content padding the page reserves on each side. */
private val HorizontalPadding = 16.dp
private val TileShape = RoundedCornerShape(10.dp)

/** EXP-1154 (web `prDescriptionGroups`): a LOADED PR body as ONE Guide group
 *  claiming every diff path ([prDescriptionGroup]); empty until loaded. */
fun prDescriptionGroups(state: PrDescriptionState?, files: List<Diff.File>?): List<SessionResultGroup> =
    when (state) {
        is PrDescriptionState.Loaded -> listOf(
            prDescriptionGroup(state.description.title, state.description.body, files),
        )
        else -> emptyList()
    }

/** EXP-1251: the Stack card's band title (web `STACK_CARD_TITLE`). */
internal const val STACK_CARD_TITLE = "Stack"

/** The Stack card's input: the rail plus what a member's tap / long-press does. */
class GuideStack(
    val view: PrStack.StackView,
    /** The issue's board default branch: the base row's fallback. */
    val boardDefaultBranch: String?,
    /** Swap the screen's subject to that member's issue, in place. */
    val onOpen: (issueId: String) -> Unit,
    /** Merge through that member (its confirm first); null = not a member. */
    val onMergeThrough: ((issueId: String) -> Unit)?,
)

@Composable
fun GuideFace(
    padding: PaddingValues,
    groups: List<SessionResultGroup>,
    /** The diff the Guide counts (the run's live diff, else the PR / branch
     *  files); null while it is not loaded: no Changes rows yet. */
    files: List<Diff.File>?,
    /** EXP-1154: the open PR's GitHub body fetch, drawn (its spinner or its
     *  refusal) only while [groups] is empty; once loaded the caller passes it
     *  IN [groups] ([prDescriptionGroups]). */
    prFallback: PrDescriptionState? = null,
    /** False for the PR-body group (one band, no `01 / 01`), web `numbered`. */
    numbered: Boolean = true,
    /** EXP-1251: the Stack card; null = not in an open stack. */
    stack: GuideStack? = null,
    /** The bar's white Merge; null = no bar. */
    merge: ChangesMergeControl? = null,
    /** A Changes row's tap (its section page); null = inert rows. */
    onOpenSection: ((GuideSectionKey) -> Unit)? = null,
) {
    var preview by remember { mutableStateOf<SessionResultEntry?>(null) }
    val coverage = remember(groups, files) { guideCoverage(groups, files) }

    // EXP-1162: the page runs under the header band — the host's insets are
    // the list's CONTENT padding, not the face's.
    val layoutDirection = LocalLayoutDirection.current
    val bottomInset = padding.calculateBottomPadding()
    BoxWithConstraints(
        modifier = Modifier
            .padding(
                start = padding.calculateStartPadding(layoutDirection),
                end = padding.calculateEndPadding(layoutDirection),
            )
            .fillMaxSize(),
    ) {
        // ONE factor for the whole page (shared rule x4).
        val availableDp = (maxWidth - HorizontalPadding * 2).value.toInt()
        val tileHeight = remember(groups, availableDp) {
            sessionResultTileHeightFitting(groups.flatMap { it.entries + it.earlier }, availableDp)
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize().detailHazeSource().testTag("work-guide"),
            contentPadding = PaddingValues(
                start = HorizontalPadding,
                end = HorizontalPadding,
                top = padding.calculateTopPadding() + 4.dp,
                bottom = BottomBarInset + bottomInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            stack?.let { card ->
                item(key = "__stack__") { GuideStackCard(card) }
            }
            // Web M8 ×4: a loaded PR body arrives as a group ([prDescriptionGroups])
            // that claims EVERY diff path; only its spinner / refusal draws here.
            if (groups.isEmpty() && prFallback != null && prFallback !is PrDescriptionState.Loaded) {
                item(key = "__pr_fallback__") { PrFallbackSection(state = prFallback) }
            }
            coverage.lead?.let { lead ->
                item(key = "__lead__") {
                    Column(
                        modifier = Modifier.testTag("guide-lead"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        GuideGroupBody(
                            group = lead.group,
                            changes = lead.changes,
                            onOpenChanges = onOpenSection?.let { open -> { open(GuideSectionKey.Lead) } },
                            tileHeight = tileHeight,
                            onPreview = { preview = it },
                        )
                    }
                }
            }
            // Indexed keys: one topic may appear more than once.
            coverage.sections.forEach { section ->
                val group = section.group
                item(key = "topic_${section.index}_${group.topic}") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (numbered) {
                            SectionHeader(
                                title = group.topic,
                                leading = { GuideCaption(guideSectionCaption(section.index, section.total)) },
                            )
                        } else {
                            SectionHeader(title = group.topic)
                        }
                        GuideGroupBody(
                            group = group,
                            changes = section.changes,
                            onOpenChanges = onOpenSection?.let { open ->
                                { open(GuideSectionKey.Numbered(section.index)) }
                            },
                            tileHeight = tileHeight,
                            onPreview = { preview = it },
                        )
                    }
                }
            }
            coverage.other?.let { other ->
                item(key = "__other__") {
                    Column(
                        modifier = Modifier.testTag("guide-other-changes"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        SectionHeader(title = other.topic)
                        GuideChangesRow(
                            changes = other.changes,
                            onOpen = onOpenSection?.let { open -> { open(GuideSectionKey.Other) } },
                        )
                    }
                }
            }
            // The whole diff, last; with no report the one `Changes` section
            // above already is it (desktop parity).
            val complete = coverage.complete
            if (complete != null && complete.fileCount > 0 && groups.isNotEmpty()) {
                item(key = "__complete__") {
                    GuideShowCompleteDiffRow(
                        changes = complete,
                        onOpen = onOpenSection?.let { open -> { open(GuideSectionKey.All) } },
                    )
                }
            }
        }
        if (merge != null) {
            FloatingBarEdge(
                bottomInset = bottomInset,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .fillMaxWidth()
                    .padding(bottom = bottomInset),
            ) {
                FloatingBarCluster(centre = { MergeCapsule(merge) })
            }
        }
    }

    preview?.let { entry ->
        ResultPreviewDialog(entry = entry, onDismiss = { preview = null })
    }
}

/** EXP-1251: the Stack card — band `Stack` (no count) over the rail, top first. */
@Composable
private fun GuideStackCard(stack: GuideStack) {
    val members = remember(stack.view) {
        stack.view.rows.map { row ->
            StackRailMember(key = row.issueId, identifier = row.identifier, title = row.title, current = row.isCurrent)
        }
    }
    Column(modifier = Modifier.testTag("guide-stack-card")) {
        SectionHeader(
            title = STACK_CARD_TITLE,
            leading = {
                Icon(
                    ExpIcons.prStack,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            },
        )
        StackRail(
            members = members,
            baseBranch = PrStack.baseRowLabel(stack.view.baseBranch, stack.boardDefaultBranch),
            onOpen = { member -> if (!member.current) stack.onOpen(member.key) },
            onMergeThrough = stack.onMergeThrough?.let { through -> { member -> through(member.key) } },
        )
    }
}

/** The muted tabular `01 / 03` in a Guide band's leading slot. */
@Composable
private fun GuideCaption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall.copy(fontFeatureSettings = "tnum"),
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        maxLines = 1,
        modifier = Modifier.testTag("guide-section-caption"),
    )
}

/** One Guide group under its band (or none, for the lead): text, its
 *  Changes row, tiles, Earlier. */
@Composable
private fun GuideGroupBody(
    group: SessionResultGroup,
    changes: GuideChangeSet?,
    onOpenChanges: (() -> Unit)?,
    tileHeight: Int,
    onPreview: (SessionResultEntry) -> Unit,
) {
    // EXP-933: the topic's report text sits ABOVE its shots.
    group.text?.let { text ->
        MarkdownView(
            markdown = text,
            modifier = Modifier.fillMaxWidth().testTag("work-result-text"),
        )
    }
    // EXP-1251: ONE Changes row per section (none for a section whose paths
    // matched nothing, or while the diff is not loaded).
    if (changes != null && changes.fileCount > 0) {
        GuideChangesRow(changes = changes, onOpen = onOpenChanges)
    }
    // The shots wrap rather than scroll sideways.
    if (group.entries.isNotEmpty()) ResultTiles(
        entries = group.entries,
        tileHeight = tileHeight,
        onOpen = onPreview,
    )
    // EXP-1172: the pictures the run SHOWED while it worked fold under a
    // collapsed `Earlier · N` band, so the final report leads.
    if (group.earlier.isNotEmpty()) {
        var earlierOpen by remember { mutableStateOf(false) }
        EarlierBand(
            count = group.earlier.size,
            expanded = earlierOpen,
            onToggle = { earlierOpen = !earlierOpen },
        )
        if (earlierOpen) ResultTiles(
            entries = group.earlier,
            tileHeight = tileHeight,
            onOpen = onPreview,
        )
    }
}

/**
 * EXP-1251: a section's ONE Changes row (web `GuideChangesRow`): the
 * `guide-changes` glyph, the label, the muted file count, `+A −D` and a
 * chevron; inert (no chevron) without [onOpen]. A hairline under it.
 */
@Composable
internal fun GuideChangesRow(
    changes: GuideChangeSet,
    onOpen: (() -> Unit)?,
    label: String = GUIDE_CHANGES_TOPIC,
) {
    GuideRow(
        changes = changes,
        onOpen = onOpen,
        testTag = "guide-changes-row",
        lead = {
            Icon(
                ExpIcons.guideChanges,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        },
        label = label,
        labelColor = MaterialTheme.colorScheme.onSurface,
        hairlineAbove = false,
    )
}

/** EXP-1251: the Guide's final hairline row: the whole diff. */
@Composable
internal fun GuideShowCompleteDiffRow(changes: GuideChangeSet, onOpen: (() -> Unit)?) {
    GuideRow(
        changes = changes,
        onOpen = onOpen,
        testTag = "guide-complete-diff",
        lead = null,
        label = DomainContract.diffUiGuideShowCompleteDiff,
        labelColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        hairlineAbove = true,
    )
}

@Composable
private fun GuideRow(
    changes: GuideChangeSet,
    onOpen: (() -> Unit)?,
    testTag: String,
    lead: (@Composable () -> Unit)?,
    label: String,
    labelColor: Color,
    hairlineAbove: Boolean,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(modifier = Modifier.fillMaxWidth()) {
        if (hairlineAbove) HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .flatRow()
                .then(if (onOpen != null) Modifier.clickable(onClick = onOpen) else Modifier)
                .padding(horizontal = 4.dp, vertical = 10.dp)
                .testTag(testTag),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            lead?.invoke()
            Text(
                label,
                style = MaterialTheme.typography.bodyMedium,
                color = labelColor,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text(
                guideFileCountLabel(changes.fileCount),
                style = MaterialTheme.typography.labelSmall.copy(fontFeatureSettings = "tnum"),
                color = muted,
                maxLines = 1,
            )
            DiffCounts(additions = changes.additions, deletions = changes.deletions)
            if (onOpen != null) {
                Icon(ExpIcons.uiChevronRight, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
            }
        }
        if (!hairlineAbove) HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
    }
}

/**
 * EXP-1154: an open PR with no run report while its GitHub body loads (a
 * spinner) or after the fetch failed (the refusal). The loaded body is a
 * Guide group ([prDescriptionGroups]).
 */
@Composable
private fun PrFallbackSection(state: PrDescriptionState) {
    Column(
        modifier = Modifier.testTag("results-pr-fallback"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when (state) {
            PrDescriptionState.Loading -> Row(
                modifier = Modifier.padding(vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                Text(
                    "Loading the pull request…",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            }
            is PrDescriptionState.Failed -> Text(
                state.message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(vertical = 12.dp),
            )
            is PrDescriptionState.Loaded -> Unit
        }
    }
}

/** One topic's wrapping row of equal-height tiles. */
@Composable
private fun ResultTiles(
    entries: List<SessionResultEntry>,
    tileHeight: Int,
    onOpen: (SessionResultEntry) -> Unit,
) {
    FlowRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        entries.forEach { entry ->
            ResultTile(entry = entry, tileHeight = tileHeight, onOpen = { onOpen(entry) })
        }
    }
}

/**
 * EXP-1172: the muted `Earlier · N` disclosure under a group's tiles — the
 * chevron fold every collapsible row wears; it expands IN PLACE to the same
 * tiles. Collapsed by default.
 */
@Composable
private fun EarlierBand(count: Int, expanded: Boolean, onToggle: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            .padding(vertical = 4.dp)
            .semantics {
                contentDescription = "$SESSION_RESULTS_EARLIER_LABEL, $count, ${if (expanded) "expanded" else "collapsed"}"
            }
            .testTag("work-results-earlier"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(
            if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = muted,
        )
        Text(
            "$SESSION_RESULTS_EARLIER_LABEL · $count",
            style = MaterialTheme.typography.labelMedium,
            color = muted,
            maxLines = 1,
        )
    }
}

/**
 * One shot: the image at the shared height and its probed width, the label as
 * a one-line caption under it. The URL is DERIVED from the attachment id — the
 * Coil `InstanceUrlInterceptor` absolutizes the relative path against the
 * owning account and attaches its bearer token, exactly as comment
 * attachments load.
 */
@Composable
internal fun ResultTile(
    entry: SessionResultEntry,
    tileHeight: Int,
    onOpen: () -> Unit,
    // EXP-1172: the transcript's inline tile reads `sessionResultTileCaption`.
    caption: String = entry.label,
) {
    Column(
        modifier = Modifier
            .width(sessionResultTileWidth(entry, tileHeight).dp)
            .testTag("work-result-${entry.attachmentId}"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        val frame = Modifier
            .fillMaxWidth()
            .height(tileHeight.dp)
            .clip(TileShape)
            .background(GlassTokens.RowFill)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, TileShape)
            .clickable(onClick = onOpen)
        if (sessionResultIsTall(entry)) {
            TallTileImage(entry = entry, tileHeight = tileHeight, modifier = frame)
        } else {
            AsyncImage(
                model = sessionResultImageUrl(entry),
                contentDescription = entry.label,
                contentScale = ContentScale.Crop,
                modifier = frame,
            )
        }
        Text(
            caption,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * EXP-1128: a TALL shot's tile — its TOP in the 4:3 frame (the source's first
 * `width * 3 / 4` rows, decoded at the tile's pixel width, so it stays sharp
 * where Coil's size cap would blur it), a bottom fade and a `Tall` pill. The
 * frame's fill stands in while the bytes load.
 */
@Composable
private fun TallTileImage(entry: SessionResultEntry, tileHeight: Int, modifier: Modifier) {
    val load = rememberTallImage(entry)
    val source = load.source
    val sourceWidth = entry.width ?: 0
    val targetPx = with(LocalDensity.current) { sessionResultTileWidth(entry, tileHeight).dp.roundToPx() }
    val decoder = (source as? TallImageSource.Regions)?.decoder
    val bitmap by produceState<ImageBitmap?>(null, decoder, targetPx) {
        value = decoder?.let {
            withContext(Dispatchers.Default) {
                decodeTallRegion(it, 0, sourceWidth * 3 / 4, targetPx)?.asImageBitmap()
            }
        }
    }
    Box(modifier = modifier) {
        when (source) {
            // A format the region decoder cannot read: Coil draws it, top-cropped.
            TallImageSource.Unsupported -> AsyncImage(
                model = sessionResultImageUrl(entry),
                contentDescription = entry.label,
                contentScale = ContentScale.Crop,
                alignment = Alignment.TopCenter,
                modifier = Modifier.fillMaxSize(),
            )
            is TallImageSource.Failed -> TallImageRetry(load.retry, Modifier.align(Alignment.Center))
            else -> bitmap?.let {
                Image(
                    bitmap = it,
                    contentDescription = entry.label,
                    contentScale = ContentScale.Crop,
                    alignment = Alignment.TopCenter,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
        // Drawn over the picture, never clickable: the frame owns the tap.
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .height(48.dp)
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.5f)))),
        )
        Text(
            "Tall",
            style = MaterialTheme.typography.labelSmall,
            color = Color.White.copy(alpha = 0.9f),
            modifier = Modifier
                .align(Alignment.BottomEnd)
                .padding(8.dp)
                .clip(CircleShape)
                .background(Color.Black.copy(alpha = 0.6f))
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape)
                .padding(horizontal = 6.dp, vertical = 2.dp)
                .testTag("work-result-tall"),
        )
    }
}

/** The shot at full size, in app: dark scrim, FIT so nothing is cropped away,
 *  a tap anywhere (or Back) closes it. EXP-1149: the picture pinch-zooms and
 *  pans (`ZoomableBox`, double tap toggles), decoded at its ORIGINAL size so a
 *  zoomed view stays sharp; zoomed, only Close, Back or a double tap leave.
 *  A TALL shot (EXP-1128) instead fits to WIDTH and scrolls; only Close and
 *  Back dismiss it, since a scrim tap would fight the scroll. */
@Composable
internal fun ResultPreviewDialog(entry: SessionResultEntry, onDismiss: () -> Unit) {
    val tall = sessionResultIsTall(entry)
    val context = LocalContext.current
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.92f)),
            contentAlignment = Alignment.Center,
        ) {
            if (tall) {
                TallImageScroll(entry)
            } else {
                val width = entry.width
                val height = entry.height
                val aspect = if (width != null && height != null && width > 0 && height > 0) {
                    width.toFloat() / height.toFloat()
                } else {
                    null
                }
                ZoomableBox(
                    modifier = Modifier.fillMaxSize().testTag("work-result-preview"),
                    contentAspect = aspect,
                    onTap = onDismiss,
                ) {
                    AsyncImage(
                        model = remember(entry.attachmentId) {
                            ImageRequest.Builder(context)
                                .data(sessionResultImageUrl(entry))
                                .size(Size.ORIGINAL)
                                .build()
                        },
                        contentDescription = entry.label,
                        contentScale = ContentScale.Fit,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            }
            IconButton(
                onClick = onDismiss,
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .statusBarsPadding()
                    .padding(8.dp),
            ) {
                Icon(
                    ExpIcons.uiClose,
                    contentDescription = "Close",
                    tint = Color.White.copy(alpha = 0.9f),
                )
            }
        }
    }
}

/**
 * EXP-1128: the tall viewer's body — the picture as a column of region-decoded
 * STRIPS (`tallImageStripRanges`), each sized from the width scale before it
 * decodes so the scroll never jumps. Never upscaled past its natural width.
 */
@Composable
private fun TallImageScroll(entry: SessionResultEntry) {
    val sourceWidth = entry.width ?: return
    val sourceHeight = entry.height ?: return
    val load = rememberTallImage(entry)
    val source = load.source
    val density = LocalDensity.current
    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        val naturalDp = with(density) { sourceWidth.toDp() }
        val columnWidth = if (maxWidth < naturalDp) maxWidth else naturalDp
        val columnPx = with(density) { columnWidth.roundToPx() }
        // dp per source row: the strip heights are known before any decode.
        val rowDp = columnWidth.value / sourceWidth
        val strips = remember(sourceHeight) { tallImageStripRanges(sourceHeight) }
        when (source) {
            // Coil at the column's width: the fallback scrolls as one picture.
            TallImageSource.Unsupported -> Box(
                modifier = Modifier
                    .width(columnWidth)
                    .fillMaxHeight()
                    .verticalScroll(rememberScrollState())
                    .testTag("work-result-preview"),
            ) {
                AsyncImage(
                    model = sessionResultImageUrl(entry),
                    contentDescription = entry.label,
                    contentScale = ContentScale.FillWidth,
                    modifier = Modifier.fillMaxWidth().height((sourceHeight * rowDp).dp),
                )
            }
            is TallImageSource.Failed -> Column(
                modifier = Modifier.fillMaxSize().padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text(
                    source.message,
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
                Spacer(Modifier.height(12.dp))
                TallImageRetry(load.retry)
            }
            else -> Unit
        }
        val decoder = (source as? TallImageSource.Regions)?.decoder
        if (source is TallImageSource.Loading || decoder != null) LazyColumn(
            modifier = Modifier.width(columnWidth).fillMaxHeight().testTag("work-result-preview"),
        ) {
            itemsIndexed(strips, key = { index, _ -> index }) { _, (top, rows) ->
                val stripHeight = (rows * rowDp).dp
                val strip by produceState<ImageBitmap?>(null, decoder, columnPx) {
                    value = decoder?.let {
                        withContext(Dispatchers.Default) {
                            decodeTallRegion(it, top, rows, columnPx)?.asImageBitmap()
                        }
                    }
                }
                Box(modifier = Modifier.fillMaxWidth().height(stripHeight)) {
                    strip?.let {
                        Image(
                            bitmap = it,
                            contentDescription = if (top == 0) entry.label else null,
                            contentScale = ContentScale.FillWidth,
                            modifier = Modifier.fillMaxWidth().height(stripHeight),
                        )
                    }
                }
            }
        }
    }
}

/** A failed tall load's one control: a tap runs the download again. */
@Composable
private fun TallImageRetry(onRetry: () -> Unit, modifier: Modifier = Modifier) {
    GlassPill(
        label = "Retry",
        icon = ExpIcons.uiRefresh,
        size = PillSize.Sm,
        onClick = onRetry,
        modifier = modifier.testTag("work-result-retry"),
    )
}

/** The stored relative attachment URL — never absolutized here (EXP-824's
 *  rule): the image loader resolves it against the active instance. */
private fun sessionResultImageUrl(entry: SessionResultEntry): String =
    "/api/attachments/${entry.attachmentId}"
