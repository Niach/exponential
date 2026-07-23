package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Searchable multi-toggle label sheet (EXP-240): dot + name rows that toggle
 * without dismissing, plus a one-tap `+ Create new label "query"` row when the
 * query matches no existing name (case-insensitive exact) — color picked
 * deterministically via [LabelPalette.autoColor], no swatch strip. The
 * signature is unchanged (incl. `onCreate(name, color)`) so CreateIssueScreen
 * keeps compiling against it.
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

    val filtered = remember(teamLabels, query) {
        val q = query.trim()
        if (q.isEmpty()) teamLabels
        else teamLabels.filter { it.name.contains(q, ignoreCase = true) }
    }
    val trimmedQuery = query.trim()
    val hasExactMatch = remember(teamLabels, trimmedQuery) {
        teamLabels.any { it.name.equals(trimmedQuery, ignoreCase = true) }
    }

    GlassSheet(title = "Labels", onDismiss = onDismiss) {
        GlassSheetSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = "Search or create labels",
        )
        Spacer(Modifier.height(4.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 420.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            filtered.forEach { label ->
                val selected = label.id in selectedLabelIds
                GlassSheetRow(
                    label = label.name,
                    selected = selected,
                    leading = {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(parseColor(label.color), CircleShape),
                        )
                    },
                    // Multi-toggle: the sheet stays open across taps.
                    onClick = { onToggle(label.id, selected) },
                )
            }
            if (trimmedQuery.isNotEmpty() && !hasExactMatch) {
                GlassSheetRow(
                    label = "Create new label “$trimmedQuery”",
                    leading = {
                        Icon(
                            Icons.Filled.Add,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                        )
                    },
                    labelColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    onClick = {
                        onCreate(trimmedQuery, LabelPalette.autoColor(trimmedQuery))
                        query = ""
                    },
                )
            }
            if (filtered.isEmpty() && trimmedQuery.isEmpty()) {
                Text(
                    "No labels yet — type a name to create one.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 12.dp),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
