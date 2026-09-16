package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.SearchIssueHit
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.rememberServerSearchHits
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The searchable "pick one of the team's other issues" list — the second stage
 * of both issue pickers (EXP-736): "Duplicate of…" and every relation pick.
 * Extracted from DuplicatePickerSheet so the two cannot drift; the caller owns
 * the sheet chrome and what picking does.
 */
@Composable
fun IssueCandidateList(
    candidates: List<IssueEntity>,
    onPick: (IssueEntity) -> Unit,
    placeholder: String = "Search issues",
    /**
     * EXP-892: the server-side full-text search, when the host ViewModel has
     * the API. Its hits only ever REORDER what is already here — this pool is
     * a subset of the team's issues (the current issue and its existing
     * relations are out), so a hit outside it is dropped rather than widening
     * the picker. Null = local-only.
     */
    searchServer: (suspend (String) -> List<SearchIssueHit>)? = null,
) {
    var query by remember { mutableStateOf("") }

    // The shared engine, ranked identically to the Search tab and the `#`
    // menu; an empty query lists the newest-created first.
    val hits = rememberServerSearchHits(query.takeIf { it.isNotBlank() }, searchServer)
    val filtered = remember(candidates, query, hits) {
        val local = IssueSearch.rank(candidates, query, limit = CANDIDATE_LIMIT)
        if (hits.isEmpty()) {
            local
        } else {
            val byId = candidates.associateBy { it.id }
            IssueSearch.mergeServerHits(local, hits, limit = CANDIDATE_LIMIT) { hit ->
                byId[hit.id]
            }
        }
    }

    GlassSheetSearchField(
        value = query,
        onValueChange = { query = it },
        placeholder = placeholder,
    )
    Spacer(Modifier.height(4.dp))
    if (filtered.isEmpty()) {
        Text(
            "No matching issues",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 16.dp),
        )
    } else {
        LazyColumn {
            itemsIndexed(filtered, key = { _, issue -> issue.id }) { index, issue ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        // EXP-892: the top row carries the app's active fill
                        // while a query is typed — the same "this is the best
                        // match" mark the `#` menu and `/` menu use.
                        .background(
                            if (index == 0 && query.isNotBlank()) {
                                Color.White.copy(alpha = 0.06f)
                            } else {
                                Color.Transparent
                            },
                        )
                        .clickable { onPick(issue) }
                        .padding(horizontal = 20.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    StatusIcon(IssueStatus.fromWire(issue.status), size = 16.dp)
                    Spacer(Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            issue.identifier,
                            style = MaterialTheme.typography.labelSmall,
                            fontFamily = FontFamily.Monospace,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                        Text(
                            issue.title,
                            style = MaterialTheme.typography.bodyMedium,
                            color = Color.White.copy(alpha = 0.9f),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

/** The picker never lists more than this, ranked (the web picker's cap). */
private const val CANDIDATE_LIMIT = 50
