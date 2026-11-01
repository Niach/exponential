package com.exponential.app.ui.work

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import at.exponential.ui.compose.CoreZoomMath
import at.exponential.ui.primitives.ZoomIcons
import at.exponential.ui.primitives.ZoomLooks
import at.exponential.ui.primitives.ZoomMotion
import at.exponential.ui.primitives.ZoomPillAppearance
import at.exponential.ui.primitives.ZoomableImageState
import at.exponential.ui.primitives.ZoomableImageView
import at.exponential.ui.primitives.rememberZoomableImageState
import coil3.compose.AsyncImagePainter
import coil3.compose.rememberAsyncImagePainter
import coil3.request.ImageRequest
import com.exponential.app.domain.tallImageStripRanges
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.attachmentIdFromUrl
import com.exponential.app.ui.theme.AppPrimitiveTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// EXP-1586: THE picture viewer, phone form — a Results / `sessions_show`
// picture and the plan lightbox open THIS full-screen page. The picture sits
// in the SDK's `ZoomableImageView` (catalog/style.json `zoomableImage`, the
// core's maths): it opens at fit (never upscaled; a tall capture at
// fit-width from its top), pinch zooms about the fingers, a double tap goes
// rest ↔ max(100%, 2× rest), one finger pans only along an axis with room, a
// tap toggles THE zoom pill. A set steps with `‹ i / n ›` and, only while the
// picture rests, a horizontal swipe (no wrap); a step opens the next picture
// at rest. Closing = ✕ or Back.

/** One picture: [id] = the attachment id (the zoom's reset key), [src] its relative URL. */
data class PictureViewerItem(val id: String, val src: String, val label: String)

/** Source rows per decoded strip: a few strips cover the screen, so memory follows the view. */
private const val STRIP_ROWS = 1024

@Composable
fun PictureViewer(items: List<PictureViewerItem>, start: Int = 0, onClose: () -> Unit) {
    if (items.isEmpty()) return
    val pager = rememberPagerState(initialPage = start.coerceIn(0, items.lastIndex)) { items.size }
    val scope = rememberCoroutineScope()
    // The shown picture is off its rest view: the set does not step.
    var zoomed by remember { mutableStateOf(false) }
    LaunchedEffect(pager.currentPage) { zoomed = false }
    Dialog(
        onDismissRequest = onClose,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Black)
                .testTag("picture-viewer"),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .height(52.dp)
                    .padding(horizontal = 8.dp),
            ) {
                if (items.size > 1) {
                    Row(
                        modifier = Modifier.align(Alignment.Center),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        CircleIconButton(
                            ExpIcons.uiChevronLeft,
                            "Previous picture",
                            onClick = { scope.launch { pager.animateScrollToPage(pager.currentPage - 1) } },
                            modifier = Modifier.testTag("picture-viewer-prev"),
                            tint = Color.White.copy(alpha = if (pager.currentPage > 0) 1f else TextEmphasis.Quaternary),
                            enabled = pager.currentPage > 0,
                            borderless = true,
                        )
                        Text(
                            "${pager.currentPage + 1} / ${items.size}",
                            style = MaterialTheme.typography.bodyMedium,
                            color = Color.White.copy(alpha = TextEmphasis.Secondary),
                            modifier = Modifier.testTag("picture-viewer-index"),
                        )
                        CircleIconButton(
                            ExpIcons.uiChevronRight,
                            "Next picture",
                            onClick = { scope.launch { pager.animateScrollToPage(pager.currentPage + 1) } },
                            modifier = Modifier.testTag("picture-viewer-next"),
                            tint = Color.White.copy(alpha = if (pager.currentPage < items.lastIndex) 1f else TextEmphasis.Quaternary),
                            enabled = pager.currentPage < items.lastIndex,
                            borderless = true,
                        )
                    }
                }
                CircleIconButton(
                    ExpIcons.uiClose,
                    "Close",
                    onClick = onClose,
                    modifier = Modifier.align(Alignment.CenterStart).testTag("picture-viewer-close"),
                    tint = Color.White,
                    borderless = true,
                )
            }
            HorizontalPager(
                state = pager,
                modifier = Modifier.fillMaxWidth().weight(1f).navigationBarsPadding(),
                userScrollEnabled = !zoomed,
            ) { index ->
                PictureViewerPage(
                    item = items[index],
                    current = index == pager.currentPage,
                    onZoomed = { if (index == pager.currentPage) zoomed = it },
                )
            }
        }
    }
}

