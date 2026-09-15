package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.Diff
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.SheetHeight
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSectionBand

// EXP-895 — the phone's file COLUMN. The web review page parks a `FileDiffNav`
// beside the cards; a phone has no beside, so the same list is a sheet reached
// from the work bar's LEADING slot (a circle wearing the files glyph and the
// file count). Picking a row closes the sheet and scrolls the page to that
// card, opened — the phone twin of the desktop IDE's `scroll_to_file`.

/** The filter field's placeholder, byte-identical to web `DIFF_FILTER_PLACEHOLDER`. */
const val DIFF_FILTER_PLACEHOLDER = "Filter files"

@Composable
fun DiffFileListSheet(
    files: List<Diff.File>,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var filter by remember { mutableStateOf("") }
    // The header counts the WHOLE diff, never the filtered slice: it is the
    // review's summary line, not a search result count.
    val summary = remember(files) {
        val totals = Diff.totals(files)
        Diff.summaryLabel(totals.files, totals.additions, totals.deletions)
    }
    val shown = remember(files, filter) { filterDiffFiles(files, filter) }
    GlassSheet(
        title = "Changed files",
        onDismiss = onDismiss,
        height = SheetHeight.Fitted,
        modifier = Modifier.testTag("changes-file-list"),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp)
                .glassSectionBand()
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(
                summary,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Spacer(Modifier.height(8.dp))
        GlassSheetSearchField(
            value = filter,
            onValueChange = { filter = it },
            placeholder = DIFF_FILTER_PLACEHOLDER,
            modifier = Modifier.testTag("diff-nav-filter"),
        )
        Spacer(Modifier.height(4.dp))
        LazyColumn(modifier = Modifier.fillMaxWidth()) {
            items(shown.size, key = { "diff_nav_$it" }) { index ->
                val file = shown[index]
                DiffFileRow(file = file, onClick = { onPick(file.path) })
            }
        }
    }
}
