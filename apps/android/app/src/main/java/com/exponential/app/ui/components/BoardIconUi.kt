package com.exponential.app.ui.components

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor

/**
 * Resolve a board's display glyph: the stored `icon` when it's a known name in
 * the shared Lucide registry (EXP-273 — the same art web/iOS/desktop render),
 * else a fallback derived from the board's shape (pre-collapse rows have
 * icon = NULL). Mirrors web `getBoardIcon` now that `type` is gone: repo-backed
 * → code, else the plain kanban board.
 */
fun boardIcon(board: BoardEntity): ImageVector =
    board.icon?.let { ExpIcons.byName(it) } ?: when {
        // The two fallbacks name their glyphs DIRECTLY rather than borrowing a
        // concept that happens to share the art today — web, iOS and desktop
        // all hard-code `code`/`square-kanban` here, and re-pointing an
        // unrelated concept must not silently change what a board draws.
        board.repositoryId != null -> ExpIcons.`code`
        else -> ExpIcons.`square-kanban`
    }

/**
 * A board's glyph tinted with its own color — the ONE way a board is drawn
 * (EXP-449): every picker, group header and board row shows the same
 * icon+color pair instead of an anonymous dot or a generic boards glyph.
 */
@Composable
fun BoardIcon(
    board: BoardEntity,
    modifier: Modifier = Modifier,
    size: Dp = 16.dp,
) {
    val color = remember(board.color) { parseColor(board.color) }
    val icon = remember(board.icon, board.repositoryId) { boardIcon(board) }
    Icon(
        icon,
        contentDescription = null,
        tint = color,
        modifier = modifier.size(size),
    )
}
