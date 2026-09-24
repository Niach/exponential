package com.exponential.app.ui.issue

import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.size
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.picker.LabelPicker
import com.exponential.app.ui.components.toPickerLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Searchable multi-toggle label sheet: dot + name rows that toggle without
 * dismissing, plus a one-tap `+ Create new label "query"` row when the query
 * matches no existing name (case-insensitive exact) — color picked
 * deterministically via [LabelPalette.autoColor], no swatch strip. The
 * signature is unchanged (incl. `onCreate(name, color)`) so the create screen
 * and the issue properties keep compiling against it.
 *
 * EXP-1021: the rows, the search field and the sheet are the shared
 * [LabelPicker]'s; what stays here is the CREATE row, which is this surface's
 * own affordance and rides as the picker's footer.
 */
@Composable
fun LabelPickerSheet(
    teamLabels: List<LabelEntity>,
    selectedLabelIds: Set<String>,
    onToggle: (String, Boolean) -> Unit,
    onCreate: (String, String) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val trimmedQuery = query.trim()
    val hasExactMatch = remember(teamLabels, trimmedQuery) {
        teamLabels.any { it.name.equals(trimmedQuery, ignoreCase = true) }
    }

    LabelPicker(
        labels = teamLabels.map { it.toPickerLabel() },
        value = selectedLabelIds,
        // The picker reports the whole new set; this surface writes one
        // toggle at a time, so the difference IS the tapped label.
        onChange = { next ->
            val added = next - selectedLabelIds
            val removed = selectedLabelIds - next
            added.forEach { onToggle(it, false) }
            removed.forEach { onToggle(it, true) }
        },
        query = query,
        onQueryChange = { query = it },
        emptyText = if (trimmedQuery.isEmpty()) "No labels yet. Type a name to create one." else "No matching labels",
        footer = if (trimmedQuery.isNotEmpty() && !hasExactMatch) {
            {
                GlassSheetRow(
                    label = "Create new label “$trimmedQuery”",
                    leading = {
                        Icon(
                            ExpIcons.uiAdd,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                        )
                    },
                    labelColor = Color.White.copy(alpha = TextEmphasis.Secondary),
                    onClick = {
                        onCreate(trimmedQuery, LabelPalette.autoColor(trimmedQuery))
                        query = ""
                    },
                )
            }
        } else {
            null
        },
        open = true,
        onOpenChange = { open -> if (!open) onDismiss() },
    )
}
