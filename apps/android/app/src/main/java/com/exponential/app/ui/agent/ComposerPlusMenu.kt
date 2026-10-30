package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuEntry
import com.exponential.app.domain.ComposerMenuRowId
import com.exponential.app.domain.ComposerMenuRowKind
import com.exponential.app.ui.components.GlassSwitch
import com.exponential.app.ui.components.GlassSwitchSize
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-1249: the composer's ONE "+" menu on a phone (web `composer-plus-menu.tsx`
 * as its sheet arm): the rows of [ComposerMenu.composerMenuLayout] in a
 * bottom sheet — S-3: Photo or file, Issue › | Device ›, Account ›,
 * Model ›, Repository › | Effort ›, Subagents ›, Ultracode | MCP servers ›,
 * Computer use, each only on its surface and while its condition holds.
 * #EXP-1434: Prompts › (after Issue, while the team has a prompt) pushes
 * [ComposerPromptsPage] as a page of [pages]. SLOP-28 (EXP-1259 D5): Implement issue
 * PUSHES a page inside this sheet ([issuePage]: THE search list, back
 * returns to the rows); the other submenu
 * rows close it and open their picker sheet (two stacked sheets are a dead
 * end on Android); a toggle flips in place.
 */
@Composable
internal fun ComposerPlusMenuSheet(
    layout: List<ComposerMenuEntry>,
    /** The picked agent is codex: Effort reads `Reasoning`. */
    codex: Boolean,
    effortValue: String?,
    /** Ultracode owns the effort (it IS `--effort ultracode`). */
    effortEnabled: Boolean,
    subagentsValue: String?,
    ultracode: Boolean,
    mcpValue: String?,
    computerUse: Boolean,
    canAttach: Boolean,
    /** EXP-1334: an ended session's option rows (Effort, Ultracode, MCP
     *  servers, Computer use) read, never change (web `rowsDisabled`). */
    rowsReadOnly: Boolean = false,
    onRow: (ComposerMenuRowId) -> Unit,
    onUltracodeChange: (Boolean) -> Unit,
    onComputerUseChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
    /** The Implement issue › page (the search list); its arg closes the menu. */
    issuePage: @Composable (close: () -> Unit) -> Unit,
    /** S-3: the runner submenus (Device, Account, Model, Repository) PUSH
     *  their page inside this sheet: THE existing picker's rows inline (the
     *  sheet hosts them, N-6); the arg closes the menu. */
    pages: Map<ComposerMenuRowId, @Composable (close: () -> Unit) -> Unit> = emptyMap(),
    /** S-3: a submenu row's muted value (the picked label) by row id. */
    values: Map<ComposerMenuRowId, String?> = emptyMap(),
) {
    var page by remember { mutableStateOf<ComposerMenuRowId?>(null) }
    GlassSheet(
        title = null,
        onDismiss = onDismiss,
        modifier = Modifier.testTag(ComposerMenu.MENU_TEST_ID),
    ) {
        val pushed = page
        if (pushed != null) {
            // The pushed page's own back: to the menu's rows, not out of it.
            BackHandler { page = null }
            val entry = layout.filterIsInstance<ComposerMenuEntry.Row>().firstOrNull { it.id == pushed }
            GlassSheetRow(
                label = entry?.label.orEmpty(),
                onClick = { page = null },
                leading = { RowGlyph(ExpIcons.uiBack) },
            )
            val page = pages[pushed]
            if (page != null) page(onDismiss) else issuePage(onDismiss)
            return@GlassSheet
        }
        Column(modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState())) {
            layout.forEach { entry ->
                when (entry) {
                    ComposerMenuEntry.Separator -> HorizontalDivider(
                        thickness = GlassTokens.Hairline,
                        color = GlassTokens.StrokeRow,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                    )
                    is ComposerMenuEntry.Row -> {
                        val label = if (codex && entry.codexLabel != null) entry.codexLabel else entry.label
                        val tag = ComposerMenu.rowTestId(entry.id)
                        when (entry.kind) {
                            ComposerMenuRowKind.Toggle -> {
                                val checked = when (entry.id) {
                                    ComposerMenuRowId.Ultracode -> ultracode
                                    else -> computerUse
                                }
                                val onChange: (Boolean) -> Unit = when (entry.id) {
                                    ComposerMenuRowId.Ultracode -> onUltracodeChange
                                    else -> onComputerUseChange
                                }
                                Box(
                                    modifier = Modifier.testTag(tag).semantics {
                                        role = Role.Switch
                                        toggleableState = if (checked) ToggleableState.On else ToggleableState.Off
                                    },
                                ) {
                                    GlassSheetRow(
                                        label = label,
                                        enabled = !rowsReadOnly,
                                        onClick = { onChange(!checked) },
                                        leading = { RowGlyph(composerMenuGlyph(entry.id)) },
                                        trailing = {
                                            GlassSwitch(checked = checked, onCheckedChange = null, size = GlassSwitchSize.Menu)
                                        },
                                    )
                                }
                            }
                            else -> {
                                val value = when (entry.id) {
                                    ComposerMenuRowId.Effort -> effortValue
                                    ComposerMenuRowId.Subagents -> subagentsValue
                                    ComposerMenuRowId.McpServers -> mcpValue
                                    else -> values[entry.id]
                                }
                                val enabled = when (entry.id) {
                                    ComposerMenuRowId.Effort -> effortEnabled && !rowsReadOnly
                                    ComposerMenuRowId.Subagents, ComposerMenuRowId.McpServers -> !rowsReadOnly
                                    ComposerMenuRowId.AddFile -> canAttach
                                    else -> true
                                }
                                Box(modifier = Modifier.testTag(tag)) {
                                    GlassSheetRow(
                                        label = label,
                                        enabled = enabled,
                                        onClick = {
                                            when (entry.id) {
                                                ComposerMenuRowId.ImplementIssue -> page = entry.id
                                                in pages -> page = entry.id
                                                else -> onRow(entry.id)
                                            }
                                        },
                                        leading = { RowGlyph(composerMenuGlyph(entry.id)) },
                                        trailing = if (entry.kind == ComposerMenuRowKind.Submenu) {
                                            { SubmenuTrailing(value) }
                                        } else {
                                            null
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The row's concept glyph (fixture `rows[].icon`, ×4). */
internal fun composerMenuGlyph(id: ComposerMenuRowId): ImageVector = when (id) {
    ComposerMenuRowId.ImplementIssue -> ExpIcons.editorIssueRef
    ComposerMenuRowId.Prompts -> ExpIcons.codingCommand
    ComposerMenuRowId.AddFile -> ExpIcons.uiAttach
    ComposerMenuRowId.Device -> ExpIcons.navDevices
    ComposerMenuRowId.Account -> ExpIcons.settingsAccount
    ComposerMenuRowId.Model -> ExpIcons.codingAssistant
    ComposerMenuRowId.Repository -> ExpIcons.settingsRepositories
    ComposerMenuRowId.Effort -> ExpIcons.uiEstimate
    ComposerMenuRowId.Subagents -> ExpIcons.codingSubagent
    ComposerMenuRowId.Ultracode -> ExpIcons.actionDefault
    ComposerMenuRowId.McpServers -> ExpIcons.uiMcp
    ComposerMenuRowId.ComputerUse -> ExpIcons.navComputer
}

@Composable
private fun RowGlyph(icon: ImageVector) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(18.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
}

/** A submenu row's trailing: its muted value, then the chevron. */
@Composable
private fun SubmenuTrailing(value: String?) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (value != null) {
            Text(value, style = MaterialTheme.typography.bodyMedium, color = muted, maxLines = 1)
            Spacer(Modifier.width(6.dp))
        }
        Icon(ExpIcons.uiChevronRight, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
    }
}

/**
 * #EXP-1434 (composer-menu.json `prompts`): Prompts › = THE search list over
 * the team's prompts in the `/` menu's order (row = the prompt's icon + its
 * name; the input filters by slash name like the `/` filter). [onPick] does
 * exactly what the `/` menu's pick does on that composer; the caller closes.
 */
@Composable
internal fun ComposerPromptsPage(
    prompts: List<com.exponential.app.domain.SlashPrompt>,
    onPick: (com.exponential.app.domain.SlashPrompt) -> Unit,
) {
    var query by remember { mutableStateOf("") }
    val visible = remember(prompts, query) {
        val needle = com.exponential.app.domain.SlashCommands.promptSlashName(query)
        prompts.filter { it.slashName.startsWith(needle) }
    }
    val byId = remember(prompts) { prompts.associateBy { it.id } }
    val items = remember(visible) {
        visible.map {
            com.exponential.app.ui.components.picker.PickerItem(
                value = it.id,
                label = it.name,
                icon = it.icon?.let(ExpIcons::byName) ?: ExpIcons.actionDefault,
            )
        }
    }
    Column(modifier = Modifier.fillMaxWidth().testTag("agent-composer-prompts-picker")) {
        com.exponential.app.ui.components.SearchListPanel(
            query = query,
            onQueryChange = { query = it },
            placeholder = "Search",
            items = items,
            onPick = { item -> byId[item.value]?.let(onPick) },
            emptyText = "No results",
        )
    }
}
