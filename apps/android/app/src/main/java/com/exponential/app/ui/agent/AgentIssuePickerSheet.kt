package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.SearchListPanel
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.markdown.ISSUE_SEARCH_PLACEHOLDER
import com.exponential.app.ui.markdown.IssueSearchRowBody
import com.exponential.app.ui.theme.TextEmphasis

// Loose batch caps (desktop parity): a hard 30-issue ceiling, and a soft note
// past 6 that a single Claude session across that many issues burns tokens.
internal const val MAX_BATCH_ISSUES = 30
internal const val LARGE_BATCH_HINT_THRESHOLD = 6

/**
 * EXP-825 → SLOP-28 (EXP-1259 D5): the "+" menu's Implement issue › PAGE —
 * THE search list ([SearchListPanel]) pushed inside the composer's menu
 * sheet, its input focused. Nothing is submitted here: every tap chips the
 * issue on the composer at once (the picks pinned first, each with its
 * check), and once something is picked the footer reads `Implement N issues`
 * (composer-menu.json `implementButton`) and only closes the menu. The rows
 * are the search list's issue rows (status glyph, mono identifier, title) at
 * the ONE engine's default limit; the checked rows stay put on toggle
 * (EXP-241 — re-sorting teleported the tapped row out from under the finger).
 */
@Composable
internal fun AgentIssuePickerPage(
    issues: List<IssueOption>,
    checkedIds: Set<String>,
    onToggle: (String) -> Unit,
    onDone: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    // EXP-241: `checkedIds` is deliberately NOT a key — the pinning is
    // recomputed when the QUERY changes, never on a toggle.
    val rows = remember(issues, query) {
        val pinned = issues.filter { it.id in checkedIds }
        pinned + IssueSearch.rank(issues, query, exclude = checkedIds)
    }
    val rowsById = remember(rows) { rows.associateBy { it.id } }
    val items = remember(rows) {
        rows.map { PickerItem(value = it.id, label = it.title, keywords = listOf(it.identifier, it.title)) }
    }
    val checked = issues.filter { it.id in checkedIds }
    val multiRepo = checked.map { it.repositoryId }.toSet().size > 1
    val tooMany = checked.size > MAX_BATCH_ISSUES
    val caption = when {
        multiRepo -> "Pick issues from a single repository per session."
        tooMany -> "At most $MAX_BATCH_ISSUES issues per session. Split the batch."
        checked.size > LARGE_BATCH_HINT_THRESHOLD -> "Large batches are token-expensive."
        else -> null
    }
    Column(modifier = Modifier.fillMaxWidth().testTag("agent-composer-issues-picker")) {
        SearchListPanel(
            query = query,
            onQueryChange = { query = it },
            placeholder = ISSUE_SEARCH_PLACEHOLDER,
            items = items,
            // The SINGLE arm's mark: a picked row wears the trailing check
            // (search-list.json), the toggle is ours.
            value = checkedIds,
            onPick = { onToggle(it.value) },
            emptyText = if (issues.isEmpty()) "No eligible issues to code" else "No matching issues",
            renderItem = { item ->
                rowsById[item.value]?.let { option ->
                    IssueSearchRowBody(
                        option.identifier,
                        option.title,
                        remember(option.status) { IssueStatusResolver.resolve(null, option.status, emptyList()) },
                    )
                }
            },
        )
        if (caption != null) {
            Text(
                caption,
                style = MaterialTheme.typography.labelSmall,
                color = if (multiRepo || tooMany) {
                    MaterialTheme.colorScheme.error
                } else {
                    MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
                },
                modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding, vertical = 4.dp),
            )
        }
        if (checked.isNotEmpty()) {
            GlassSubmitButton(
                label = ComposerMenu.implementButtonLabel(checked.size),
                onClick = onDone,
                modifier = Modifier
                    .padding(horizontal = GlassSheetDefaults.HorizontalPadding, vertical = 12.dp)
                    .testTag(ComposerMenu.IMPLEMENT_SUBMIT_TEST_ID),
            )
        }
    }
}
