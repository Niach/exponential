package com.exponential.app.ui.issue

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.text.format.Formatter
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.ui.icons.ExpIcons
import java.io.File
import kotlinx.coroutines.launch

/**
 * The issue's file attachments (EXP-297) — everything that is not one of the
 * five inline-embeddable raster types. These rows never appear in the
 * markdown, so this section is the only place they exist for the user:
 * open in another app, share, delete.
 *
 * EXP-327: there is no attach button here any more, and no empty state. Files
 * are attached from the description editor's image button ("Photo library /
 * Files"), which is the one place a user reaches for when adding something —
 * so with nothing attached this section renders nothing at all.
 */
@Composable
fun IssueFilesSection(
    viewModel: IssueDetailViewModel,
    canDelete: Boolean,
    modifier: Modifier = Modifier,
) {
    val files by viewModel.fileAttachments.collectAsStateWithLifecycle()
    val pending by viewModel.pendingFiles.collectAsStateWithLifecycle()
    val busyIds by viewModel.busyAttachmentIds.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var confirmDelete by remember { mutableStateOf<AttachmentEntity?>(null) }

    // No files (and none in flight): stay out of the way entirely. A failed
    // upload keeps a pending row, so errors still have somewhere to surface.
    if (files.isEmpty() && pending.isEmpty()) return

    Column(modifier = modifier.fillMaxWidth()) {
        Text(
            text = "Files",
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )

        for (file in files) {
            FileRow(
                filename = file.filename,
                subtitle = Formatter.formatShortFileSize(context, file.sizeBytes),
                busy = file.id in busyIds,
                canDelete = canDelete,
                onOpen = {
                    scope.launch {
                        val local = viewModel.downloadToCache(file) ?: return@launch
                        openFile(context, local, file.contentType)
                    }
                },
                onShare = {
                    scope.launch {
                        val local = viewModel.downloadToCache(file) ?: return@launch
                        shareFile(context, local, file.contentType)
                    }
                },
                onDelete = { confirmDelete = file },
            )
        }

        for (upload in pending) {
            PendingFileRow(
                upload = upload,
                onRetry = { viewModel.retryFileUpload(upload.key) },
                onDismiss = { viewModel.dismissFileUpload(upload.key) },
            )
        }
    }

    confirmDelete?.let { target ->
        AlertDialog(
            onDismissRequest = { confirmDelete = null },
            title = { Text("Delete file") },
            text = {
                Text(
                    "Delete \"${target.filename}\"? This cannot be undone. " +
                        "Anywhere it is referenced in text, a placeholder is left behind.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = null
                    viewModel.deleteAttachment(target.id)
                }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmDelete = null }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun FileRow(
    filename: String,
    subtitle: String,
    busy: Boolean,
    canDelete: Boolean,
    onOpen: () -> Unit,
    onShare: () -> Unit,
    onDelete: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = !busy, onClick = onOpen)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (busy) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                strokeWidth = 2.dp,
            )
        } else {
            Icon(
                ExpIcons.uiFile,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = filename,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Box {
            IconButton(onClick = { menuOpen = true }) {
                Icon(
                    ExpIcons.uiMoreVertical,
                    contentDescription = "File actions",
                    modifier = Modifier.size(18.dp),
                )
            }
            DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                DropdownMenuItem(
                    text = { Text("Open") },
                    onClick = {
                        menuOpen = false
                        onOpen()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Share") },
                    onClick = {
                        menuOpen = false
                        onShare()
                    },
                )
                if (canDelete) {
                    DropdownMenuItem(
                        text = { Text("Delete", color = MaterialTheme.colorScheme.error) },
                        onClick = {
                            menuOpen = false
                            onDelete()
                        },
                    )
                }
            }
        }
    }
}

@Composable
private fun PendingFileRow(
    upload: PendingFileUpload,
    onRetry: () -> Unit,
    onDismiss: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (upload.error == null) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                strokeWidth = 2.dp,
            )
        } else {
            Icon(
                ExpIcons.uiWarning,
                contentDescription = null,
                modifier = Modifier.size(18.dp),
                tint = MaterialTheme.colorScheme.error,
            )
        }
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = upload.filename,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = upload.error ?: "Uploading…",
                style = MaterialTheme.typography.bodySmall,
                color = if (upload.error == null) {
                    MaterialTheme.colorScheme.onSurfaceVariant
                } else {
                    MaterialTheme.colorScheme.error
                },
            )
        }
        if (upload.error != null) {
            Row(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = onRetry) { Text("Retry") }
                IconButton(onClick = onDismiss) {
                    Icon(
                        ExpIcons.uiClose,
                        contentDescription = "Dismiss",
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}

private fun fileProviderUri(context: Context, file: File): Uri =
    FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)

/**
 * Hand the cached bytes to whatever app can render them. The chooser carries
 * the read grant; if the device has nothing at all for the type we fall back
 * to a share sheet, which at least lets the user save it somewhere.
 */
private fun openFile(context: Context, file: File, contentType: String) {
    val uri = fileProviderUri(context, file)
    val view = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, contentType.ifBlank { "*/*" })
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    val chooser = Intent.createChooser(view, "Open with")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    try {
        context.startActivity(chooser)
    } catch (_: ActivityNotFoundException) {
        shareFile(context, file, contentType)
    }
}

private fun shareFile(context: Context, file: File, contentType: String) {
    val uri = fileProviderUri(context, file)
    val send = Intent(Intent.ACTION_SEND).apply {
        type = contentType.ifBlank { "*/*" }
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    val chooser = Intent.createChooser(send, "Share file")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    runCatching { context.startActivity(chooser) }
}
