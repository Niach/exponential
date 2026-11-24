package com.exponential.app.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.SearchList
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.highlightedTitle
import com.exponential.app.ui.components.traceHighlightFills
import com.exponential.app.ui.components.picker.PickerMark
import com.exponential.app.ui.components.picker.PickerRow
import com.exponential.app.ui.session.FindMarks
import at.exponential.ui.primitives.MarkdownHighlight
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.remember
import androidx.compose.ui.text.AnnotatedString
import com.exponential.app.ui.components.IssueRowContent
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SearchListInput
import com.exponential.app.ui.components.SearchListRows
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerItemBody
import com.exponential.app.ui.markdown.IssueSearchRowBody
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * Search: the ACTIVE team's issues (#EXP-1382, every board of that team) —
 * local matching ranked off the UI thread, augmented by ONE server full-text
 * search scoped to the team. Assigned issues no longer
 * live here (EXP-58): they moved to the "My Work" tab (PersonalScreen)
 * alongside the inbox.
 *
 * EXP-686 took it off the bottom bar — it is pushed from the board header's
 * search button now, so it carries its own back button and no bar inset.
 *
 * EXP-922: ONE flat relevance-ordered list, undone issues first, the same
 * limit and the same copy as the web sheet, the desktop palette and iOS.
 */
@Composable
fun SearchScreen(
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit = {},
    /** #EDE-99: a Chats row's pick ([isRoot] = it opens the chat, else the child's session view). */
    onOpenChatHit: (pick: SearchList.ChatPick, isRoot: Boolean) -> Unit = { _, _ -> },
    viewModel: SearchViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // Raw field text lives in Compose state so typing is instant; the ViewModel
    // recomputes matches off a debounced copy. Re-seed the ViewModel on mount
    // so a restored field (process recreation) and its results line up.
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(Unit) { viewModel.setQuery(query) }
    // #EDE-99 (search-list.json A PICK): the hit's session with find in chat
    // open on the query, starting at the hit.
    val openChat: (SearchList.ChatRow) -> Unit = { row ->
        val pick = SearchList.globalChatPick(query, row.hit)
        onOpenChatHit(pick, pick.sessionId == row.rootId)
    }

    // #NEW-133 (search-list.json SUGGESTION): picking the `Search: <query>`
    // row rewrites the field; the focus stays, the search re-runs.
    val suggestion = state.suggestion
    val applySuggestion: (SearchList.SearchSuggestion) -> Unit = {
        query = it.query
        viewModel.setQuery(it.query)
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // SLOP-28 (EXP-1259 D4): THE search list — its input row (focused
            // on open, the result count trailing) over the picker's issue rows.
            // EXP-1649: web-mobile's header IS that row — the back arrow, THE
            // search field (glyph, text, clear) and the count; no title.
            SearchListInput(
                value = query,
                onValueChange = {
                    query = it
                    viewModel.setQuery(it)
                },
                // #EDE-99: with a chat store the search names chats too.
                placeholder = if (state.store) SearchList.Chats.Copy.PLACEHOLDER else "Search issues",
                count = state.sections.count,
                autoFocus = true,
                onSubmit = {
                    val issue = state.results.firstOrNull()
                    val chat = state.chats.firstOrNull()
                    when {
                        // The top row is the suggestion while it shows.
                        suggestion != null -> applySuggestion(suggestion)
                        issue != null -> onOpenIssue(issue.issue.id)
                        chat != null && state.sections.titled -> openChat(chat)
                    }
                },
                asField = true,
                leading = {
                    CircleIconButton(
                        ExpIcons.uiBack,
                        "Back",
                        onClick = onBack,
                        borderless = true,
                        size = 24.dp,
                        glyphSize = 16.dp,
                    )
                },
            )

            // EXP-922: the ×4 copy set — same words on web, desktop, iOS and
            // Android (web `lib/issue-search.ts`; `issue-search-surfaces.test.ts` greps this file).
            when {
                // #EDE-99: with a store, ONE list of titled Sections (Issues, Chats).
                state.sections.titled && state.query.isNotEmpty() -> if (state.sections.empty && suggestion == null) {
                    EmptyState(title = SearchList.Chats.Copy.NO_RESULTS, icon = ExpIcons.navSearch)
                } else {
                    SearchSections(
                        state = state,
                        onOpenIssue = onOpenIssue,
                        onOpenChat = openChat,
                        onPickSuggestion = applySuggestion,
                        modifier = Modifier.weight(1f).padding(top = 4.dp, bottom = 8.dp),
                    )
                }
                state.query.isEmpty() -> EmptyState(
                    title = "Search issues across all your boards.",
                    description = SEARCH_EMPTY_DETAIL,
                    icon = ExpIcons.navSearch,
                )
                state.results.isEmpty() && suggestion == null -> EmptyState(
                    title = "No issues match",
                    icon = ExpIcons.navSearch,
                )
                else -> {
                    val byId = state.results.associateBy { it.issue.id }
                    SearchListRows(
                        items = listOfNotNull(suggestion?.let(::suggestionItem)) + state.results.map { result ->
                            PickerItem(value = result.issue.id, label = result.issue.title)
                        },
                        onPick = { item ->
                            if (item.value == SUGGESTION_VALUE) suggestion?.let(applySuggestion) else onOpenIssue(item.value)
                        },
                        rowTestTag = { item -> SearchList.Suggestion.TEST_ID.takeIf { item.value == SUGGESTION_VALUE } },
                        // A whole screen: the rows fill it, the page ground fades.
                        capHeight = false,
                        fadeColor = GlassTokens.BackgroundBottom,
                        modifier = Modifier.weight(1f).padding(top = 4.dp, bottom = 8.dp),
                        renderItem = { item ->
                            val result = byId[item.value]
                            if (result != null) {
                                IssueSearchRowBody(result.issue.identifier, result.issue.title, result.status)
                            } else {
                                // The suggestion: the Picker's ACTION row (icon + name).
                                PickerItemBody(item)
                            }
                        },
                    )
                }
            }
        }
    }
}

