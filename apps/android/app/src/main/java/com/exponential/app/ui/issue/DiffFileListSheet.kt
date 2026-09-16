package com.exponential.app.ui.issue

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SheetHeight

// EXP-895/EXP-916 — the phone's file COLUMN. The web review page parks a file
// TREE beside the cards; a phone has no beside, so the same tree is a sheet
// reached from the work bar's LEADING slot (a circle wearing the files glyph
// and the file count). Picking a row closes the sheet and scrolls the page to
// that card, opened — the phone twin of the desktop IDE's `scroll_to_file`.
// The sheet is the frame; [DiffFileTree] is the whole of the content.

/** The filter field's placeholder — the contract's, byte-identical ×4. */
const val DIFF_FILTER_PLACEHOLDER: String = DomainContract.diffUiFilterPlaceholder

@Composable
fun DiffFileListSheet(
    files: List<Diff.File>,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(
        title = DomainContract.diffUiChangedFilesTitle,
        onDismiss = onDismiss,
        height = SheetHeight.Fitted,
        modifier = Modifier.testTag("changes-file-list"),
    ) {
        DiffFileTree(files = files, onPick = onPick)
    }
}
