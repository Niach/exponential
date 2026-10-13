package com.exponential.app.ui.thread

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.TopBarBackButton

/**
 * EXP-1291 Threads v2: THE conversation screen for a thread root OR an agent
 * root (`thread/{sessionId}`) — lane THREAD builds the real page
 * (`session-thread.json`): the pager tabs Conversation · one per issue · + ·
 * Guide, the conversation rows ([SessionTree.ThreadRow]: messages, session
 * cards, question / plan / subagent / picture / event cards), the queue bar
 * and the composer ([ThreadViewModel.send]); an agent channel shows its
 * conversation only.
 *
 * Navigation out: [onOpenSession] = a session card's raw view
 * (`session/{id}`), [onOpenIssue] = an issue, [onOpenThread] = another
 * thread (a `thread_started` card), [onOpenAgentSettings] = an agent root's
 * settings page.
 *
 * STUB (compiling, minimal body) — final signature.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ThreadScreen(
    onBack: () -> Unit,
    onOpenSession: (sessionId: String) -> Unit,
    onOpenIssue: (issueId: String) -> Unit,
    onOpenThread: (rootSessionId: String) -> Unit,
    onOpenAgentSettings: (agentSessionId: String) -> Unit,
    viewModel: ThreadViewModel = hiltViewModel(),
) {
    val view by viewModel.view.collectAsStateWithLifecycle()
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = { Text(view?.title.orEmpty(), maxLines = 1) },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        val rows = view?.rows.orEmpty()
        LazyColumn(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .testTag(SessionTree.ThreadCopy.TEST_ID_CONVERSATION),
        ) {
            items(rows, key = { it.id }) { row ->
                when (row) {
                    is SessionTree.ThreadRow.SessionRow -> Text(
                        "${row.card.name} · ${row.card.kindWord}",
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onOpenSession(row.card.sessionId) }
                            .padding(16.dp)
                            .testTag(row.card.testId),
                    )
                    is SessionTree.ThreadRow.MessageRow -> Text(row.body, modifier = Modifier.padding(16.dp))
                    is SessionTree.ThreadRow.Event -> Text(row.text, modifier = Modifier.padding(16.dp))
                    else -> Unit
                }
            }
        }
    }
}
