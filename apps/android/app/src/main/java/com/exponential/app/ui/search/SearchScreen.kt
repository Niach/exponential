package com.exponential.app.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.IssueRow
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The Search tab: a pure cross-board search — instant local matching over
 * identifier + title, augmented by the server's full-text search. Assigned
 * issues no longer live here (EXP-58): they moved to the "My Work" tab
 * (PersonalScreen) alongside the inbox.
 */
@Composable
fun SearchScreen(
    onOpenIssue: (String) -> Unit,
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
            Text(
                "Search",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            SearchField(
                query = query,
                onQueryChange = {
                    query = it
                    viewModel.setQuery(it)
                },
                modifier = Modifier.padding(horizontal = 16.dp),
            )

            when {
                state.query.isEmpty() -> EmptyState(
                    message = "Search issues across all your boards.",
                    detail = "Matches identifiers, titles, and full text.",
                    icon = ExpIcons.navSearch,
                )
                state.groups.isEmpty() -> EmptyState(
                    message = "No issues match",
                    icon = ExpIcons.navSearch,
                )
                else -> LazyColumn(
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = BottomBarInset),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    state.groups.forEach { group ->
                        item(key = "board-${group.board.id}") {
                            BoardHeader(board = group.board)
                        }
                        items(group.issues, key = { it.id }) { issue ->
                            IssueRow(
                                issue = issue,
                                labels = emptyList(),
                                assignee = null,
                                onClick = { onOpenIssue(issue.id) },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BoardHeader(board: BoardEntity) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BoardIcon(board, size = 14.dp)
        Spacer(Modifier.width(8.dp))
        Text(
            board.name,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
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