/** THE pill in the app's glass tokens (iOS `PictureViewerPage.pillStyle`). */
@Composable
private fun pictureViewerPill(): ZoomPillAppearance = remember {
    ZoomPillAppearance(
        looks = ZoomLooks.fromTokens(
            AppPrimitiveTokens.copy(
                foreground = Color.White,
                mutedForeground = Color.White.copy(alpha = TextEmphasis.Secondary),
                card = GlassTokens.OpaqueCardFill,
                border = GlassTokens.StrokeCard,
                accent = GlassTokens.RowFillActive,
            ),
        ),
        icons = ZoomIcons(
            zoomOut = { c, s -> Icon(ExpIcons.uiMinus, contentDescription = null, tint = c, modifier = Modifier.size(s)) },
            zoomIn = { c, s -> Icon(ExpIcons.uiAdd, contentDescription = null, tint = c, modifier = Modifier.size(s)) },
            fit = { c, s -> Icon(ExpIcons.uiFullscreen, contentDescription = null, tint = c, modifier = Modifier.size(s)) },
            check = { c, s -> Icon(ExpIcons.uiCheck, contentDescription = null, tint = c, modifier = Modifier.size(s)) },
        ),
    )
}

/**
 * One picture of the viewer. An attachment decodes from its original bytes
 * through ONE region decoder (EXP-1128: a 25k px full-page capture never
 * goes through Coil's size cap or a single texture), laid out at its pixel
 * size as strips; anything else (an external URL, bytes the region decoder
 * cannot read) is Coil's original-size bitmap.
 */
@Composable
private fun PictureViewerPage(item: PictureViewerItem, current: Boolean, onZoomed: (Boolean) -> Unit) {
    val state = rememberZoomableImageState(CoreZoomMath)
    // A page the set stepped away from goes back to rest.
    LaunchedEffect(current) {
        if (!current) state.rest?.let { state.animateTo(it.view, animated = false, report = false) }
    }
    val attachmentId = remember(item.src) { attachmentIdFromUrl(item.src) }
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        if (attachmentId == null) {
            CoilPicture(item, state, onZoomed)
        } else {
            val load = rememberTallImage(attachmentId)
            when (val source = load.source) {
                TallImageSource.Loading -> PictureViewerSpinner()
                is TallImageSource.Failed -> PictureViewerFailure(source.message, load.retry)
                TallImageSource.Unsupported -> CoilPicture(item, state, onZoomed)
                is TallImageSource.Regions -> StripPicture(item, source, state, onZoomed)
            }
        }
    }
}

@Composable
private fun ZoomablePicture(
    item: PictureViewerItem,
    naturalSize: Size,
    state: ZoomableImageState,
    onZoomed: (Boolean) -> Unit,
    content: @Composable () -> Unit,
) {
    ZoomableImageView(
        naturalSize = naturalSize,
        state = state,
        modifier = Modifier.fillMaxSize(),
        contentKey = item.id,
        contentDescription = item.label,
        onZoom = { _, atRest -> onZoomed(!atRest) },
        appearance = pictureViewerPill(),
        motion = ZoomMotion(spec = Motion.standard(), reduced = LocalReduceMotion.current),
        content = content,
    )
}

/**
 * The region-decoded picture: [STRIP_ROWS]-row strips over the natural-size
 * box, only the ones on screen (and one either side) decoded, each at the
 * power-of-two subsample the CURRENT zoom draws it at, so a zoomed-out
 * full-page capture holds thumbnails and 100% holds source pixels.
 */
