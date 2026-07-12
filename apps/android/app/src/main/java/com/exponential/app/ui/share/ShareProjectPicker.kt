package com.exponential.app.ui.share

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.UnfoldMore
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSection

/**
 * Share-destination selector card (EXP-60): a compact glass row at the TOP of
 * the share compose form showing where the shared content will land — project
 * color dot + project name over its workspace name, with an unfold affordance.
 * Tapping opens [ShareProjectPickerSheet]. Replaces the old always-expanded
 * inline project list that used to sit at the bottom of the form.
 */
@Composable
fun ShareProjectSelector(
    groups: List<WorkspaceProjects>,
    selectedProjectId: String?,
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
                // Placeholder while the project list loads — keeps the card's
                // footprint so the form doesn't jump when the row fills in.
                Text(
                    "Loading projects…",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .padding(horizontal = 16.dp, vertical = 14.dp),
                )
            }
            groups.isEmpty() -> {
                // No shareable projects — without this the share is a silent
                // dead end: an editable form whose Create button can never
                // enable.
                Text(
                    "Open Exponential to create your first project, then try sharing again.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .padding(horizontal = 16.dp, vertical = 14.dp),
                )
            }
            else -> {
                val selected = groups.firstNotNullOfOrNull { group ->
                    group.projects.firstOrNull { it.id == selectedProjectId }?.let { group to it }
                }
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .glassSection()
                        .clickable(onClick = onClick)
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                ) {
                    if (selected != null) {
                        Box(
                            modifier = Modifier
                                .size(10.dp)
                                .background(parseColor(selected.second.color), CircleShape),
                        )
                        Spacer(Modifier.width(10.dp))
                    }
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            selected?.second?.name ?: "Choose a project",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface.copy(
                                alpha = if (selected != null) TextEmphasis.Primary else TextEmphasis.Tertiary,
                            ),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (selected != null) {
                            Text(
                                selected.first.workspace.name,
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                    Spacer(Modifier.width(8.dp))
                    Icon(
                        Icons.Filled.UnfoldMore,
                        contentDescription = "Change project",
                        modifier = Modifier.size(18.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
            }
        }
    }
}

/**
 * Workspace-grouped project picker sheet for the share composer — the same
 * Material 3 chooser pattern as [com.exponential.app.ui.issue.IssuePickerSheet]
 * (title + [ListItem] rows + trailing check), with a secondary workspace header
 * above each group's projects.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareProjectPickerSheet(
    groups: List<WorkspaceProjects>,
    selectedProjectId: String?,
    onSelect: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        dragHandle = { BottomSheetDefaults.DragHandle() },
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(bottom = 12.dp),
        ) {
            Text(
                text = "Share to",
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp),
            )
            groups.forEach { group ->
                Text(
                    group.workspace.name,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 8.dp, bottom = 2.dp),
                )
                group.projects.forEach { project ->
                    val isSelected = project.id == selectedProjectId
                    ListItem(
                        headlineContent = {
                            Text(project.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        },
                        leadingContent = {
                            Box(
                                modifier = Modifier
                                    .size(10.dp)
                                    .background(parseColor(project.color), CircleShape),
                            )
                        },
                        trailingContent = if (isSelected) {
                            { Icon(Icons.Filled.Check, contentDescription = "Selected") }
                        } else null,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                onSelect(project.id)
                                onDismiss()
                            },
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
