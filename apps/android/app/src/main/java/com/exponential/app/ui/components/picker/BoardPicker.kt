package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the board picker: every row draws the board's ICON
 * (`BoardIconUi`) and COLOUR, everywhere a board is picked (the composer,
 * the create-issue sheet, move-to-board, the share target). EXP-1021 fills
 * the rows over [Picker].
 */
data class BoardPickerBoard(
    val id: String,
    val name: String,
    /** Contract `boardIcon`; null = the default glyph. */
    val icon: String? = null,
    /** The board's hex colour; null = the foreground. */
    val colorHex: String? = null,
)

fun boardPickerItems(boards: List<BoardPickerBoard>): List<PickerItem<String>> =
    boards.map { board -> PickerItem(value = board.id, label = board.name) }

@Composable
fun BoardPicker(
    boards: List<BoardPickerBoard>,
    value: String?,
    onChange: (String) -> Unit,
    search: Boolean = true,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = boardPickerItems(boards),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        search = search,
        emptyText = "No boards",
        title = "Board",
        trigger = trigger,
    )
}
