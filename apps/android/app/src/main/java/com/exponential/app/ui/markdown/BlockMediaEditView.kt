package com.exponential.app.ui.markdown

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.formatDuration
import com.exponential.app.domain.isInlineAudio
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.media.DurationChip
import com.exponential.app.ui.markdown.media.PlayGlyph
import com.exponential.app.ui.markdown.model.PendingImage
import kotlinx.coroutines.launch

/**
 * An inline media block while editing (EXP-824): a video shows its poster in
 * an aspect box with the play glyph and duration chip (no playback in edit
 * mode — the photo treatment, like [BlockImageEditView]'s static tile), an
 * audio file a compact row. Both carry the preparing / uploading / retry
 * badges and the top-right delete affordance. A draft renders from the
 * prepared bytes; a committed row from the synced attachment (poster via
 * Coil's [InstanceUrlInterceptor]).
 */
@Composable
fun BlockMediaEditView(
    model: EditorModel,
    row: EditorRow.Media,
    modifier: Modifier = Modifier,
) {
    val pending: PendingImage? = model.pendingImages[row.url]
    val info = LocalAttachmentDims.current.infoOf(row.url)
    val uploadState = model.uploadState(row.id)
    val isAudio = when {
        pending != null && pending.contentType.isNotEmpty() -> isInlineAudio(pending.contentType)
        else -> info?.isAudio == true
    }
    val durationMs = pending?.durationMs ?: info?.durationMs
    val scope = rememberCoroutineScope()

    Column(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 4.dp),
            contentAlignment = Alignment.TopEnd,
        ) {
            if (isAudio) {
                AudioEditTile(label = row.label, durationMs = durationMs)
            } else {
                val aspect = pendingAspect(pending)
                    ?: info?.aspectRatio
                    ?: DEFAULT_VIDEO_ASPECT_RATIO
                VideoEditTile(
                    url = row.url,
                    label = row.label,
                    posterBytes = pending?.poster,
                    hasSyncedPoster = info?.hasPoster == true,
                    aspect = aspect,
                    durationMs = durationMs,
                )
            }

            when (uploadState) {
                EditorModel.ImageUploadState.Preparing ->
                    UploadingBadge(Modifier.align(Alignment.BottomStart).padding(8.dp), text = "Preparing…")
                EditorModel.ImageUploadState.Uploading ->
                    UploadingBadge(Modifier.align(Alignment.BottomStart).padding(8.dp))
                EditorModel.ImageUploadState.Failed -> RetryBadge(
                    Modifier.align(Alignment.Center),
                    error = model.uploadError(row.id),
                ) {
                    scope.launch { model.retryUpload(row.id) }
                }
                EditorModel.ImageUploadState.Idle -> Unit
            }

            IconButton(
                onClick = { model.deleteImageRow(row.id) },
                modifier = Modifier.padding(4.dp),
            ) {
                Icon(
                    ExpIcons.uiClose,
                    contentDescription = "Remove ${row.label}",
                    tint = Color.White.copy(alpha = 0.85f),
                    modifier = Modifier.size(24.dp),
                )
            }
        }
    }
}

@Composable
private fun VideoEditTile(
    url: String,
    label: String,
    posterBytes: ByteArray?,
    hasSyncedPoster: Boolean,
    aspect: Float,
    durationMs: Long?,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .aspectRatio(aspect)
            .clip(RoundedCornerShape(8.dp))
            .background(Color.Black),
    ) {
        val bitmap = remember(posterBytes) {
            posterBytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
        }
        when {
            bitmap != null -> Image(
                bitmap = bitmap,
                contentDescription = label,
                contentScale = ContentScale.Fit,
                modifier = Modifier.matchParentSize(),
            )
            hasSyncedPoster -> AsyncImage(
                model = attachmentPosterUrl(url),
                contentDescription = label,
                contentScale = ContentScale.Fit,
                modifier = Modifier.matchParentSize(),
            )
            else -> Box(Modifier.matchParentSize().background(Color.White.copy(alpha = 0.06f)))
        }
        PlayGlyph(Modifier.align(Alignment.Center))
        DurationChip(durationMs, Modifier.align(Alignment.BottomEnd).padding(8.dp))
    }
}

@Composable
private fun AudioEditTile(label: String, durationMs: Long?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(Color.White.copy(alpha = 0.06f))
            .padding(horizontal = 10.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PlayGlyph(size = 32.dp)
        Spacer(Modifier.width(10.dp))
        Text(
            label,
            style = MdStyle.body,
            color = MdStyle.Text,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(10.dp))
        Text(
            formatDuration(durationMs),
            style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.8f),
            color = MdStyle.Dim,
        )
        // Room for the delete affordance drawn over the row's end.
        Spacer(Modifier.width(36.dp))
    }
}

/** The prepared pick's own size, or null while it is still being probed. */
private fun pendingAspect(pending: PendingImage?): Float? {
    val w = pending?.width
    val h = pending?.height
    return if (w != null && h != null && w > 0 && h > 0) w.toFloat() / h.toFloat() else null
}
