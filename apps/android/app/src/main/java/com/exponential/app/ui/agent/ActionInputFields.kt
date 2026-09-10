package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.domain.ActionInputValues
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.IconPicker
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the picked action's typed inputs, rendered INSIDE the composer
 * card under its chip (the card is the chrome, so these are bare rows with
 * hairlines rather than a grouped card of their own). Only PICK types exist
 * since the ONE launcher: repo/board/pr → a picker row over the team registry
 * / synced boards / the team's open pull requests; icon (EXP-273) → the
 * curated swatch grid the create-board form draws. The stored value is the
 * picked id or the glyph name; a cleared optional picker stores "" which the
 * submit path drops. A def of any other type — `text`/`textarea` from an
 * older row, or something newer than this build — blocks the run with the
 * needs-a-newer-app caption instead of rendering.
 */
@Composable
internal fun ActionInputFields(
    defs: List<ActionInputDto>,
    values: Map<String, String>,
    repos: List<TeamRepo>,
    boards: List<StartBoardOption>,
    pullRequests: List<StartPullRequestOption>,
    onValueChange: (key: String, value: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        if (ActionInputValues.hasUnsupportedType(defs)) {
            Text(
                "This action needs a newer app version.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
            return@Column
        }
        defs.forEachIndexed { index, def ->
            if (index > 0) GroupDivider()
            ActionInputField(
                def = def,
                value = values[def.key] ?: "",
                repos = repos,
                boards = boards,
                pullRequests = pullRequests,
                onValueChange = { onValueChange(def.key, it) },
            )
        }
    }
}

@Composable
private fun ActionInputField(
    def: ActionInputDto,
    value: String,
    repos: List<TeamRepo>,
    boards: List<StartBoardOption>,
    pullRequests: List<StartPullRequestOption>,
    onValueChange: (String) -> Unit,
) {
    val label = if (def.required) def.label else "${def.label} (optional)"
    when (def.type) {
        "repo" -> PickerRow(
            label = label,
            value = when {
                value.isEmpty() && def.required -> "Select"
                value.isEmpty() -> "None"
                else -> repos.firstOrNull { it.id == value }?.fullName ?: value
            },
            options = (if (def.required) emptyList() else listOf("")) + repos.map { it.id },
            selected = value.takeIf { it.isNotEmpty() || !def.required },
            optionLabel = { id ->
                if (id.isEmpty()) "None" else repos.firstOrNull { it.id == id }?.fullName ?: id
            },
            onSelect = onValueChange,
        )
        "board" -> PickerRow(
            label = label,
            value = when {
                value.isEmpty() && def.required -> "Select"
                value.isEmpty() -> "None"
                else -> boards.firstOrNull { it.id == value }?.name ?: value
            },
            options = (if (def.required) emptyList() else listOf("")) + boards.map { it.id },
            selected = value.takeIf { it.isNotEmpty() || !def.required },
            optionLabel = { id ->
                if (id.isEmpty()) "None" else boards.firstOrNull { it.id == id }?.name ?: id
            },
            onSelect = onValueChange,
        )
        // EXP-259: the value is the REPRESENTATIVE issue id of an open
        // issue-linked PR (batch PRs dedupe by prUrl, so one row can list
        // several identifiers).
        "pr" -> if (pullRequests.isEmpty()) {
            PickerRow(
                label = label,
                value = "No open pull requests",
                options = emptyList(),
                selected = null,
                optionLabel = { it },
                onSelect = {},
            )
        } else {
            PickerRow(
                label = label,
                value = when {
                    value.isEmpty() && def.required -> "Select"
                    value.isEmpty() -> "None"
                    else -> pullRequests.firstOrNull { it.issueId == value }?.label ?: value
                },
                options = (if (def.required) emptyList() else listOf("")) +
                    pullRequests.map { it.issueId },
                selected = value.takeIf { it.isNotEmpty() || !def.required },
                optionLabel = { id ->
                    if (id.isEmpty()) {
                        "None"
                    } else {
                        pullRequests.firstOrNull { it.issueId == id }?.label ?: id
                    }
                },
                onSelect = onValueChange,
            )
        }
        // EXP-273: the curated icon set. The value is a glyph NAME, so there
        // is nothing team-scoped to look up — the picker is the create-board
        // form's (EXP-575). Optional inputs keep a "No icon" reset in the sheet.
        "icon" -> Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                label,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Primary),
                modifier = Modifier.weight(1f),
            )
            IconPicker(
                selected = value,
                onSelect = onValueChange,
                allowsNone = !def.required,
            )
        }
        // Unreachable: [ActionInputFields] short-circuits on an unsupported
        // type before any row renders.
        else -> Unit
    }
}
