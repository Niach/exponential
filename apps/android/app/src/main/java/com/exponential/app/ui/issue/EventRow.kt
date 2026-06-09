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
            "$who ${agentEventVerb(event.type)} · ${relativeTime(event.createdAt)}",
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
        )
    }
}
