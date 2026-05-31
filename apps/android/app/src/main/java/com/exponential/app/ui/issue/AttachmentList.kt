package com.exponential.app.ui.issue

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.Image
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.auth.AuthRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.stateIn

// Surfaces attachments synced via the `attachments` Electric shape as a
// discoverable list. The canonical reference for an attachment is the
// markdown embed in the description — the upload endpoint at
// /api/issues/:id/images returns the same URL that lives in `url`.
@Composable
fun AttachmentList(
    issueId: String,
    viewModel: AttachmentListViewModel = hiltViewModel(),
) {
    LaunchedEffect(issueId) { viewModel.bind(issueId) }
    val attachments by viewModel.attachments.collectAsStateWithLifecycle()
    val context = LocalContext.current

    if (attachments.isEmpty()) return

    Column(modifier = Modifier.fillMaxWidth()) {
        Text(
            "Attachments (${attachments.size})",
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(8.dp))
        attachments.forEach { attachment ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(vertical = 3.dp)
                    .background(
                        MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.35f),
                        RoundedCornerShape(8.dp),
                    )
                    .clickable {
                        runCatching {
                            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(attachment.url))
                            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            context.startActivity(intent)
                        }
                    }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Icon(
                    if (attachment.contentType.startsWith("image/"))
                        Icons.Filled.Image
                    else
                        Icons.Filled.AttachFile,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        attachment.filename,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        formatBytes(attachment.sizeBytes),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Icon(
                    Icons.AutoMirrored.Filled.OpenInNew,
                    contentDescription = "Open",
                    modifier = Modifier.size(16.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

private fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val units = listOf("KB", "MB", "GB")
    var value = bytes.toDouble() / 1024.0
    for (unit in units) {
        if (value < 1024.0) return "%.1f %s".format(value, unit)
        value /= 1024.0
    }
    return "%.1f TB".format(value)
}

@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel
class AttachmentListViewModel @Inject constructor(
    private val holder: DatabaseHolder,
    private val auth: AuthRepository,
) : ViewModel() {
    private val accountId = auth.activeAccountId.value ?: ""
    private val db = holder.database(forAccountId = accountId)

    private val issueIdFlow = MutableStateFlow<String?>(null)

    val attachments: StateFlow<List<AttachmentEntity>> = issueIdFlow.flatMapLatest { id ->
        if (id == null) flowOf(emptyList()) else db.attachmentDao().observeByIssue(id)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    fun bind(issueId: String) {
        issueIdFlow.value = issueId
    }
}
