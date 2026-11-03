package com.exponential.app.ui.apps

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.domain.AppExport
import com.exponential.app.domain.AppTemplates
import com.exponential.app.domain.Apps
import com.exponential.app.domain.Chats
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// VAPP-110 (fixtures/apps.json THE LIST, web `components/apps/apps-list.tsx`):
// the Apps TAB = the band whose `New app` pill opens THE NEW MENU (VAPP-242:
// New app, Import…), then one row per app by name (glyph, name, the version
// pill, the `…` menu: Edit app, Export, Delete app); Exponential ships no
// built-in apps. VAPP-242: a row RUNS the app (THE RUN VIEW); the editor sits
// behind `…` › Edit app. VAPP-176: New app = `apps.create {teamId}` (a DRAFT)
// and its editor at once, no form; Import… opens the import step as a sheet.

/** An app's glyph: its icon in its colour (the board set), muted without one. */
@Composable
fun AppGlyph(icon: String?, color: String?, size: Dp = 16.dp, modifier: Modifier = Modifier) {
    val glyph = icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps
    val tint = color?.let { runCatching { parseColor(it) }.getOrNull() }
        ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Icon(glyph, contentDescription = null, tint = tint, modifier = modifier.size(size))
}

/** The version pill = `Apps.listPill` (eApps V1, #VAPP-210: the PUBLISHED
 *  version `v{n}`, or the muted `Draft` while nothing is published). */
@Composable
fun AppVersionPill(app: AppListEntry) {
    val versioned = app.publishedVersion > 0
    GlassPill(
        Apps.listPill(app.versions),
        size = PillSize.Sm,
        contentColor = if (versioned) null else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

@Composable
fun AppsScreen(
    /** VAPP-242: a row tap = THE RUN VIEW. */
    onRunApp: (appId: String) -> Unit,
    /** `…` › Edit app, New app, an import = THE EDITOR. */
    onOpenApp: (appId: String) -> Unit,
    viewModel: AppsViewModel = hiltViewModel(),
) {
    val apps by viewModel.apps.collectAsStateWithLifecycle()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    var importing by rememberSaveable { mutableStateOf(false) }
    var deleting by remember { mutableStateOf<AppListEntry?>(null) }
    var exporting by remember { mutableStateOf<AppListEntry?>(null) }

    val title = Chats.Copy.HEADERS.getValue("apps")
    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(bottom = BottomBarInset)
                .testTag(Apps.TestIds.LIST),
        ) {
            // #EXP-1384 (chats.json `shell.header`): the Apps tab's page header.
            PageHeader(title = title)
        Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
            AppsListContent(
                apps = apps,
                canManage = { viewModel.canManage(it, me, isOwner) },
                onOpen = { onRunApp(it.id) },
                onEdit = { onOpenApp(it.id) },
                onNew = { viewModel.create(onOpenApp) },
                onImport = { importing = true },
                creating = busy,
                onDelete = { deleting = it },
                onExport = { exporting = it },
            )
            Spacer(Modifier.height(24.dp))
        }
        }
    }

    if (importing) {
        AppImportSheet(
            busy = busy,
            onDismiss = { importing = false },
            onValidate = viewModel::validateImport,
            onImport = { text ->
                viewModel.importApp(text) { id ->
                    importing = false
                    onOpenApp(id)
                }
            },
            onTemplate = { template, includeRecords ->
                viewModel.importTemplate(template, includeRecords) { id ->
                    importing = false
                    onOpenApp(id)
                }
            },
        )
    }
    exporting?.let { app ->
        val records by remember(app.id) { viewModel.records(app.id) }.collectAsStateWithLifecycle(emptyList())
        AppExportSheet(app = app, records = records, onDismiss = { exporting = null })
    }
    deleting?.let { app ->
        GlassAlert(
            title = Apps.deleteTitle(app.name),
            body = Apps.Copy.DELETE_BODY,
            onDismiss = { if (!busy) deleting = null },
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { deleting = null }),
                GlassAlertAction(
                    Apps.Copy.DELETE,
                    destructive = true,
                    loading = busy,
                    testTag = "app-delete-confirm",
                    onClick = { viewModel.delete(app) { deleting = null } },
                ),
            ),
            defaultAction = 0,
        )
    }
}

