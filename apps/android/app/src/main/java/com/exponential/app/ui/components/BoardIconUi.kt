package com.exponential.app.ui.components

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.Bolt
import androidx.compose.material.icons.filled.BugReport
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Campaign
import androidx.compose.material.icons.filled.ChatBubble
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Inventory2
import androidx.compose.material.icons.filled.Lightbulb

import androidx.compose.material.icons.filled.Public
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material.icons.filled.ViewKanban
import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.data.db.BoardEntity

/**
 * Curated icon set (contract boardIconValues) → Material glyphs, in the
 * picker's display order. Mirrors web `BOARD_ICON_COMPONENTS` with Material
 * equivalents. Every contract name maps.
 */
val BoardIconGlyphs: Map<String, ImageVector> = linkedMapOf(
    "code" to Icons.Filled.Code,
    "square-kanban" to Icons.Filled.ViewKanban,
    "megaphone" to Icons.Filled.Campaign,
    "bug" to Icons.Filled.BugReport,
    "rocket" to Icons.Filled.RocketLaunch,
    "book-open" to Icons.AutoMirrored.Filled.MenuBook,
    "globe" to Icons.Filled.Public,
    "heart" to Icons.Filled.Favorite,
    "star" to Icons.Filled.Star,
    "zap" to Icons.Filled.Bolt,
    "wrench" to Icons.Filled.Build,
    "shield" to Icons.Filled.Shield,
    "package" to Icons.Filled.Inventory2,
    "terminal" to Icons.Filled.Terminal,
    "lightbulb" to Icons.Filled.Lightbulb,
    "message-circle" to Icons.Filled.ChatBubble,
)

/**
 * Resolve a board's display glyph: the stored `icon` when it's a known
 * curated name, else a fallback derived from the board's shape (pre-collapse
 * rows have icon = NULL). Mirrors web `getBoardIcon` now that `type` is gone:
 * repo-backed → code, else the plain kanban board.
 */
fun boardIcon(board: BoardEntity): ImageVector =
    board.icon?.let { BoardIconGlyphs[it] } ?: when {
        board.repositoryId != null -> Icons.Filled.Code
        else -> Icons.Filled.ViewKanban
    }
