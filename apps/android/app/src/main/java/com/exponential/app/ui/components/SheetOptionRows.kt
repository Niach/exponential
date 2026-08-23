package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.R
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// The grouped-sheet building blocks the unified Start-coding sheet introduced
// (EXP-208/EXP-211 — iOS Form parity), extracted for reuse by the
// device-settings sheet (EXP-481). Visuals are byte-identical to the
// originals; only the visibility moved — except [PickerRow], which since
// EXP-607 opens a [GlassSheet] instead of an anchored dropdown (a dropdown
// over a bottom sheet lands wherever M3 can fit it; a sheet always presents
// the same way, and matches how every other picker in the app reads).

/** Aligned with the grouped cards' inner content edge (16dp card inset + 16dp row padding). */
@Composable
internal fun SectionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(horizontal = 32.dp, vertical = 2.dp),
    )
}

// iOS-inset-grouped-section analog (EXP-208): a rounded glass container that
// wraps a group of rows, separated inside by [GroupDivider] hairlines.
@Composable
internal fun OptionGroup(content: @Composable () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(GlassTokens.RowFill, RoundedCornerShape(12.dp)),
    ) {
        content()
    }
}

/** Hairline between rows in an [OptionGroup] (TeamSettingsScreen's idiom). */
@Composable
internal fun GroupDivider() {
    HorizontalDivider(color = Color.White.copy(alpha = 0.06f))
}

// iOS-Form-style picker row: label left, selected value + chevron right; tap
// opens a glass sheet of the options. Disabled = dimmed + non-interactive.
@Composable
internal fun PickerRow(
    label: String,
    value: String,
    options: List<String>,
    selected: String?,
    optionLabel: (String) -> String,
    onSelect: (String) -> Unit,
    enabled: Boolean = true,
) {
    var open by remember { mutableStateOf(false) }
    val contentAlpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled) { open = true }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = contentAlpha),
            modifier = Modifier.weight(1f),
        )
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Secondary else TextEmphasis.Quaternary,
            ),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Tertiary else TextEmphasis.Quaternary,
            ),
        )
    }
    if (open) {
        GlassSheet(title = label, onDismiss = { open = false }) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState()),
            ) {
                options.forEach { option ->
                    GlassSheetRow(
                        label = optionLabel(option),
                        selected = option == selected,
                        onClick = {
                            open = false
                            onSelect(option)
                        },
                    )
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

// EXP-615: the loose agent pill strip is gone — every surface now renders ONE
// segmented capsule via [AgentSegmentedTabs] (LaunchOptionsSection.kt).

@Composable
internal fun SwitchRow(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    enabled: Boolean = true,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
            ),
            modifier = Modifier.weight(1f),
        )
        Switch(checked = checked, onCheckedChange = onCheckedChange, enabled = enabled)
    }
}

// ── Agent launch-option vocabulary (shared by the start + settings sheets) ──

/** Sentinel-free UI state: an empty effort means "CLI default" (omit --effort). */
internal const val CLI_DEFAULT_EFFORT = ""

/** Same convention for codex/pi models: an empty model means "CLI default". */
internal const val CLI_DEFAULT_MODEL = ""

internal const val DEFAULT_AGENT = "claude"

internal fun modelValuesFor(agent: String): List<String> = when (agent) {
    "codex" -> DomainContract.codexModelValues
    "pi" -> DomainContract.piModelValues
    else -> DomainContract.codingModelValues
}

internal fun effortValuesFor(agent: String): List<String> = when (agent) {
    "codex" -> DomainContract.codexEffortValues
    "pi" -> DomainContract.piThinkingValues
    else -> DomainContract.codingEffortValues
}

/** The Start-coding sheet's model vocabulary: claude has no CLI-default entry. */
internal fun modelOptionsFor(agent: String): List<String> =
    if (agent == DEFAULT_AGENT) DomainContract.codingModelValues
    else listOf(CLI_DEFAULT_MODEL) + modelValuesFor(agent)

/** claude has no CLI-default model entry; codex/pi default to the blank one. */
internal fun defaultModelFor(agent: String): String =
    if (agent == DEFAULT_AGENT) DomainContract.codingModelValues.first() else CLI_DEFAULT_MODEL

/**
 * Plan mode is claude (native) + pi (via the launcher-injected extension,
 * EXP-441); codex has no launch-into-plan mode.
 */
internal fun supportsPlanMode(agent: String): Boolean =
    agent == DEFAULT_AGENT || agent == "pi"

internal fun agentLabel(value: String): String = when (value) {
    "claude" -> "Claude Code"
    "codex" -> "Codex"
    "pi" -> "pi"
    else -> value
}

/** Monochrome brand marks derived from the desktop IDE's SVG icons (EXP-208). */
internal fun agentIconRes(value: String): Int = when (value) {
    "codex" -> R.drawable.ic_agent_codex
    "pi" -> R.drawable.ic_agent_pi
    else -> R.drawable.ic_agent_claude
}

internal fun modelLabel(value: String): String = when (value) {
    CLI_DEFAULT_MODEL -> "CLI default"
    "gpt-5.6-sol" -> "GPT-5.6 Sol"
    "gpt-5.6-terra" -> "GPT-5.6 Terra"
    "gpt-5.6-luna" -> "GPT-5.6 Luna"
    "grok-4.5" -> "Grok 4.5"
    else -> value.replaceFirstChar { it.uppercaseChar() }
}

internal fun effortLabel(value: String): String = when (value) {
    CLI_DEFAULT_EFFORT -> "CLI default"
    "xhigh" -> "XHigh"
    else -> value.replaceFirstChar { it.uppercaseChar() }
}
