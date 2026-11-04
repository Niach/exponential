package com.exponential.app.ui.apps

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.LocalDetailHaze
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.work.WorkTopBar
import dev.chrisbanes.haze.rememberHazeState

// VAPP-242 (fixtures/apps.json THE RUN VIEW, web `components/apps/app-run-view.tsx`):
// an app as a member uses it, FULL SCREEN, pushed over the Apps tab. It runs
// the PUBLISHED package (`apps.package`, never the draft) through THE SAME
// app host as the editor's preview (`AppPackageSurface`: records with `me`,
// the published grants, read-only access, the HostBanner, the consent card,
// the screens bar). No toolbar, no frame. The header = the detail header:
// back, the app's glyph + name and the `…` (`run.menu`): Edit app (the
// creator or an owner) → THE EDITOR, Export (published > 0), Delete app
// (the creator or an owner). Nothing published = the page-size EmptyState
// `copy.runEmpty`. `app.navigate` keeps the screen in memory (the view
// model's request); an `appScreen` link opens here on that screen.

@Composable
fun AppRunScreen(
    onBack: () -> Unit,
    onEdit: (appId: String) -> Unit,
    onOpenTeamSettings: (teamId: String, section: String) -> Unit = { _, _ -> },
    // The editor's view model reads the app, access and screen request; the
    // run view never calls `leave()` (it starts no editor session).
    viewModel: AppEditorViewModel = hiltViewModel(),
) {
    val app by viewModel.app.collectAsStateWithLifecycle()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val access by viewModel.myAccess.collectAsStateWithLifecycle()
    val screenRequest by viewModel.screen.collectAsStateWithLifecycle()
    val records by viewModel.records.collectAsStateWithLifecycle()
    val deleting by viewModel.deleting.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    var exportOpen by rememberSaveable { mutableStateOf(false) }
    var deleteOpen by rememberSaveable { mutableStateOf(false) }

    val entry = app
    val canManage = entry != null && (isOwner || (me != null && entry.createdBy == me))
    CompositionLocalProvider(LocalDetailHaze provides rememberHazeState()) {
        Scaffold(
            containerColor = Color.Transparent,
            modifier = Modifier.testTag(Apps.TestIds.RUN),
            topBar = {
                WorkTopBar(
                    title = entry?.name.orEmpty(),
                    titleContent = {
                        if (entry != null) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                AppGlyph(entry.icon, entry.color, size = 20.dp)
                                Text(entry.name, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                            }
                        }
                    },
                    onBack = onBack,
                    verb = null,
                    verbEnabled = false,
                    onVerb = {},
                    menu = if (entry != null && (canManage || entry.publishedVersion > 0)) {
                        {
                            AppRunMenu(
                                canEdit = canManage,
                                canExport = entry.publishedVersion > 0,
                                canDelete = canManage,
                                onEdit = { onEdit(entry.id) },
                                onExport = { exportOpen = true },
                                onDelete = { deleteOpen = true },
                            )
                        }
                    } else {
                        null
                    },
                )
            },
        ) { padding ->
            Box(Modifier.padding(padding).fillMaxSize()) {
                val pkg = entry?.packageJson
                when {
                    entry == null -> SkeletonRows(modifier = Modifier.align(Alignment.TopCenter).padding(16.dp))
                    pkg == null || entry.publishedVersion == 0 -> EmptyState(
                        title = Apps.Copy.RUN_EMPTY,
                        icon = entry.icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps,
                        size = EmptyStateSize.Page,
                        modifier = Modifier.align(Alignment.Center),
                    )
                    // A new published package = a new host (the key); the
                    // screen request outlives it (`resolveScreen`).
                    else -> key(pkg) {
                        AppPackageSurface(
                            packageJson = pkg,
                            onOpenTeamSettings = onOpenTeamSettings,
                            appId = viewModel.appId,
                            teamId = entry.teamId,
                            me = me,
                            grants = true,
                            readOnly = access == "read",
                            screen = screenRequest,
                            onScreen = viewModel::requestScreen,
                        )
                    }
                }
            }
        }
    }
    if (exportOpen && entry != null) {
        AppExportSheet(app = entry, records = records, onDismiss = { exportOpen = false })
    }
    if (deleteOpen && entry != null) {
        GlassAlert(
            title = Apps.deleteTitle(entry.name),
            body = Apps.Copy.DELETE_BODY,
            onDismiss = { if (!deleting) deleteOpen = false },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { deleteOpen = false }),
                GlassAlertAction(
                    Apps.Copy.DELETE,
                    destructive = true,
                    loading = deleting,
                    testTag = "app-delete-confirm",
                    onClick = {
                        viewModel.delete {
                            deleteOpen = false
                            onBack()
                        }
                    },
                ),
            ),
            defaultAction = 0,
        )
    }
}

/** VAPP-242 `run.menu`: Edit app, Export, Delete app (in that order). */
@Composable
private fun AppRunMenu(
    canEdit: Boolean,
    canExport: Boolean,
    canDelete: Boolean,
    onEdit: () -> Unit,
    onExport: () -> Unit,
    onDelete: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            contentDescription = "App actions",
            onClick = { open = true },
            borderless = true,
            modifier = Modifier.padding(end = 8.dp).testTag(Apps.TestIds.RUN_MENU),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            if (canEdit) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                    text = { Text(Apps.Copy.MENU_EDIT) },
                    onClick = {
                        open = false
                        onEdit()
                    },
                    modifier = Modifier.testTag("app-run-edit"),
                )
            }
            if (canExport) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiShare, contentDescription = null) },
                    text = { Text(Apps.Copy.MENU_EXPORT) },
                    onClick = {
                        open = false
                        onExport()
                    },
                    modifier = Modifier.testTag("app-run-export"),
                )
            }
            if (canDelete) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                    text = { Text(Apps.Copy.MENU_DELETE) },
                    destructive = true,
                    onClick = {
                        open = false
                        onDelete()
                    },
                    modifier = Modifier.testTag("app-run-delete"),
                )
            }
        }
    }
}
