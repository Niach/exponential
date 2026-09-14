package com.exponential.app.ui.personal

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.drafts.DraftsListContent
import com.exponential.app.ui.drafts.DraftsViewModel
import com.exponential.app.ui.inbox.InboxListContent
import com.exponential.app.ui.inbox.InboxViewModel
import com.exponential.app.ui.myissues.MyIssuesListContent

/**
 * The personal tab ("My Work", EXP-58): Inbox and My Issues merged into one
 * board-independent surface — the same pairing the web UI keeps at the top
 * of its sidebar (Inbox + My Issues), folded into a single bottom-bar
 * destination behind a segmented control. This replaces both the old routed
 * Inbox screen and the My Issues list that used to hide inside the Search
 * tab's empty-query state; Search is a pure search screen again.
 */

// rememberSaveable-friendly segment keys (plain strings, no custom Saver).
// Reviews (EXP-147) and Support (EXP-180) each moved out to their own
// bottom-bar destinations.
private const val SECTION_INBOX = "inbox"
private const val SECTION_MY_ISSUES = "my_issues"
// EXP-878: unsent issue drafts. The segment only EXISTS while there are
// resolved drafts — an empty third option would be a permanent dead end.
private const val SECTION_DRAFTS = "drafts"

@Composable
fun PersonalScreen(
    onOpenIssue: (String) -> Unit,
    onOpenSupport: () -> Unit,
    // EXP-878: a draft row resumes the create screen on the draft's own board.
    onOpenDraft: (boardId: String, draftId: String) -> Unit = { _, _ -> },
    inboxViewModel: InboxViewModel = hiltViewModel(),
    draftsViewModel: DraftsViewModel = hiltViewModel(),
) {
    val inboxState by inboxViewModel.state.collectAsStateWithLifecycle()
    val drafts by draftsViewModel.drafts.collectAsStateWithLifecycle()
    val hasDrafts = drafts.isNotEmpty()
    var section by rememberSaveable { mutableStateOf(SECTION_INBOX) }
    // A persisted "drafts" pick falls back to the inbox once the last draft is
    // gone — the same tolerance the pre-EXP-147 "reviews" value gets.
    val effective = when {
        section == SECTION_MY_ISSUES -> SECTION_MY_ISSUES
        section == SECTION_DRAFTS && hasDrafts -> SECTION_DRAFTS
        else -> SECTION_INBOX
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // "Mark all read" rides the title row — the iOS top-bar-trailing
            // placement — now that the segmented control spans full width.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "My Work",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Spacer(Modifier.weight(1f))
                if (effective == SECTION_INBOX && inboxState.totalUnread > 0) {
                    TextButton(onClick = { inboxViewModel.markAllRead() }) {
                        Text("Mark all read")
                    }
                }
            }
            GlassSegmentedControl(
                // Drafts is offered only while there are any (EXP-878).
                options = buildList {
                    add(SECTION_INBOX)
                    add(SECTION_MY_ISSUES)
                    if (hasDrafts) add(SECTION_DRAFTS)
                },
                // Anything unknown renders the inbox (incl. saved
                // pre-EXP-147 "reviews" / pre-EXP-180 "support" values) —
                // highlight accordingly.
                selected = effective,
                label = {
                    when (it) {
                        SECTION_MY_ISSUES -> "My Issues"
                        SECTION_DRAFTS -> "Drafts"
                        else -> "Inbox"
                    }
                },
                onSelect = { section = it },
                modifier = Modifier.padding(horizontal = 16.dp),
                badge = { if (it == SECTION_INBOX) inboxState.totalUnread else 0 },
            )
            Spacer(Modifier.height(8.dp))
            when (effective) {
                SECTION_MY_ISSUES -> MyIssuesListContent(onOpenIssue = onOpenIssue)
                SECTION_DRAFTS -> DraftsListContent(
                    onOpenDraft = onOpenDraft,
                    viewModel = draftsViewModel,
                )
                else -> InboxListContent(
                    onOpenIssue = onOpenIssue,
                    onOpenSupport = onOpenSupport,
                    viewModel = inboxViewModel,
                )
            }
        }
    }
}
