package com.exponential.app.ui.threads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.SectionHeader

/**
 * EXP-1291: the Threads TAB (it replaced Reviews) — lane THREADS builds the
 * real page (`threads-list.json`): groups Needs you · Running · Recent,
 * banded per team on phones, the Agents section. A row opens its thread
 * ([onOpenThread] = `thread/{rootId}`); an agent row opens its channel (the
 * same thread screen, [onOpenThread] with the agent root id).
 *
 * STUB (compiling, minimal body) — final signature.
 */
@Composable
fun ThreadsScreen(
    onOpenThread: (rootSessionId: String) -> Unit,
    viewModel: ThreadsViewModel = hiltViewModel(),
) {
    val groups by viewModel.groups.collectAsStateWithLifecycle()
    Scaffold(containerColor = Color.Transparent) { padding ->
        LazyColumn(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .testTag(SessionTree.ListCopy.TEST_ID_LIST),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = BottomBarInset),
        ) {
            item(key = "__title__") {
                Text(
                    SessionTree.ListCopy.TITLE,
                    style = MaterialTheme.typography.headlineLarge,
                    modifier = Modifier.padding(top = 8.dp, bottom = 12.dp),
                )
            }
            if (groups.isEmpty()) {
                item(key = "__empty__") { Text(SessionTree.ListCopy.EMPTY) }
            }
            groups.forEach { group ->
                item(key = "group_${group.key.wire}") { SectionHeader(title = group.title) }
                items(group.rows, key = { it.sessionId }) { row ->
                    Text(
                        row.title,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onOpenThread(row.sessionId) }
                            .padding(vertical = 10.dp)
                            .testTag(SessionTree.ListCopy.TEST_ID_ROW),
                    )
                }
            }
        }
    }
}