@Composable
private fun StripPicture(
    item: PictureViewerItem,
    source: TallImageSource.Regions,
    state: ZoomableImageState,
    onZoomed: (Boolean) -> Unit,
) {
    val decoder = source.decoder
    val width = decoder.width
    val height = decoder.height
    if (width <= 0 || height <= 0) return
    val density = LocalDensity.current.density
    val strips = remember(decoder) { tallImageStripRanges(height, STRIP_ROWS) }
    // The largest power-of-two subsample still at least as wide as drawn.
    val sample by remember(state, decoder) {
        derivedStateOf {
            val shown = width * state.view.scale * density
            var s = 1
            while (shown > 0f && width / (s * 2) >= shown) s *= 2
            s
        }
    }
    // The strips under the viewport, one strip of margin either side.
    val visible by remember(state, decoder) {
        derivedStateOf {
            val frame = state.frame ?: return@derivedStateOf IntRange.EMPTY
            val view = state.view
            if (view.scale <= 0f) return@derivedStateOf IntRange.EMPTY
            val y0 = -view.y / view.scale - STRIP_ROWS
            val y1 = (frame.viewportHeight - view.y) / view.scale + STRIP_ROWS
            val first = strips.indexOfFirst { (top, rows) -> top + rows > y0 }
            val last = strips.indexOfLast { (top, _) -> top < y1 }
            if (first < 0 || last < first) IntRange.EMPTY else first..last
        }
    }
    ZoomablePicture(item, Size(width.toFloat(), height.toFloat()), state, onZoomed) {
        Column(modifier = Modifier.fillMaxSize()) {
            strips.forEachIndexed { index, (top, rows) ->
                val shown = index in visible
                val targetPx = width / sample
                val strip by produceState<ImageBitmap?>(null, decoder, shown, targetPx) {
                    value = if (shown) {
                        withContext(Dispatchers.Default) { decodeTallRegion(decoder, top, rows, targetPx)?.asImageBitmap() }
                    } else {
                        null
                    }
                }
                Box(modifier = Modifier.fillMaxWidth().weight(rows.toFloat())) {
                    strip?.let {
                        Image(
                            bitmap = it,
                            contentDescription = null,
                            contentScale = ContentScale.FillBounds,
                            modifier = Modifier.fillMaxSize(),
                        )
                    }
                }
            }
        }
    }
}

/** An external URL or a format the region decoder cannot read: Coil at the original size. */
@Composable
private fun CoilPicture(item: PictureViewerItem, state: ZoomableImageState, onZoomed: (Boolean) -> Unit) {
    val context = LocalContext.current
    val painter = rememberAsyncImagePainter(
        remember(item.src) {
            ImageRequest.Builder(context)
                .data(item.src)
                .size(coil3.size.Size.ORIGINAL)
                .build()
        },
    )
    val painterState by painter.state.collectAsState()
    when (val loaded = painterState) {
        is AsyncImagePainter.State.Success -> {
            val natural = loaded.painter.intrinsicSize
            if (natural.isSpecified && natural.width > 0f && natural.height > 0f) {
                ZoomablePicture(item, natural, state, onZoomed) {
                    Image(
                        painter = painter,
                        contentDescription = null,
                        contentScale = ContentScale.FillBounds,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
            }
        }
        is AsyncImagePainter.State.Error -> PictureViewerFailure("Couldn't load this picture.", painter::restart)
        else -> PictureViewerSpinner()
    }
}

@Composable
private fun PictureViewerSpinner() {
    CircularProgressIndicator(
        color = Color.White,
        strokeWidth = 2.dp,
        modifier = Modifier.size(24.dp),
    )
}

@Composable
private fun PictureViewerFailure(message: String, onRetry: () -> Unit) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            message,
            style = MaterialTheme.typography.bodySmall,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.height(12.dp))
        TallImageRetry(onRetry)
    }
}
