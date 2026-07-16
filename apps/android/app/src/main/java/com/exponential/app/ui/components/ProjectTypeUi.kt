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
import com.exponential.app.data.db.ProjectEntity

/**
 * Creation-template presentation metadata (the old dev/tasks/feedback project
 * TYPES survive only here, mirroring web `PROJECT_TEMPLATES`): each pre-sets the
 * public toggle + stored icon and whether the create form leads with the repo
 * picker. Every resulting project is the same shape — repo optional, publicness
 * a toggle.
 */
data class ProjectTemplate(
    val label: String,
    val description: String,
    val icon: ImageVector,
    /** Pre-set publicness — feedback boards are anonymously readable. */
    val isPublic: Boolean,
    /** Curated icon name (contract projectIconValues) stored on the project. */
    val iconName: String,
    /** Whether the form opens with the repository section shown (repo optional). */
    val suggestsRepo: Boolean,
)

val ProjectTemplates: List<ProjectTemplate> = listOf(
    ProjectTemplate(
        label = "Dev board",
        description = "Connect a GitHub repo — branches, PRs and coding sessions.",
        icon = Icons.Filled.Code,
        isPublic = false,
        iconName = "code",
        suggestsRepo = true,
    ),
    ProjectTemplate(
        label = "Task board",
        description = "Plain issue tracking — no repository needed.",
        icon = Icons.Filled.ViewKanban,
        isPublic = false,
        iconName = "square-kanban",
        suggestsRepo = false,
    ),
    ProjectTemplate(
        label = "Feedback board",
        description = "Public, read-only board — collect feedback with the widget.",
        icon = Icons.Filled.Campaign,
        isPublic = true,
        iconName = "megaphone",
        suggestsRepo = false,
    ),
)

/**
 * Curated icon set (contract projectIconValues) → Material glyphs, in the
 * picker's display order. Mirrors web `PROJECT_ICON_COMPONENTS` with Material
 * equivalents. Every contract name maps.
 */
val ProjectIconGlyphs: Map<String, ImageVector> = linkedMapOf(
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
 * Resolve a project's display glyph: the stored `icon` when it's a known
 * curated name, else a fallback derived from the project's shape (pre-collapse
 * rows have icon = NULL). Mirrors web `getProjectIcon` now that `type` is gone:
 * public → megaphone, repo-backed → code, else the plain task board.
 */
fun projectIcon(project: ProjectEntity): ImageVector =
    project.icon?.let { ProjectIconGlyphs[it] } ?: when {
        project.isPublic -> Icons.Filled.Campaign
        project.repositoryId != null -> Icons.Filled.Code
        else -> Icons.Filled.ViewKanban
    }
