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
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

/**
 * Search: a pure cross-board search — instant local matching over identifier +
 * title, augmented by the server's full-text search. Assigned issues no longer
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
    viewModel: SearchViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    // Raw field text lives in Compose state so typing is instant; the ViewModel
    // recomputes matches off a debounced copy. Re-seed the ViewModel on mount
    // so a restored field (process recreation) and its results line up.
    var query by rememberSaveable { mutableStateOf("") }
    LaunchedEffect(Unit) { viewModel.setQuery(query) }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircleIconButton(ExpIcons.uiBack, "Back", onClick = onBack, borderless = true)
                Spacer(Modifier.width(12.dp))
                Text(
                    "Search",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            SearchField(
                query = query,
                onQueryChange = {
                    query = it
                    viewModel.setQuery(it)
                },
                modifier = Modifier.padding(horizontal = 16.dp),
            )

            // EXP-922: the ×4 copy set — same words on web, desktop, iOS and
            // Android (web `lib/issue-search.ts`; `issue-search-surfaces.test.ts` greps this file).
            when {
                state.query.isEmpty() -> EmptyState(
                    message = "Search issues across all your boards.",
                    detail = "Matches identifiers, titles, and full text.",
                    icon = ExpIcons.navSearch,
                )
                state.results.isEmpty() -> EmptyState(
                    message = "No issues match",
                    icon = ExpIcons.navSearch,
                )
                else -> LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 24.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    items(state.results, key = { it.issue.id }) { result ->
                        SearchResultRow(result = result, onClick = { onOpenIssue(result.issue.id) })
                    }
                }
            }
        }
    }
}

/**
 * The ×4 search row (EXP-922): the issue's status glyph, its title, and a
 * sub-line carrying the board's own glyph, the board name and the identifier —
 * the web `IssueSearchSheet` row, the desktop palette's `render_issue_row` and
 * the iOS `resultRow`. Priority is deliberately absent: no other client shows
 * it here. The anchor glyph is the right one cross-team (EXP-314): status rows
 * are team-scoped and search spans teams.
 */
@Composable
private fun SearchResultRow(result: SearchResult, onClick: () -> Unit) {
    val issue = result.issue
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("issue-row-${issue.identifier}")
            .glassRow()
            .clickable(onClick = onClick)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        StatusIcon(IssueStatus.fromWire(issue.status), size = 16.dp)
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                issue.title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(2.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (result.board != null) {
                    BoardIcon(result.board, size = 12.dp)
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    if (result.board != null) {
                        "${result.board.name} · ${issue.identifier}"
                    } else {
                        issue.identifier
                    },
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

// Rounded glass search field — the styling the issue list's inline search used
// before search moved to this tab.
@Composable
private fun SearchField(
    query: String,
    onQueryChange: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    GlassTextField(
        value = query,
        onValueChange = onQueryChange,
        modifier = modifier.fillMaxWidth(),
        placeholder = "Search issues",
        leadingIcon = {
            Icon(
                ExpIcons.navSearch,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
        trailingIcon = {
            if (query.isNotEmpty()) {
                IconButton(onClick = { onQueryChange("") }) {
                    Icon(ExpIcons.uiClose, contentDescription = "Clear search")
                }
            }
        },
        singleLine = true,
    )
}
