package com.exponential.app.ui.issue

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp

/**
 * A reusable bottom-sheet picker for issue properties (status, priority, assignee, ...).
 *
 * Material 3 chooser pattern: a column of [ListItem] rows, each with a leading icon, a label, and
 * a trailing check mark when selected. Tapping a row invokes [onSelect] with the item and the sheet
 * dismisses via [onDismiss].
 *
 * Uses `skipPartiallyExpanded = true` so it presents as a proper full-height surface even when
 * stacked above another bottom sheet (e.g. opened from inside `CreateIssueSheet`).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun <T> IssuePickerSheet(
    title: String,
    items: List<T>,
    selected: T?,
    keyOf: (T) -> Any = { it as Any },
    labelOf: (T) -> String,
    iconOf: ((T) -> ImageVector)? = null,
    onSelect: (T) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val selectedKey = selected?.let(keyOf)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(bottom = 12.dp),
        ) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )
            items.forEach { item ->
                val isSelected = keyOf(item) == selectedKey
                ListItem(
                    headlineContent = { Text(labelOf(item)) },
                    leadingContent = iconOf?.let {
                        { Icon(it(item), contentDescription = null) }
                    },
                    trailingContent = if (isSelected) {
                        { Icon(Icons.Filled.Check, contentDescription = "Selected") }
                    } else null,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            onSelect(item)
                            onDismiss()
                        },
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

