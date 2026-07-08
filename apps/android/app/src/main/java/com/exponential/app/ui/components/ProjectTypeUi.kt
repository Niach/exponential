package com.exponential.app.ui.components

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Campaign
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.ViewKanban
import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.domain.DomainContract

/**
 * Presentation metadata for the three project board types (contract
 * projectTypeValues). Shared by the create form, project rows and switcher so
 * the icon/label/description stay consistent. Mirrors the web `Code2` /
 * `SquareKanban` / `Megaphone` mapping with Material equivalents.
 */
data class ProjectTypeInfo(
    val type: String,
    val label: String,
    val description: String,
    val icon: ImageVector,
    /** True for feedback boards — they carry a public affordance. */
    val isPublic: Boolean,
)

val ProjectTypeInfos: List<ProjectTypeInfo> = listOf(
    ProjectTypeInfo(
        type = DomainContract.projectTypeDev,
        label = "Dev board",
        description = "Code with agents, PRs and a GitHub repository.",
        icon = Icons.Filled.Code,
        isPublic = false,
    ),
    ProjectTypeInfo(
        type = DomainContract.projectTypeTasks,
        label = "Task board",
        description = "Plain issue tracking — no repository needed.",
        icon = Icons.Filled.ViewKanban,
        isPublic = false,
    ),
    ProjectTypeInfo(
        type = DomainContract.projectTypeFeedback,
        label = "Feedback board",
        description = "A public, read-only board for collecting feedback.",
        icon = Icons.Filled.Campaign,
        isPublic = true,
    ),
)

/** The presentation info for a raw type string, defaulting to Dev for unknowns. */
fun projectTypeInfo(type: String?): ProjectTypeInfo =
    ProjectTypeInfos.firstOrNull { it.type == type } ?: ProjectTypeInfos.first()
