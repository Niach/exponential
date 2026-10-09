package com.exponential.app.ui.personal

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import com.exponential.app.ui.theme.TextEmphasis
import androidx.compose.foundation.layout.width
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
import com.exponential.app.ui.components.TabPager
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
    // EXP-878: a draft row resumes the create screen on the draft's own board.
    onOpenDraft: (boardId: String, draftId: String) -> Unit = { _, _ -> },
    // EXP-980: a blocked-run inbox row opens the run it is about.
    onOpenSession: (String) -> Unit = {},
    // EXP-933: an agent message's issue row opens that issue's Results face.
    onOpenIssueGuide: (String) -> Unit = onOpenIssue,
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
    // Drafts is offered only while there are any (EXP-878).
    val sections = buildList {
        add(SECTION_INBOX)
        add(SECTION_MY_ISSUES)
        if (hasDrafts) add(SECTION_DRAFTS)
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
                    "Inbox",
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
                options = sections,
                // Anything unknown renders the inbox (incl. saved
                // pre-EXP-147 "reviews" / pre-EXP-180 "support" values) —
                // highlight accordingly.
                selected = effective,
                label = {
                    when (it) {
                        SECTION_MY_ISSUES -> "My issues"
                        SECTION_DRAFTS -> "Drafts"
                        else -> "Inbox"
                    }
                },
                onSelect = { section = it },
                modifier = Modifier.padding(horizontal = 16.dp),
                // P17: label + a PLAIN trailing count (unread, drafts), no
                // icons, no capsule badge — one segment anatomy ×3 phones.
                trailing = { option ->
                    val count = when (option) {
                        SECTION_INBOX -> inboxState.totalUnread
                        SECTION_DRAFTS -> drafts.size
                        else -> 0
                    }
                    if (count > 0) {
                        {
                            Spacer(Modifier.width(6.dp))
                            Text(
                                count.toString(),
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        }
                    } else {
                        null
                    }
                },
            )
            Spacer(Modifier.height(8.dp))
            // EXP-1190: the sections page with a horizontal swipe (the Work
            // screen's faces), filling the column's remaining height.
            TabPager(
                pages = sections,
                selected = effective,
                onSelect = { section = it },
                key = { it },
                modifier = Modifier.weight(1f).fillMaxWidth(),
            ) { page ->
                when (page) {
                    SECTION_MY_ISSUES -> MyIssuesListContent(onOpenIssue = onOpenIssue)
                    SECTION_DRAFTS -> DraftsListContent(
                        onOpenDraft = onOpenDraft,
                        viewModel = draftsViewModel,
                    )
                    else -> InboxListContent(
                        onOpenIssue = onOpenIssue,
                        onOpenSession = onOpenSession,
                        onOpenIssueGuide = onOpenIssueGuide,
                        viewModel = inboxViewModel,
                    )
                }
            }
        }
    }
}
