package com.exponential.app.ui.components

import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.domain.MARKDOWN_PREVIEW_MAX_BYTES
import com.exponential.app.domain.MarkdownPreviewState
import com.exponential.app.domain.markdownPreviewHttpError
import com.exponential.app.domain.markdownPreviewOutcome
import com.exponential.app.domain.markdownPreviewPrecheck
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import kotlinx.coroutines.CancellationException

/**
 * EXP-1003: the in-app preview for `.md` attachments — the native twin of the
 * web's `AttachmentMarkdownPreview` (EXP-955). The file renders read-only
 * through the same [MarkdownView] as descriptions and comments, inside a full
 * sheet with a Download action (the system share chooser, where "Save to
 * Files/Drive" lives). The size is checked twice: a row over the ceiling
 * never fetches, and a fetched body over it (legacy `size_bytes = 0` rows)
 * shows the same too-large hint.
 */
@Composable
fun AttachmentMarkdownPreviewSheet(
    attachment: AttachmentEntity,
    load: suspend (AttachmentEntity) -> ByteArray,
    onDownload: () -> Unit,
    onDismiss: () -> Unit,
    downloading: Boolean = false,
) {
    val context = LocalContext.current
    var state by remember(attachment.id) {
        mutableStateOf<MarkdownPreviewState>(MarkdownPreviewState.Loading)
    }
    LaunchedEffect(attachment.id) {
        state = markdownPreviewPrecheck(attachment.sizeBytes) ?: try {
            // The loader stops at the ceiling + 1 byte, so a body past it
            // (a legacy `size_bytes = 0` row) is too large by its BYTE count,
            // never a truncated render.
            val bytes = load(attachment)
            if (bytes.size > MARKDOWN_PREVIEW_MAX_BYTES) {
                MarkdownPreviewState.TooLarge
            } else {
                markdownPreviewOutcome(bytes)
            }
        } catch (cancel: CancellationException) {
            throw cancel
        } catch (t: Throwable) {
            MarkdownPreviewState.Error(
                (t as? TrpcException)?.status?.value?.let(::markdownPreviewHttpError)
                    ?: trpcErrorMessage(t, "Couldn't load this file."),
            )
        }
    }

    GlassSheet(
        title = attachment.filename.ifBlank { "Preview" },
        onDismiss = onDismiss,
        modifier = Modifier.testTag("attachment-markdown-preview"),
        height = SheetHeight.Full,
        primaryAction = SheetPrimaryAction(
            label = "Download",
            onClick = onDownload,
            loading = downloading,
            icon = ExpIcons.uiDownload,
        ),
    ) {
        Text(
            text = if (attachment.sizeBytes > 0) {
                "Markdown · ${Formatter.formatShortFileSize(context, attachment.sizeBytes)}"
            } else {
                "Markdown"
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(
                start = GlassSheetDefaults.HorizontalPadding,
                end = GlassSheetDefaults.HorizontalPadding,
                bottom = 12.dp,
            ),
        )
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = GlassSheetDefaults.HorizontalPadding),
        ) {
            when (val current = state) {
                MarkdownPreviewState.Loading -> Row(
                    modifier = Modifier.padding(vertical = 24.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(14.dp),
                        strokeWidth = 2.dp,
                    )
                    Text(
                        "Loading...",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                MarkdownPreviewState.TooLarge -> Text(
                    "This file is too large to preview here. Download it to read it.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 24.dp),
                )
                is MarkdownPreviewState.Error -> Text(
                    current.message,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(vertical = 24.dp),
                )
                is MarkdownPreviewState.Ready -> MarkdownView(current.markdown)
            }
        }
    }
}
