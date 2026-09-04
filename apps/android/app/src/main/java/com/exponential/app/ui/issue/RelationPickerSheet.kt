package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.RelationPick
import com.exponential.app.domain.relationPicks
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * "Add relation" (EXP-736), in two stages inside ONE sheet: pick the KIND of
 * relation, then the issue it points at. Both stages share the sheet so the
 * back-and-forth doesn't remount the bottom sheet — the title is what changes.
 *
 * The pick carries its own direction ("Blocked by" is the inverse side of
 * `blocks`); the ViewModel turns it into the canonical create input, or into
 * the duplicate lockstep write.
 */
@Composable
fun RelationPickerSheet(
    candidates: List<IssueEntity>,
    onPick: (RelationPick, IssueEntity) -> Unit,
    onDismiss: () -> Unit,
) {
    var stage by remember { mutableStateOf<RelationPick?>(null) }
    val pick = stage

    GlassSheet(title = pick?.title ?: "Add relation", onDismiss = onDismiss) {
        if (pick == null) {
            Column(modifier = Modifier.fillMaxWidth()) {
                relationPicks.forEach { entry ->
                    GlassSheetRow(
                        label = entry.title,
                        leading = {
                            Icon(
                                entry.icon,
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                            )
                        },
                        onClick = { stage = entry },
                    )
                }
                Spacer(Modifier.height(8.dp))
            }
        } else {
            IssueCandidateList(
                candidates = candidates,
                onPick = { issue ->
                    onPick(pick, issue)
                    onDismiss()
                },
            )
        }
    }
}
