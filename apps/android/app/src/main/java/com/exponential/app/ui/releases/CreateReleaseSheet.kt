package com.exponential.app.ui.releases

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Release creation sheet (EXP-62): name (optional — blank lets the server
 * auto-name "Release N") + the shared [IssueMultiSelectPicker] so the issues
 * are picked BEFORE the release exists. Create stays disabled until at least
 * one issue is selected — an empty release is useless. Mirrors the web/IDE
 * creation dialogs; one releases.create call attaches the bundle in the same
 * server transaction.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreateReleaseSheet(
    candidates: List<IssueEntity>,
    creating: Boolean,
    onConfirm: (name: String?, issueIds: List<String>) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var name by remember { mutableStateOf("") }
    var selected by remember { mutableStateOf(setOf<String>()) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
            Text(
                "New release",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )
            TextField(
                value = name,
                onValueChange = { name = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
                placeholder = {
                    Text(
                        "Release name (optional)",
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
                singleLine = true,
                shape = RoundedCornerShape(12.dp),
                colors = TextFieldDefaults.colors(
                    focusedContainerColor = GlassTokens.RowFill,
                    unfocusedContainerColor = GlassTokens.RowFill,
                    disabledContainerColor = GlassTokens.RowFill,
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    disabledIndicatorColor = Color.Transparent,
                ),
            )
            Spacer(Modifier.height(8.dp))
            IssueMultiSelectPicker(
                candidates = candidates,
                selected = selected,
                onToggle = { id ->
                    selected = if (id in selected) selected - id else selected + id
                },
            )
            Spacer(Modifier.height(8.dp))
            Button(
                onClick = {
                    onConfirm(name.trim().ifEmpty { null }, selected.toList())
                },
                enabled = selected.isNotEmpty() && !creating,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
            ) {
                Text(
                    when {
                        creating -> "Creating…"
                        selected.isEmpty() -> "Create release"
                        selected.size == 1 -> "Create with 1 issue"
                        else -> "Create with ${selected.size} issues"
                    }
                )
            }
        }
    }
}
