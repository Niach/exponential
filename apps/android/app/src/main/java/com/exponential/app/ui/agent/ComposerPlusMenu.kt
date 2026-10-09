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
import androidx.compose.runtime.Composable
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
 * bottom sheet — Implement issue ›, Run action ›, Add file or image | Effort ›,
 * Subagents ›, Ultracode | MCP servers ›, Computer use. A submenu row CLOSES
 * this sheet and opens its picker sheet (two stacked sheets are a dead end on
 * Android: the picker IS the pushed page); a toggle flips in place.
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
    onRow: (ComposerMenuRowId) -> Unit,
    onUltracodeChange: (Boolean) -> Unit,
    onComputerUseChange: (Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    GlassSheet(
        title = null,
        onDismiss = onDismiss,
        modifier = Modifier.testTag(ComposerMenu.MENU_TEST_ID),
    ) {
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
                                    else -> null
                                }
                                val enabled = when (entry.id) {
                                    ComposerMenuRowId.Effort -> effortEnabled
                                    ComposerMenuRowId.AddFile -> canAttach
                                    else -> true
                                }
                                Box(modifier = Modifier.testTag(tag)) {
                                    GlassSheetRow(
                                        label = label,
                                        enabled = enabled,
                                        onClick = { onRow(entry.id) },
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
    ComposerMenuRowId.RunAction -> ExpIcons.actionRun
    ComposerMenuRowId.AddFile -> ExpIcons.uiAttach
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
