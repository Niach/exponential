package com.exponential.app.ui.issue

import androidx.compose.foundation.background
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity

private val SUGGESTED_COLORS = listOf(
    "#6366f1", "#22c55e", "#eab308", "#ef4444", "#f97316", "#06b6d4", "#a855f7",
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LabelPickerSheet(
    workspaceLabels: List<LabelEntity>,
    selectedLabelIds: Set<String>,
    onToggle: (String, Boolean) -> Unit,
    onCreate: (String, String) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var newName by remember { mutableStateOf("") }
    var newColor by remember { mutableStateOf(SUGGESTED_COLORS.first()) }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(modifier = Modifier.padding(16.dp).fillMaxWidth()) {
            Text("Labels", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.size(12.dp))

            workspaceLabels.forEach { label ->
                val selected = label.id in selectedLabelIds
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onToggle(label.id, selected) }
                        .padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        modifier = Modifier
                            .size(12.dp)
                            .background(parseColor(label.color), CircleShape),
                    )
                    Spacer(Modifier.width(10.dp))
                    Text(label.name, modifier = Modifier.weight(1f))
                    if (selected) {
                        Icon(
                            Icons.Filled.Check,
                            null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }

            Spacer(Modifier.size(16.dp))
            Text(
                "New label",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.size(6.dp))
            OutlinedTextField(
                value = newName,
                onValueChange = { newName = it },
                placeholder = { Text("Label name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.size(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SUGGESTED_COLORS.forEach { color ->
                    val selected = color == newColor
                    Box(
                        modifier = Modifier
                            .size(if (selected) 28.dp else 22.dp)
                            .background(parseColor(color), CircleShape)
                            .clickable { newColor = color },
                    )
                }
            }
            Spacer(Modifier.size(8.dp))
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(
                    enabled = newName.isNotBlank(),
                    onClick = {
                        onCreate(newName, newColor)
                        newName = ""
                    },
                ) {
                    Icon(Icons.Filled.Add, null)
                    Spacer(Modifier.width(6.dp))
                    Text("Create label")
                }
            }
        }
    }
}
