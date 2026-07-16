package com.exponential.app.ui.personal

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
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
import com.exponential.app.ui.inbox.InboxListContent
import com.exponential.app.ui.inbox.InboxViewModel
import com.exponential.app.ui.myissues.MyIssuesListContent
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton

/**
 * The personal tab ("My Work", EXP-58): Inbox and My Issues merged into one
 * project-independent surface — the same pairing the web UI keeps at the top
 * of its sidebar (Inbox + My Issues), folded into a single bottom-bar
 * destination behind a segmented control. This replaces both the old routed
 * Inbox screen and the My Issues list that used to hide inside the Search
 * tab's empty-query state; Search is a pure search screen again.
 */

// rememberSaveable-friendly segment keys (plain strings, no custom Saver).
// Reviews moved out to its own bottom-bar destination (EXP-147).
private const val SECTION_INBOX = "inbox"
private const val SECTION_MY_ISSUES = "my_issues"

@Composable
fun PersonalScreen(
    onOpenIssue: (String) -> Unit,
    inboxViewModel: InboxViewModel = hiltViewModel(),
) {
    val inboxState by inboxViewModel.state.collectAsStateWithLifecycle()
    var section by rememberSaveable { mutableStateOf(SECTION_INBOX) }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            Text(
                "My Work",
                style = MaterialTheme.typography.headlineLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
            )
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    // Reserve the TextButton's height even while "Mark all
                    // read" is hidden so toggling segments never shifts the
                    // lists below.
                    .heightIn(min = 40.dp)
                    .padding(horizontal = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                SegmentPill(
                    label = "Inbox",
                    // Anything but my_issues renders the inbox (incl. a saved
                    // pre-EXP-147 "reviews" value) — highlight accordingly.
                    active = section != SECTION_MY_ISSUES,
                    unread = inboxState.totalUnread,
                    onClick = { section = SECTION_INBOX },
                )
                Spacer(Modifier.width(8.dp))
                SegmentPill(
                    label = "My Issues",
                    active = section == SECTION_MY_ISSUES,
                    onClick = { section = SECTION_MY_ISSUES },
                )
                Spacer(Modifier.weight(1f))
                if (section != SECTION_MY_ISSUES && inboxState.totalUnread > 0) {
                    TextButton(onClick = { inboxViewModel.markAllRead() }) {
                        Text("Mark all read")
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
            when (section) {
                SECTION_MY_ISSUES -> MyIssuesListContent(onOpenIssue = onOpenIssue)
                else -> InboxListContent(
                    onOpenIssue = onOpenIssue,
                    viewModel = inboxViewModel,
                )
            }
        }
    }
}

/** Capsule glass segment (the iOS filter-pill style) with an unread count. */
@Composable
private fun SegmentPill(
    label: String,
    active: Boolean,
    onClick: () -> Unit,
    unread: Int = 0,
) {
    Row(
        modifier = Modifier
            .glassButton(active = active)
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (active) 1f else TextEmphasis.Secondary,
            ),
        )
        if (unread > 0) {
            Spacer(Modifier.width(6.dp))
            Text(
                unread.toString(),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}
