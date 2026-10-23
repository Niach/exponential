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
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
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
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SheetPrimaryAction
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// VAPP-110 (fixtures/apps.json, web `components/apps/apps-list.tsx`): the
// Apps list = the band with `New app` as its trailing pill, then one row per
// app (glyph, name, the version pill, the `…` menu: Rename, Delete); the
// built-in app first with its `Built-in` pill and no menu. A row opens the
// editor. New app / Rename = the board form's identity row (icon + colour +
// name) and the optional description, in a sheet.

private const val DEFAULT_ICON = "puzzle"
private const val DEFAULT_COLOR = "#6366f1"

/** An app's glyph: its icon in its colour (the board set), the built-in one muted. */
@Composable
fun AppGlyph(icon: String?, color: String?, size: Dp = 16.dp, modifier: Modifier = Modifier) {
    val glyph = icon?.let { ExpIcons.byName(it) } ?: ExpIcons.navApps
    val tint = color?.let { runCatching { parseColor(it) }.getOrNull() }
        ?: MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Icon(glyph, contentDescription = null, tint = tint, modifier = modifier.size(size))
}

/** The version pill: `Built-in`, `v{n}`, or the muted "no version" copy. */
@Composable
fun AppVersionPill(app: AppListEntry) {
    val versioned = app.builtin || app.headVersion > 0
    GlassPill(
        if (app.builtin) Apps.Copy.BUILTIN else Apps.versionLabel(app.headVersion),
        size = PillSize.Sm,
        contentColor = if (versioned) null else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppsScreen(
    onBack: () -> Unit,
    onOpenApp: (appId: String) -> Unit,
    viewModel: AppsViewModel = hiltViewModel(),
) {
    val apps by viewModel.apps.collectAsStateWithLifecycle()
    val me by viewModel.me.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val busy by viewModel.busy.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)

    var creating by rememberSaveable { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<AppListEntry?>(null) }
    var deleting by remember { mutableStateOf<AppListEntry?>(null) }

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {},
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .testTag(Apps.TestIds.LIST),
        ) {
            AppsListContent(
                apps = apps,
                canManage = { viewModel.canManage(it, me, isOwner) },
                onOpen = { onOpenApp(it.id) },
                onNew = { creating = true },
                onRename = { renaming = it },
                onDelete = { deleting = it },
            )
            Spacer(Modifier.height(24.dp))
        }
    }

    if (creating) {
        AppFormSheet(
            app = null,
            busy = busy,
            onDismiss = { creating = false },
            onSubmit = { name, icon, color, description ->
                viewModel.create(name, icon, color, description) { id ->
                    creating = false
                    onOpenApp(id)
                }
            },
        )
    }
    renaming?.let { app ->
        AppFormSheet(
            app = app,
            busy = busy,
            onDismiss = { renaming = null },
            onSubmit = { name, icon, color, description ->
                viewModel.rename(app, name, icon, color, description) { renaming = null }
            },
        )
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
    onRename: (AppListEntry) -> Unit,
    onDelete: (AppListEntry) -> Unit,
) {
    SectionHeader(Apps.Copy.TITLE) {
        GlassPill(
            Apps.Copy.NEW_APP,
            icon = ExpIcons.uiAdd,
            size = PillSize.Sm,
            onClick = onNew,
            modifier = Modifier.testTag(Apps.TestIds.NEW_APP),
        )
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
                if (canManage(app)) {
                    AppRowMenu(app, onRename = { onRename(app) }, onDelete = { onDelete(app) })
                } else {
                    // The menu's slot keeps every pill on one column.
                    Spacer(Modifier.size(32.dp))
                }
            }
        }
    }
}

@Composable
private fun AppRowMenu(app: AppListEntry, onRename: () -> Unit, onDelete: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        CircleIconButton(
            ExpIcons.uiMore,
            contentDescription = "Menu for ${app.name}",
            onClick = { open = true },
            glyphSize = 16.dp,
            borderless = true,
            size = 32.dp,
        )
        GlassDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            GlassMenuItem(
                leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                text = { Text(Apps.Copy.MENU_RENAME) },
                onClick = {
                    open = false
                    onRename()
                },
            )
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

/** New app (null [app]) or Rename: the identity row + the description. */
@Composable
fun AppFormSheet(
    app: AppListEntry?,
    busy: Boolean,
    onDismiss: () -> Unit,
    onSubmit: (name: String, icon: String, color: String, description: String) -> Unit,
) {
    var name by rememberSaveable { mutableStateOf(app?.name.orEmpty()) }
    var icon by rememberSaveable { mutableStateOf(app?.icon ?: DEFAULT_ICON) }
    var color by rememberSaveable { mutableStateOf(app?.color ?: DEFAULT_COLOR) }
    var description by rememberSaveable { mutableStateOf(app?.description.orEmpty()) }
    GlassSheet(
        title = if (app == null) Apps.Copy.NEW_APP else Apps.Copy.MENU_RENAME,
        onDismiss = onDismiss,
        modifier = Modifier.testTag("app-form-dialog"),
        primaryAction = SheetPrimaryAction(
            label = if (app == null) Apps.Copy.CREATE else Apps.Copy.RENAME,
            onClick = { if (name.isNotBlank()) onSubmit(name, icon, color, description) },
            enabled = name.isNotBlank() && !busy,
            loading = busy,
            testTag = "app-form-submit",
        ),
    ) {
        OptionGroup {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, top = 6.dp, bottom = 6.dp),
            ) {
                IconPicker(selected = icon, onSelect = { icon = it }, accentColor = parseColor(color))
                ColorPicker(selected = color, onSelect = { color = it })
                GlassTextField(
                    value = name,
                    onValueChange = { name = it.take(Apps.NAME_MAX) },
                    placeholder = Apps.Copy.NAME_PLACEHOLDER,
                    singleLine = true,
                    bordered = false,
                    containerColor = Color.Transparent,
                    modifier = Modifier.weight(1f).testTag("app-form-name"),
                )
            }
        }
        Spacer(Modifier.height(12.dp))
        GlassTextField(
            value = description,
            onValueChange = { description = it.take(Apps.DESCRIPTION_MAX) },
            placeholder = Apps.Copy.DESCRIPTION_PLACEHOLDER,
            minLines = 3,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp).testTag("app-form-description"),
        )
        Spacer(Modifier.height(8.dp))
    }
}
