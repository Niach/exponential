package com.exponential.app.ui.apps

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.json.JsonValue
import com.exponential.app.domain.AppActions
import com.exponential.app.domain.AppRecords
import com.exponential.app.domain.Apps
import com.exponential.app.ui.components.ColorPicker
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.OptionGroup
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis

// VAPP-177 (fixtures/apps.json THE SETTINGS TAB, web
// `components/apps/app-settings-tab.tsx`): phones open the editor's Settings
// tab as the `…` menu's sheet, grants included: the identity row (icon +
// colour + name, the name a deferred write on focus loss / Done), the
// description (saved on focus loss), the records line (app-records.json
// `recordsCountLabel`), the Tools group (app-actions.json: one row per
// function the head package declares, a Switch for a team OWNER only, a
// destructive action `Always asks`), then Export (a version exists) and
// Delete app (the creator or an owner). Everyone else reads it.

private const val DEFAULT_COLOR = "#6366f1"

@Composable
internal fun AppSettingsBody(
    app: AppListEntry,
    viewModel: AppEditorViewModel,
    records: Int,
    canManage: Boolean,
    canGrant: Boolean,
    onExport: () -> Unit,
    onDelete: () -> Unit,
) {
    val granted by viewModel.grantedTools.collectAsStateWithLifecycle()
    val grantBusy by viewModel.grantBusy.collectAsStateWithLifecycle()
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val focus = LocalFocusManager.current
    // Seeded per APP, never on a resync of the same one.
    var name by rememberSaveable(app.id) { mutableStateOf(app.name) }
    var icon by rememberSaveable(app.id) { mutableStateOf(app.icon) }
    var color by rememberSaveable(app.id) { mutableStateOf(app.color ?: DEFAULT_COLOR) }
    var description by rememberSaveable(app.id) { mutableStateOf(app.description) }
    val saveName = {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) name = app.name else if (trimmed != app.name) viewModel.update(name = trimmed)
    }
    val saveDescription = { if (description != app.description) viewModel.update(description = description) }
    // VAPP-120: the typed actions the HEAD package declares; FEED-159: a
    // stored grant that is no longer grantable reads as none.
    val declared = remember(app.packageJson) {
        val functions = app.packageJson
            ?.let { runCatching { JsonValue.parse(it) }.getOrNull() }
            ?.get("functions")?.array?.mapNotNull { it.string }
            .orEmpty()
        AppActions.declaredActions(functions)
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .testTag(Apps.TestIds.SETTINGS),
    ) {
        OptionGroup {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth().padding(start = 12.dp, top = 6.dp, bottom = 6.dp),
            ) {
                if (canManage) {
                    IconPicker(
                        selected = icon,
                        onSelect = { next ->
                            icon = next
                            viewModel.update(icon = next)
                        },
                        accentColor = parseColor(color),
                    )
                    ColorPicker(
                        selected = color,
                        onSelect = { next ->
                            color = next
                            viewModel.update(color = next)
                        },
                    )
                } else {
                    Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { AppGlyph(app.icon, app.color, size = 20.dp) }
                }
                GlassTextField(
                    value = name,
                    onValueChange = { name = it.take(Apps.NAME_MAX) },
                    placeholder = Apps.Copy.NAME_PLACEHOLDER,
                    enabled = canManage,
                    singleLine = true,
                    bordered = false,
                    containerColor = Color.Transparent,
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                    modifier = Modifier
                        .weight(1f)
                        .testTag("app-settings-name")
                        .commitOnBlur(saveName),
                )
            }
        }
        Spacer(Modifier.height(12.dp))
        OptionGroup {
            GlassTextField(
                value = description,
                onValueChange = { description = it.take(Apps.DESCRIPTION_MAX) },
                placeholder = Apps.Copy.DESCRIPTION_PLACEHOLDER,
                minLines = 3,
                enabled = canManage,
                bordered = false,
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("app-settings-description")
                    .commitOnBlur(saveDescription),
            )
        }
        // VAPP-122: the count; no plan quota on phones (null = the plain count).
        Text(
            AppRecords.recordsCountLabel(records, null),
            style = MaterialTheme.typography.bodyMedium,
            color = muted,
            modifier = Modifier.padding(start = 20.dp, end = 20.dp, top = 8.dp).testTag("app-settings-records"),
        )
        Spacer(Modifier.height(16.dp))

        // ── Tools (VAPP-120) ─────────────────────────────────────────────
        Column(Modifier.padding(horizontal = 20.dp).testTag("app-settings-tools")) {
            Text(AppActions.Copy.TITLE, style = MaterialTheme.typography.titleSmall)
            Text(AppActions.Copy.HINT, style = MaterialTheme.typography.bodyMedium, color = muted)
        }
        Spacer(Modifier.height(8.dp))
        OptionGroup {
            if (declared.isEmpty()) {
                Text(
                    AppActions.Copy.NONE,
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                )
            }
            declared.forEachIndexed { index, tool ->
                if (index > 0) GroupDivider()
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("app-tool-$tool")
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            AppActions.actionTitle(tool),
                            style = MaterialTheme.typography.bodyMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            tool,
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            color = muted,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (AppActions.actionGrantable(tool)) {
                        val editable = canGrant && grantBusy != tool
                        GlassSwitch(
                            checked = tool in granted,
                            onCheckedChange = if (editable) ({ on -> viewModel.setGrant(tool, on) }) else null,
                            enabled = editable,
                        )
                    } else {
                        // The ruling: a destructive or owner-only action is never grantable.
                        Text(AppActions.Copy.ALWAYS, style = MaterialTheme.typography.bodyMedium, color = muted)
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))

        // ── Export + Delete app ──────────────────────────────────────────
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        ) {
            if (app.headVersion > 0) {
                GlassPill(
                    Apps.Copy.MENU_EXPORT,
                    icon = ExpIcons.uiShare,
                    onClick = onExport,
                    modifier = Modifier.testTag("app-export"),
                )
            }
            Spacer(Modifier.weight(1f))
            if (canManage) {
                GlassPill(
                    Apps.Copy.MENU_DELETE,
                    icon = ExpIcons.uiDelete,
                    onClick = onDelete,
                    contentColor = DesignTokens.Semantic.Red,
                    modifier = Modifier.testTag("app-delete"),
                )
            }
        }
        Spacer(Modifier.height(8.dp))
    }
}

/** [onBlur] fires when focus LEAVES the field (never on the first,
 *  unfocused report): the save-on-blur. */
@Composable
private fun Modifier.commitOnBlur(onBlur: () -> Unit): Modifier {
    var hadFocus by remember { mutableStateOf(false) }
    return onFocusChanged { state ->
        if (hadFocus && !state.hasFocus) onBlur()
        hadFocus = state.hasFocus
    }
}
