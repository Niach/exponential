package com.exponential.app.ui.personal

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.Chats
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.PinnedHeaderColumn
import com.exponential.app.ui.components.PageHeaderCircle
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.inbox.InboxListContent
import com.exponential.app.ui.inbox.InboxViewModel

/**
 * Session 58cb32df (chats.json `shell`): the INBOX page — pushed from the
 * Chats header's inbox circle, no longer a tab and no segments (My issues
 * and Drafts moved to the board's chips). Back · `copy.inbox` · Mark all
 * read over the active team's inbox ([InboxListContent]).
 */
@Composable
fun PersonalScreen(
    onOpenIssue: (String) -> Unit,
    onBack: () -> Unit = {},
    // EXP-980: a blocked-run inbox row opens the run it is about.
    onOpenSession: (String) -> Unit = {},
    // #EXP-1534: an agent_signed_out row opens its device's settings.
    onOpenDevice: (String) -> Unit = {},
    // EXP-933: an agent message's issue row opens that issue's Results face.
    onOpenIssueGuide: (String) -> Unit = onOpenIssue,
    inboxViewModel: InboxViewModel = hiltViewModel(),
) {
    val inboxState by inboxViewModel.state.collectAsStateWithLifecycle()
    val markAllFailure by inboxViewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(markAllFailure, onShown = inboxViewModel::clearFailure)
    Scaffold(containerColor = Color.Transparent) { padding ->
        PinnedHeaderColumn(
            modifier = Modifier.padding(padding).fillMaxSize().testTag("inbox-page"),
            header = {
                PageHeader(
                    title = Chats.Copy.INBOX,
                    onBack = onBack,
                    ownActions = if (inboxState.totalUnread > 0) {
                        {
                            PageHeaderCircle(
                                ExpIcons.notificationMarkRead,
                                "Mark all read",
                                onClick = { inboxViewModel.markAllRead() },
                                modifier = Modifier.testTag("inbox-mark-all-read"),
                            )
                        }
                    } else {
                        null
                    },
                )
            },
        ) {
            InboxListContent(
                onOpenIssue = onOpenIssue,
                onOpenSession = onOpenSession,
                onOpenDevice = onOpenDevice,
                onOpenIssueGuide = onOpenIssueGuide,
                viewModel = inboxViewModel,
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}
