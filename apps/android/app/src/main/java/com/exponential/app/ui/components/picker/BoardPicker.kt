package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor

/**
 * EXP-1029 contract, EXP-1021 implementation — the board picker: every row
 * draws the board's ICON (`BoardIconUi`) and COLOUR, everywhere a board is
 * picked (the composer, the create-issue sheet, move-to-board).
 */
data class BoardPickerBoard(
    val id: String,
    val name: String,
    /**
     * Contract `boardIcon`, RESOLVED by the adapter (`toPickerBoard` via
     * `boardIconName`, fallback included); null = no glyph, which only a
     * non-board row such as an optional input's "None" reset carries.
     */
    val icon: String? = null,
    /** The board's hex colour; null = the foreground. */
    val colorHex: String? = null,
)

fun boardPickerItems(boards: List<BoardPickerBoard>): List<PickerItem<String>> =
    boards.map { board ->
        PickerItem(
            value = board.id,
            label = board.name,
            // A board row's glyph arrives resolved (`BoardIconUi` owns the
            // fallback rule); this only looks the name up in the registry.
            icon = board.icon?.let { ExpIcons.byName(it) },
            color = board.colorHex?.takeIf { it.isNotBlank() }?.let(::parseColor),
        )
    }

@Composable
fun BoardPicker(
    boards: List<BoardPickerBoard>,
    value: String?,
    onChange: (String) -> Unit,
    search: Boolean = true,
    /** The sheet headline; the default names the picker. */
    title: String = "Board",
    /**
     * EXP-1021: the sheet CONTROLLED by the caller, for a picker that is a
     * state machine rather than a chip (the issue screens open theirs from a
     * properties sheet that has already closed).
     */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = boardPickerItems(boards),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        search = search,
        emptyText = "No boards",
        title = title,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
