package com.exponential.app.ui.share

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.components.picker.boardPickerItems
import com.exponential.app.ui.components.toPickerBoard
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

/**
 * Share-destination selector card (EXP-60): a compact glass row at the TOP of
 * the share compose form showing where the shared content will land — the
 * board's own icon + name over its team name, with an unfold affordance.
 * Tapping opens [ShareBoardPickerSheet]. Replaces the old always-expanded
 * inline board list that used to sit at the bottom of the form.
 */
@Composable
fun ShareBoardSelector(
    groups: List<TeamBoards>,
    selectedBoardId: String?,
    loading: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Text(
            "Share to",
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.padding(horizontal = 4.dp),
        )
        Spacer(Modifier.height(8.dp))
        when {
            groups.isEmpty() && loading -> {
                // Placeholder while the board list loads — keeps the card's
                // footprint so the form doesn't jump when the row fills in.
                Text(
                    "Loading boards…",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassGroup()
                        .padding(horizontal = 16.dp, vertical = 14.dp),
                )
            }
            groups.isEmpty() -> {
                // No shareable boards — without this the share is a silent
                // dead end: an editable form whose Create button can never
                // enable.
                Text(
                    "Open Exponential to create your first board, then try sharing again.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassGroup()
                        .padding(horizontal = 16.dp, vertical = 14.dp),
                )
            }
            else -> {
                val selected = groups.firstNotNullOfOrNull { group ->
                    group.boards.firstOrNull { it.id == selectedBoardId }?.let { group to it }
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassGroup()
                        .clickable(onClick = onClick)
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    if (selected != null) {
                        BoardIcon(selected.second)
                        Spacer(Modifier.width(10.dp))
                    }
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            selected?.second?.name ?: "Choose a board",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = if (selected != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                            ),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (selected != null) {
                            Text(
                                selected.first.team.name,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    Spacer(Modifier.width(8.dp))
                    Icon(
                        ExpIcons.uiSelector,
                        contentDescription = "Change board",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
            }
        }
    }
}

/**
 * The share composer's board picker, opened by [ShareBoardSelector].
 *
 * EXP-1021 swept it onto the shared [Picker]: it used to draw `GlassSheetRow`s
 * with a trailing check, so the share flow said "this is picked" one way here
 * and another way in every picker it opens next.
 *
 * It rides the PRIMITIVE with [boardPickerItems]' rows rather than
 * [com.exponential.app.ui.components.picker.BoardPicker], because the share
 * target is the one board list that spans TEAMS and the shared
 * `BoardPickerBoard` contract (web `board-picker.tsx`, iOS, desktop) carries no
 * team. A picker row is FLAT — the contract has no section header — so the team
 * rides where a picker row says that kind of thing: the muted second line, and
 * a search keyword. `PickerContractTest` records that exception.
 */
@Composable
fun ShareBoardPickerSheet(
    groups: List<TeamBoards>,
    selectedBoardId: String?,
    onSelect: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val rows = remember(groups) {
        groups.flatMap { group ->
            boardPickerItems(group.boards.map { it.toPickerBoard() }).map { row ->
                row.copy(
                    description = group.team.name,
                    keywords = listOf(row.label, group.team.name),
                )
            }
        }
    }
    Picker(
        items = rows,
        mode = PickerMode.Single,
        value = setOfNotNull(selectedBoardId),
        onChange = { picked -> picked.firstOrNull()?.let(onSelect) },
        search = true,
        emptyText = "No boards",
        title = "Share to",
        searchPlaceholder = "Search boards",
        open = true,
        onOpenChange = { next -> if (!next) onDismiss() },
    )
}
