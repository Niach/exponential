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
import com.exponential.app.ui.components.IssueRowContent
import com.exponential.app.ui.components.BoardIcon
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SearchListInput
import com.exponential.app.ui.components.SearchListRows
import com.exponential.app.ui.components.picker.PickerItem
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
            // SLOP-28 (EXP-1259 D4): THE search list — its input row (focused
            // on open, the result count trailing) over the picker's issue rows.
            SearchListInput(
                value = query,
                onValueChange = {
                    query = it
                    viewModel.setQuery(it)
                },
                placeholder = "Search issues",
                count = state.results.size.takeIf { state.query.isNotEmpty() },
                autoFocus = true,
                onSubmit = { state.results.firstOrNull()?.let { onOpenIssue(it.issue.id) } },
            )

            // EXP-922: the ×4 copy set — same words on web, desktop, iOS and
            // Android (web `lib/issue-search.ts`; `issue-search-surfaces.test.ts` greps this file).
            when {
                state.query.isEmpty() -> EmptyState(
                    title = "Search issues across this team's boards.",
                    description = "Matches identifiers, titles, and full text.",
                    icon = ExpIcons.navSearch,
                )
                state.results.isEmpty() -> EmptyState(
                    title = "No issues match",
                    icon = ExpIcons.navSearch,
                )
                else -> {
                    val byId = state.results.associateBy { it.issue.id }
                    SearchListRows(
                        items = state.results.map { result ->
                            PickerItem(value = result.issue.id, label = result.issue.title)
                        },
                        onPick = { item -> onOpenIssue(item.value) },
                        // A whole screen: the rows fill it, the page ground fades.
                        capHeight = false,
                        fadeColor = GlassTokens.BackgroundBottom,
                        modifier = Modifier.weight(1f).padding(top = 4.dp, bottom = 8.dp),
                        renderItem = { item ->
                            byId[item.value]?.let { result ->
                                IssueSearchRowBody(result.issue.identifier, result.issue.title, result.status)
                            }
                        },
                    )
                }
            }
        }
    }
}

