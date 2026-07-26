package com.exponential.app.ui.components

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.icons.ExpIcons

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
