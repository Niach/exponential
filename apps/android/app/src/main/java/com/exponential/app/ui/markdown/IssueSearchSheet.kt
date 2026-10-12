package com.exponential.app.ui.markdown

import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.SearchList
import com.exponential.app.domain.TypeaheadPrefill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.SearchListPanel
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.theme.TextEmphasis

/** The issue search's input placeholder ×4 (web `ISSUE_SEARCH_PLACEHOLDER`). */
const val ISSUE_SEARCH_PLACEHOLDER = "Search issues"

/**
 * SLOP-28 (EXP-1259 D4): the `#` typeahead on a phone = THE search list in a
 * bottom sheet over the keyboard, its input FOCUSED and prefilled with the
 * token typed after the `#` (spaces allowed), the rows ranked by the ONE
 * engine at its DEFAULT limit (no 6/8 cap) with the server's full-text hits
 * spliced in behind them. A pick hands the row back; Backspace in the empty
 * input is [onBackspaceEmpty] (the host deletes the `#`); a swipe or back is
 * [onDismiss] (the editor text stays as typed).
 */
@Composable
fun IssueSearchSheet(
    handler: IssueRefHandler,
    initialQuery: String,
    onPick: (IssueRefTarget) -> Unit,
    onDismiss: () -> Unit,
    onBackspaceEmpty: (() -> Unit)? = null,
) {
    var query by remember { mutableStateOf(initialQuery) }
    val results = rememberIssueRefCandidates(handler, query, limit = IssueSearch.DEFAULT_LIMIT)
    val items = remember(results) { results.map(::issueSearchItem) }
    val byId = remember(results) { results.associateBy { it.issueId } }
    GlassSheet(title = null, onDismiss = onDismiss, modifier = Modifier.testTag("issue-ref-search-sheet")) {
        SearchListPanel(
            query = query,
            onQueryChange = { query = it },
            placeholder = ISSUE_SEARCH_PLACEHOLDER,
            items = items,
            onPick = { item -> byId[item.value]?.let(onPick) },
            emptyText = "No issues match",
            onBackspaceEmpty = onBackspaceEmpty,
            renderItem = { item -> byId[item.value]?.let { IssueSearchRowBody(it.identifier, it.title, it.resolvedStatus) } },
        )
    }
}

/** An issue as a search-list row: what it matches on is identifier + title. */
private fun issueSearchItem(target: IssueRefTarget): PickerItem<String> = PickerItem(
    value = target.issueId,
    label = target.title,
    keywords = listOf(target.identifier, target.title),
)

/**
 * THE issue row body of the search list ×4: the status glyph, the mono
 * identifier, the title (EXP-581's candidate row, now the picker row).
 */
@Composable
fun RowScope.IssueSearchRowBody(identifier: String, title: String, status: ResolvedIssueStatus?) {
    if (status != null) {
        StatusIcon(status, size = 16.dp)
        Spacer(Modifier.width(10.dp))
    }
    Text(
        identifier,
        style = MaterialTheme.typography.labelMedium,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        maxLines = 1,
    )
    Spacer(Modifier.width(10.dp))
    Text(
        title,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurface,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.weight(1f),
    )
}

/**
 * The `#` typeahead for a composer that hoists its own [field] (the agent and
 * steer composers): while [armed] and a `#` token ends at the caret, the
 * issue search sheet opens prefilled with it and the host is disarmed ([onDisarm])
 * so its inline menu never offers issues. A pick replaces `#token` with
 * `#IDENT ` ([onCommit] gets the new value); [onRefocus] hands focus back to
 * the field however the sheet closes.
 */
@Composable
internal fun IssueTypeaheadSheetHost(
    handler: IssueRefHandler?,
    field: TextFieldValue,
    armed: Boolean,
    onDisarm: () -> Unit,
    onCommit: (TextFieldValue) -> Unit,
    onRefocus: () -> Unit = {},
) {
    var open by remember { mutableStateOf<TypeaheadPrefill?>(null) }
    val prefill = if (armed && handler != null) {
        SearchList.typeaheadPrefill(field.text, field.selection.start)
    } else {
        null
    }
    LaunchedEffect(prefill) {
        if (prefill != null && open == null) {
            open = prefill
            onDisarm()
        }
    }
    val live by rememberUpdatedState(field)
    val token = open ?: return
    val issues = handler ?: return
    fun close(next: Pair<String, Int>?) {
        open = null
        next?.let { (text, caret) -> onCommit(TextFieldValue(text, TextRange(caret))) }
        onRefocus()
    }
    IssueSearchSheet(
        handler = issues,
        initialQuery = token.query,
        onPick = { target -> close(SearchList.spliceTypeaheadPick(live.text, token.start, target.identifier)) },
        onDismiss = { close(null) },
        onBackspaceEmpty = { close(SearchList.deleteTypeaheadToken(live.text, token.start)) },
    )
}
