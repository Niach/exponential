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
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import coil3.compose.AsyncImage
import coil3.request.ImageRequest
import coil3.size.Size
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.SessionResultGroup
import com.exponential.app.domain.sessionResultIsTall
import com.exponential.app.domain.sessionResultTileHeightFitting
import com.exponential.app.domain.sessionResultTileWidth
import com.exponential.app.domain.tallImageStripRanges
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-879: the Work screen's RESULTS FACE — the report (EXP-933: each
// topic's GFM text above its shots) and screenshots a run published with `exponential_sessions_results`, read off the synced
// `coding_sessions.results` blob. ONE scrolling page: a filled group band per
// topic (the EXP-818 section band every list wears) over a WRAPPING row of
// equal-height tiles, each captioned with its label; a tap opens the shot
// full screen.
//
// Results is a SUB-FACE of Run, like Changes: it owns neither the Stop/Resume
// verb (the top bar's, on Run only) nor the merge capsule (the Changes bar's)
// — so it has NO floating bar at all (EXP-1150: the faces are tabs now).

/** The horizontal content padding the page reserves on each side. */
private val HorizontalPadding = 16.dp
private val TileShape = RoundedCornerShape(10.dp)

@Composable
fun ResultsFace(
    padding: PaddingValues,
    groups: List<SessionResultGroup>,
) {
    var preview by remember { mutableStateOf<SessionResultEntry?>(null) }

    BoxWithConstraints(modifier = Modifier.padding(padding).fillMaxSize()) {
        // ONE factor for the whole page: the base height unless the widest
        // tile would overflow the column, then every tile scales by the same
        // amount so the equal-height strip survives (shared rule ×4). An
        // unmeasured page (zero width) renders at the base.
        val availableDp = (maxWidth - HorizontalPadding * 2).value.toInt()
        val tileHeight = remember(groups, availableDp) {
            sessionResultTileHeightFitting(groups.flatMap { it.entries }, availableDp)
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("work-results"),
            contentPadding = PaddingValues(
                start = HorizontalPadding,
                end = HorizontalPadding,
                top = 4.dp,
                bottom = BottomBarInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            // Indexed keys: a workflow's page concatenates several runs'
            // reports, so one topic may appear more than once.
            groups.forEachIndexed { index, group ->
                item(key = "topic_${index}_${group.topic}") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SectionHeader(title = group.topic)
                        // EXP-933: the topic's report text sits ABOVE its
                        // shots; a text-only topic is header + text.
                        group.text?.let { text ->
                            MarkdownView(
                                markdown = text,
                                modifier = Modifier.fillMaxWidth().testTag("work-result-text"),
                            )
                        }
                        // The shots wrap rather than scroll sideways: a topic
                        // with an iOS, an Android and a web shot reads as one
                        // block, not three hidden behind a swipe.
                        if (group.entries.isNotEmpty()) FlowRow(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            verticalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            group.entries.forEach { entry ->
                                ResultTile(
                                    entry = entry,
                                    tileHeight = tileHeight,
                                    onOpen = { preview = entry },
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    preview?.let { entry ->
        ResultPreviewDialog(entry = entry, onDismiss = { preview = null })
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
private fun ResultTile(entry: SessionResultEntry, tileHeight: Int, onOpen: () -> Unit) {
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
            entry.label,
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
    val bytes by rememberTallImageBytes(entry)
    val sourceWidth = entry.width ?: 0
    val targetPx = with(LocalDensity.current) { sessionResultTileWidth(entry, tileHeight).dp.roundToPx() }
    val bitmap by produceState<ImageBitmap?>(null, bytes, targetPx) {
        val data = bytes?.getOrNull() ?: return@produceState
        value = withContext(Dispatchers.Default) {
            decodeTallRegion(data, 0, sourceWidth * 3 / 4, targetPx)?.asImageBitmap()
        }
    }
    Box(modifier = modifier) {
        bitmap?.let {
            Image(
                bitmap = it,
                contentDescription = entry.label,
                contentScale = ContentScale.Crop,
                alignment = Alignment.TopCenter,
                modifier = Modifier.fillMaxSize(),
            )
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
private fun ResultPreviewDialog(entry: SessionResultEntry, onDismiss: () -> Unit) {
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
    val bytes by rememberTallImageBytes(entry)
    val density = LocalDensity.current
    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        val naturalDp = with(density) { sourceWidth.toDp() }
        val columnWidth = if (maxWidth < naturalDp) maxWidth else naturalDp
        val columnPx = with(density) { columnWidth.roundToPx() }
        // dp per source row: the strip heights are known before any decode.
        val rowDp = columnWidth.value / sourceWidth
        val strips = remember(sourceHeight) { tallImageStripRanges(sourceHeight) }
        LazyColumn(
            modifier = Modifier.width(columnWidth).fillMaxHeight().testTag("work-result-preview"),
        ) {
            itemsIndexed(strips, key = { index, _ -> index }) { _, (top, rows) ->
                val stripHeight = (rows * rowDp).dp
                val strip by produceState<ImageBitmap?>(null, bytes, columnPx) {
                    val data = bytes?.getOrNull() ?: return@produceState
                    value = withContext(Dispatchers.Default) {
                        decodeTallRegion(data, top, rows, columnPx)?.asImageBitmap()
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

/** The stored relative attachment URL — never absolutized here (EXP-824's
 *  rule): the image loader resolves it against the active instance. */
private fun sessionResultImageUrl(entry: SessionResultEntry): String =
    "/api/attachments/${entry.attachmentId}"