/**
 * EXP-922 + #NEW-133: the empty-query detail, ONE constant (web
 * `ISSUE_SEARCH_EMPTY_DETAIL`; `issue-search-surfaces.test.ts` greps it here).
 */
private const val SEARCH_EMPTY_DETAIL =
    "Matches identifiers, titles and full text. Filter with by:, is:, status:, created: and updated:."

/** The suggestion row's value (never an issue id or a chat key). */
private const val SUGGESTION_VALUE = "search-suggestion"

/** #NEW-133: the suggestion as the Picker's ACTION row: icon by concept + `Search: <query>`. */
private fun suggestionItem(suggestion: SearchList.SearchSuggestion): PickerItem<String> = PickerItem(
    value = SUGGESTION_VALUE,
    label = suggestion.label,
    icon = ExpIcons.byConcept(SearchList.Suggestion.ICON) ?: ExpIcons.navSearch,
)

/**
 * #EDE-99 (search-list.json CHATS): the global search as genUI Sections:
 * Issues (the issue rows) then Chats (the chat store's rows), each only
 * while it has rows, its count after the title; Chats spins while the store
 * answers a newer query, its previous rows kept. The top row is
 * preselected (Enter picks it).
 */
@Composable
private fun SearchSections(
    state: SearchState,
    onOpenIssue: (String) -> Unit,
    onOpenChat: (SearchList.ChatRow) -> Unit,
    onPickSuggestion: (SearchList.SearchSuggestion) -> Unit,
    modifier: Modifier = Modifier,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    LazyColumn(
        modifier = modifier.fillMaxWidth().testTag(SearchList.LIST_TEST_ID),
        contentPadding = PaddingValues(vertical = 4.dp),
    ) {
        // #NEW-133: the suggestion row leads, above the first section header.
        val suggestion = state.suggestion
        if (suggestion != null) {
            item(key = SUGGESTION_VALUE) {
                Box(Modifier.testTag(SearchList.Suggestion.TEST_ID)) {
                    PickerRow(
                        item = suggestionItem(suggestion),
                        mark = PickerMark.None,
                        onClick = { onPickSuggestion(suggestion) },
                        highlighted = true,
                    )
                }
            }
        }
        var first = suggestion == null
        for (section in state.sections.sections) {
            val title = if (section.key == SearchList.SectionKey.Issues) SearchList.Chats.Copy.ISSUES else SearchList.Chats.Copy.CHATS
            val tag = if (section.key == SearchList.SectionKey.Issues) SearchList.Chats.TestIds.ISSUES else SearchList.Chats.TestIds.CHATS
            item(key = "section:${section.key.wire}") {
                SectionHeader(
                    title = title,
                    loading = section.loading,
                    modifier = Modifier.padding(top = if (section.key == SearchList.SectionKey.Chats) 8.dp else 0.dp).testTag(tag),
                    trailing = { Text(section.count.toString(), style = MaterialTheme.typography.labelLarge, color = muted) },
                )
            }
            when (section.key) {
                SearchList.SectionKey.Issues -> {
                    val top = first
                    itemsIndexed(state.results, key = { _, result -> "issue:${result.issue.id}" }) { index, result ->
                        Box(Modifier.testTag(SearchList.ROW_TEST_ID)) {
                            PickerRow(
                                item = PickerItem(value = result.issue.id, label = result.issue.title),
                                mark = PickerMark.None,
                                onClick = { onOpenIssue(result.issue.id) },
                                body = { IssueSearchRowBody(result.issue.identifier, result.issue.title, result.status) },
                                highlighted = top && index == 0,
                            )
                        }
                    }
                }
                SearchList.SectionKey.Chats -> {
                    val top = first
                    itemsIndexed(state.chats, key = { _, row -> row.key }) { index, row ->
                        Box(Modifier.testTag(SearchList.Chats.TestIds.CHAT_ROW)) {
                            PickerRow(
                                item = PickerItem(value = row.key, label = row.title),
                                mark = PickerMark.None,
                                onClick = { onOpenChat(row) },
                                body = { SearchChatRowBody(row) },
                                highlighted = top && index == 0,
                            )
                        }
                    }
                }
            }
            first = false
        }
    }
}

