package com.exponential.app.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.ui.components.RepositorySelector
import com.exponential.app.ui.theme.LabelPalette

// New-project form. Mirrors the web create-project dialog: name + prefix
// (auto-derived from the name, editable) + a color swatch + a REQUIRED
// repository (masterplan v4 §6 — a project is backed by a repo). The selector
// lists only already-connected repos; connecting new repos stays web-only. The
// host owns isCreating/error/repos and dismisses on success.
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun CreateProjectSheet(
    isCreating: Boolean,
    error: String?,
    repos: List<WorkspaceRepo>,
    reposLoading: Boolean,
    onDismiss: () -> Unit,
    onCreate: (name: String, prefix: String, color: String, repositoryId: String) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var name by remember { mutableStateOf("") }
    var prefix by remember { mutableStateOf("") }
    var prefixEdited by remember { mutableStateOf(false) }
    var color by remember { mutableStateOf("#6366f1") }
    var repositoryId by remember { mutableStateOf<String?>(null) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Text("New Project", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(12.dp))
            OutlinedTextField(
                value = name,
                onValueChange = {
                    name = it
                    if (!prefixEdited) prefix = derivePrefix(it)
                },
                label = { Text("Name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                value = prefix,
                onValueChange = {
                    prefixEdited = true
                    prefix = it.uppercase().take(10)
                },
                label = { Text("Prefix") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(12.dp))
            Text("Color", style = MaterialTheme.typography.labelMedium)
            Spacer(Modifier.height(6.dp))
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                LabelPalette.colors.forEach { c ->
                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .clip(CircleShape)
                            .background(Color(android.graphics.Color.parseColor(c)))
                            .then(
                                if (c == color) {
                                    Modifier.border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape)
                                } else {
                                    Modifier
                                }
                            )
                            .clickable { color = c },
                    )
                }
            }
            Spacer(Modifier.height(16.dp))
            Text("Repository", style = MaterialTheme.typography.labelMedium)
            Spacer(Modifier.height(6.dp))
            RepositorySelector(
                repos = repos,
                loading = reposLoading,
                selectedId = repositoryId,
                onSelect = { repositoryId = it },
            )
            if (error != null) {
                Spacer(Modifier.height(8.dp))
                Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(16.dp))
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                Button(
                    enabled = !isCreating && name.isNotBlank() && prefix.isNotBlank() && repositoryId != null,
                    onClick = {
                        val repoId = repositoryId ?: return@Button
                        onCreate(name.trim(), prefix.trim().uppercase(), color, repoId)
                    },
                ) {
                    Text(if (isCreating) "Creating…" else "Create")
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

private fun derivePrefix(name: String): String =
    name.trim().filter { it.isLetterOrDigit() }.take(3).uppercase()
