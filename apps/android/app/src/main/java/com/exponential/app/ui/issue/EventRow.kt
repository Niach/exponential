package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.UserEntity

// Compact Linear-style activity line for non-agent events (status/assignee/label).
// Extracted from CommentThread.kt (pure move — no behavior change).
@Composable
internal fun EventRow(event: IssueEventEntity, actor: UserEntity?) {
    val who = actor?.name ?: actor?.email ?: "Someone"
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(CommentMeta))
        Text(
            "$who ${eventVerb(event.type)} · ${relativeTime(event.createdAt)}",
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
        )
    }
}

// Human-readable verb for a synced issue event. Covers the surviving event kinds
// (issueEventTypeValues in the domain contract); anything else degrades to the
// type name with underscores spaced out.
internal fun eventVerb(type: String): String = when (type) {
    "status_changed" -> "changed the status"
    "assignee_changed" -> "changed the assignee"
    "label_added" -> "added a label"
    "label_removed" -> "removed a label"
    "pr_opened" -> "opened a pull request"
    "pr_merged" -> "merged the pull request"
    else -> type.replace('_', ' ')
}