/** The band + rows, stateless: null [apps] = loading (skeleton rows). */
@Composable
fun AppsListContent(
    apps: List<AppListEntry>?,
    canManage: (AppListEntry) -> Boolean,
    onOpen: (AppListEntry) -> Unit,
    onNew: () -> Unit,
    onDelete: (AppListEntry) -> Unit,
    /** VAPP-242: `…` › Edit app (the creator or an owner) = THE EDITOR. */
    onEdit: (AppListEntry) -> Unit = {},
    /** VAPP-176: THE NEW MENU's Import… (the import sheet). */
    onImport: () -> Unit = {},
    /** VAPP-176: New app's create in flight (the pill waits). */
    creating: Boolean = false,
    /** VAPP-121: any member exports an app that has a version. */
    onExport: (AppListEntry) -> Unit = {},
) {
    SectionHeader(Apps.Copy.TITLE) {
        AppsNewMenu(onNew = onNew, onImport = onImport, creating = creating)
    }
    when {
        apps == null -> SkeletonRows(modifier = Modifier.padding(vertical = 4.dp))
        apps.isEmpty() -> EmptyState(title = Apps.Copy.EMPTY, icon = ExpIcons.navApps, size = EmptyStateSize.List)
        else -> apps.forEach { app ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("${Apps.TestIds.ROW}-${app.slug}")
                    .flatRow()
                    .clickable { onOpen(app) }
                    .padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
            ) {
                AppGlyph(app.icon, app.color)
                Text(
                    app.name,
                    fontWeight = FontWeight.Medium,
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                AppVersionPill(app)
                val manage = canManage(app)
                if (manage || app.headVersion > 0) {
                    AppRowMenu(
                        app,
                        canManage = manage,
                        onEdit = { onEdit(app) },
                        onExport = { onExport(app) },
                        onDelete = { onDelete(app) },
                    )
                } else {
                    // The menu's slot keeps every pill on one column.
                    Spacer(Modifier.size(32.dp))
                }
            }
        }
    }
}

@Composable
private fun AppRowMenu(app: AppListEntry, canManage: Boolean, onEdit: () -> Unit, onExport: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            contentDescription = "Menu for ${app.name}",
            onClick = { open = true },
            glyphSize = 16.dp,
            borderless = true,
            size = 32.dp,
            modifier = Modifier.testTag("${Apps.TestIds.ROW}-menu-${app.slug}"),
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            // VAPP-242 (`run.menu`): Edit app first (the creator or an owner).
            if (canManage) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                    text = { Text(Apps.Copy.MENU_EDIT) },
                    onClick = {
                        open = false
                        onEdit()
                    },
                    modifier = Modifier.testTag("apps-row-edit"),
                )
            }
            if (app.headVersion > 0) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiShare, contentDescription = null) },
                    text = { Text(AppExport.Copy.EXPORT) },
                    onClick = {
                        open = false
                        onExport()
                    },
                )
            }
            if (canManage) {
            GlassMenuItem(
                leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                text = { Text(Apps.Copy.MENU_DELETE) },
                destructive = true,
                onClick = {
                    open = false
                    onDelete()
                },
            )
            }
        }
    }
}

/** VAPP-242 THE NEW MENU (`run.newMenu`, testId `newMenu`): the band's
 *  `New app` pill opens the existing menu: New app (ui-add), Import… (ui-file). */
