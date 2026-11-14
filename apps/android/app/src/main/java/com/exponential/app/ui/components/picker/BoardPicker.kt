package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.ui.components.rememberSidebarFolders
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
    /** #EXP-1832: the board's team (the member's folders are per team); null = no folders. */
    val teamId: String? = null,
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

/**
 * #EXP-1832 (sidebar-folders.json `rows`): the board rows inside the member's
 * READ-ONLY folders: a folder row (folder glyph, name, count while folded,
 * the chevron), its boards one level in. [rows] = `SidebarFolders.rows`.
 */
fun boardPickerGroupedItems(boards: List<BoardPickerBoard>, rows: List<SidebarFolders.Row>): List<PickerItem<String>> {
    val byId = boardPickerItems(boards).associateBy { it.value }
    return rows.mapNotNull { row ->
        if (row.group) {
            PickerItem(
                value = row.id,
                label = row.name ?: SidebarFolders.DEFAULT_FOLDER_NAME,
                icon = ExpIcons.uiFolder,
                folder = PickerFolder(count = row.count, open = row.open),
            )
        } else {
            byId[row.id]?.copy(depth = row.depth)
        }
    }
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
    // #EXP-1832: the member's folders of the boards' team (one team here).
    val teamId = boards.firstNotNullOfOrNull { it.teamId }?.takeIf { team -> boards.all { it.teamId == null || it.teamId == team } }
    val folders = rememberSidebarFolders(teamId, SidebarFolders.SECTION_BOARD, boards.map { it.id })
    val grouped = remember(boards, folders) {
        if (folders.isFlat) null else boardPickerGroupedItems(boards, folders.rows)
    }
    Picker(
        items = boardPickerItems(boards),
        groupedItems = grouped,
        onToggleGroup = folders.toggle,
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