/**
 * #EDE-99: a Chats row = THE row: the concept glyph (`search-chat`, an
 * agent's `search-agent`), the chat's title over the hit's snippet (one
 * line, the query's matches painted by genUI `highlight`), the date trailing.
 */
@Composable
private fun RowScope.SearchChatRowBody(row: SearchList.ChatRow) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Icon(
        ExpIcons.byConcept(row.icon) ?: ExpIcons.searchChat,
        contentDescription = null,
        modifier = Modifier.size(16.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
    Spacer(Modifier.width(10.dp))
    Column(
        modifier = Modifier.weight(1f).heightIn(min = (SearchList.Chats.ROW_HEIGHT_PHONE - 16).dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            row.title,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        val style = MaterialTheme.typography.bodySmall
        val highlight = remember(row.highlight) { MarkdownHighlight(row.highlight, -1, FindMarks.Match, FindMarks.MatchCurrent) }
        val marked = remember(row.subtitle, highlight) { highlightedTitle(AnnotatedString(row.subtitle), highlight) }
        val fills = traceHighlightFills(marked, style)
        Text(
            marked,
            style = style,
            color = muted,
            maxLines = 1,
            softWrap = false,
            overflow = TextOverflow.Ellipsis,
            onTextLayout = fills.second,
            modifier = Modifier.then(fills.first),
        )
    }
    Spacer(Modifier.width(8.dp))
    Text(row.trailing, style = MaterialTheme.typography.labelMedium, color = muted, maxLines = 1)
}
