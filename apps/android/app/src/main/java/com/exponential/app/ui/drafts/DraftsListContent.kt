package com.exponential.app.ui.drafts

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The Drafts section of My Work (EXP-878) — one band over flat rows, like
 * every other list since EXP-818. The section only OFFERS itself while
 * resolved drafts exist (PersonalScreen gates the segment on the same flow),
 * so the empty state here is the vanishing race, not a normal destination.
 */
@Composable
fun DraftsListContent(
    onOpenDraft: (boardId: String, draftId: String) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: DraftsViewModel = hiltViewModel(),
) {
    val drafts by viewModel.drafts.collectAsStateWithLifecycle()

    if (drafts.isEmpty()) {
        EmptyState(
            message = "No drafts",
            icon = ExpIcons.navDrafts,
            modifier = modifier,
        )
        return
    }

    LazyColumn(
        modifier = modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        item(key = "drafts-header") {
            SectionHeader(
                title = "Drafts",
                trailing = {
                    Text(
                        drafts.size.toString(),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
            )
        }
        items(drafts, key = { it.draft.id }) { row ->
            DraftListRow(
                row = row,
                onOpen = { onOpenDraft(row.draft.boardId, row.draft.id) },
                onDelete = { viewModel.delete(row.draft.id) },
            )
        }
    }
}
