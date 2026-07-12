package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.ui.markdown.model.PendingImage
import kotlinx.coroutines.launch

/**
 * An image block while editing: full-width tile pre-sized by the pending/probed
 * aspect ratio, a top-right delete affordance, and an uploading / retry overlay.
 * Mirrors iOS `BlockImageView`. Drafts render from in-memory bytes; committed
 * images load via Coil (the [InstanceUrlInterceptor] resolves the relative URL).
 */
@Composable
fun BlockImageEditView(
    model: EditorModel,
    row: EditorRow.Image,
    modifier: Modifier = Modifier,
) {
    val pending: PendingImage? = model.pendingImages[row.url]
    val uploadState = model.uploadState(row.id)
    val aspect = aspectRatioOf(pending)
    val source: Any = pending?.bytes ?: row.url
    val scope = rememberCoroutineScope()

    Column(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(vertical = 4.dp),
            contentAlignment = Alignment.TopEnd,
        ) {
            AsyncImage(
                model = source,
                contentDescription = row.alt,
                contentScale = ContentScale.Fit,
                modifier = Modifier
                    .fillMaxWidth()
                    .aspectRatio(aspect)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color.White.copy(alpha = 0.06f)),
            )

            if (uploadState == EditorModel.ImageUploadState.Uploading) {
                UploadingBadge(Modifier.align(Alignment.BottomStart).padding(8.dp))
            }
            if (uploadState == EditorModel.ImageUploadState.Failed) {
                // Re-runs the registered host upload for this row; the top-right
                // X (below) is the remove affordance. The badge shows WHY it
                // failed (server 4xx text) — a bare "tap to retry" hid
                // deterministic rejections (EXP-61).
                RetryBadge(
                    Modifier.align(Alignment.Center),
                    error = model.uploadError(row.id),
                ) {
                    scope.launch { model.retryUpload(row.id) }
                }
            }

            IconButton(
                onClick = { model.deleteImageRow(row.id) },
                modifier = Modifier.padding(4.dp),
            ) {
                Icon(
                    Icons.Filled.Cancel,
                    contentDescription = "Remove image",
                    tint = Color.White.copy(alpha = 0.85f),
                    modifier = Modifier.size(24.dp),
                )
            }
        }
    }
}

@Composable
private fun UploadingBadge(modifier: Modifier) {
    androidx.compose.foundation.layout.Row(
        modifier = modifier
            .clip(RoundedCornerShape(percent = 50))
            .background(Color.Black.copy(alpha = 0.45f))
            .padding(horizontal = 10.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CircularProgressIndicator(
            modifier = Modifier.size(14.dp),
            strokeWidth = 2.dp,
            color = Color.White,
        )
        Spacer(Modifier.size(6.dp))
        Text("Uploading…", color = Color.White, style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.8f))
    }
}

@Composable
private fun RetryBadge(modifier: Modifier, error: String?, onRetry: () -> Unit) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(8.dp))
            .background(Color.Black.copy(alpha = 0.55f))
            .padding(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        IconButton(onClick = onRetry) {
            Icon(Icons.Filled.Refresh, contentDescription = "Retry upload", tint = Color.White.copy(alpha = 0.8f))
        }
        Text("Upload failed — tap to retry", color = Color.White.copy(alpha = 0.7f), style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.75f))
        if (error != null) {
            Spacer(Modifier.height(4.dp))
            Text(
                error,
                color = Color.White.copy(alpha = 0.55f),
                style = MdStyle.body.copy(fontSize = MdStyle.bodySize * 0.65f),
                maxLines = 3,
            )
        }
    }
}

private fun aspectRatioOf(pending: PendingImage?): Float {
    val w = pending?.width
    val h = pending?.height
    return if (w != null && h != null && h > 0) w.toFloat() / h.toFloat() else 4f / 3f
}
