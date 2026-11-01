package com.exponential.app.ui.work

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.sessionResultIsTall
import com.exponential.app.domain.sessionResultTileWidth
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// EXP-1172: a published picture's tile + its full-size lightbox (kept from
// the retired Guide face; S-3: the Guide tab is gone ×4, the session view's
// inline picture tile still draws through these).

private val TileShape = RoundedCornerShape(10.dp)

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

/** The shot at full size: THE picture viewer (EXP-1586) over this one
 *  picture; a TALL shot (EXP-1128) opens at fit-width from its top. */
@Composable
internal fun ResultPreviewDialog(
    entry: SessionResultEntry,
    onDismiss: () -> Unit,
) {
    val items = remember(entry.attachmentId, entry.label) {
        listOf(PictureViewerItem(id = entry.attachmentId, src = sessionResultImageUrl(entry), label = entry.label))
    }
    PictureViewer(items = items, onClose = onDismiss)
}

/** A failed tall load's one control: a tap runs the download again. */
@Composable
internal fun TallImageRetry(onRetry: () -> Unit, modifier: Modifier = Modifier) {
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
internal fun sessionResultImageUrl(entry: SessionResultEntry): String =
    "/api/attachments/${entry.attachmentId}"