@Composable
private fun AppsNewMenu(onNew: () -> Unit, onImport: () -> Unit, creating: Boolean) {
    var open by remember { mutableStateOf(false) }
    Box {
        GlassPill(
            Apps.Copy.NEW_APP,
            icon = ExpIcons.uiAdd,
            size = PillSize.Sm,
            onClick = { open = true },
            enabled = !creating,
            modifier = Modifier.testTag(Apps.TestIds.NEW_APP),
        )
        GlassDropdownMenu(
            expanded = open,
            onDismissRequest = { open = false },
            modifier = Modifier.testTag(Apps.TestIds.NEW_MENU),
        ) {
            GlassMenuItem(
                leadingIcon = { Icon(ExpIcons.uiAdd, contentDescription = null) },
                text = { Text(Apps.Copy.NEW_APP) },
                onClick = {
                    open = false
                    onNew()
                },
                modifier = Modifier.testTag("apps-new-menu-new"),
            )
            GlassMenuItem(
                leadingIcon = { Icon(ExpIcons.uiFile, contentDescription = null) },
                text = { Text(Apps.Copy.IMPORT) },
                onClick = {
                    open = false
                    onImport()
                },
                modifier = Modifier.testTag(Apps.TestIds.IMPORT_APP),
            )
        }
    }
}

/** VAPP-121 / VAPP-176: Import… = its own sheet, the import step: the
 *  monospace field, Choose file, the server's issues and Import. VAPP-234
 *  (fixtures/app-templates.json): the templates step above it (a row =
 *  [onTemplate] at once, the sample-records switch). */
@Composable
fun AppImportSheet(
    busy: Boolean,
    onDismiss: () -> Unit,
    onValidate: suspend (String) -> List<AppExport.Issue>?,
    onImport: (text: String) -> Unit,
    onTemplate: (template: String, includeRecords: Boolean) -> Unit = { _, _ -> },
) {
    // Plain remember: an import may be up to 4 MB, far past the Binder limit for saved state.
    var text by remember { mutableStateOf("") }
    val issues = rememberImportIssues(text, onValidate)
    GlassSheet(
        title = AppExport.Copy.IMPORT_TITLE,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("app-import-dialog"),
        primaryAction = SheetPrimaryAction(
            label = AppExport.Copy.IMPORT_ACTION,
            onClick = { if (issues?.isEmpty() == true) onImport(text) },
            enabled = issues?.isEmpty() == true && !busy,
            loading = busy,
            testTag = "app-import-confirm",
        ),
    ) {
        AppTemplatesStep(busy = busy, onTemplate = onTemplate)
        Spacer(Modifier.height(12.dp))
        AppImportStep(text = text, onTextChange = { text = it }, issues = issues)
        Spacer(Modifier.height(8.dp))
    }
}

/** VAPP-234: the templates step: the band, one row per signed template
 *  (glyph, name over the muted description), the sample-records switch. */
@Composable
fun AppTemplatesStep(busy: Boolean, onTemplate: (template: String, includeRecords: Boolean) -> Unit) {
    var withRecords by rememberSaveable { mutableStateOf(false) }
    Column(modifier = Modifier.fillMaxWidth()) {
        SectionHeader(AppTemplates.Copy.TITLE, modifier = Modifier.padding(horizontal = 16.dp))
        OptionGroup {
            AppTemplates.TEMPLATES.forEachIndexed { index, template ->
                if (index > 0) GroupDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("${AppTemplates.TestIds.ROW}-${template.id}")
                        .clickable(enabled = !busy) { onTemplate(template.id, withRecords) }
                        .padding(horizontal = 16.dp, vertical = 10.dp),
                ) {
                    AppGlyph(template.icon, template.color)
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            template.name,
                            fontWeight = FontWeight.Medium,
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            template.description,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        OptionGroup {
            SwitchRow(
                title = AppTemplates.Copy.INCLUDE_RECORDS,
                checked = withRecords,
                onCheckedChange = { withRecords = it },
                enabled = !busy,
                modifier = Modifier.testTag(AppTemplates.TestIds.RECORDS),
            )
        }
    }
}
