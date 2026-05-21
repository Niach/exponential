package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.LabelEntity

/**
 * Wraps [content] (typically the [IssueRow]) in a Material 3 [SwipeToDismissBox] with:
 *  - EndToStart (swipe right-to-left) → mark Done.
 *  - StartToEnd (swipe left-to-right) → mark Cancelled.
 *
 * Backed by [confirmValueChange] so each gesture only fires once (when threshold is
 * crossed). The state snaps back to [SwipeToDismissBoxValue.Settled] after the action
 * runs, since the row stays in the list (the grouped layout will re-bucket it).
 *
 * Note on "archive": the cross-platform plan calls for StartToEnd to archive (set
 * `archivedAt`). The backend `issues.update` tRPC procedure doesn't currently accept
 * `archivedAt`, so we use Cancelled as the closest non-destructive equivalent.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SwipeableIssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    canMutate: Boolean,
    onMarkDone: () -> Unit,
    onMarkCancelled: () -> Unit,
    onClick: () -> Unit,
) {
    if (!canMutate) {
        IssueRow(issue, labels, onClick)
        return
    }

    val dismissState = rememberSwipeToDismissBoxState(
        confirmValueChange = { target ->
            when (target) {
                SwipeToDismissBoxValue.EndToStart -> {
                    onMarkDone()
                    // Don't actually dismiss; we want the row to snap back.
                    false
                }
                SwipeToDismissBoxValue.StartToEnd -> {
                    onMarkCancelled()
                    false
                }
                SwipeToDismissBoxValue.Settled -> true
            }
        },
        positionalThreshold = { distance -> distance * 0.35f },
    )

    // Safety net: if confirmValueChange ever lets a non-settled state through,
    // reset on the next composition so the row stays visible.
    LaunchedEffect(dismissState.currentValue) {
        if (dismissState.currentValue != SwipeToDismissBoxValue.Settled) {
            dismissState.reset()
        }
    }

    SwipeToDismissBox(
        state = dismissState,
        backgroundContent = { SwipeBackground(dismissState.dismissDirection) },
        enableDismissFromStartToEnd = true,
        enableDismissFromEndToStart = true,
    ) {
        IssueRow(issue, labels, onClick)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SwipeBackground(direction: SwipeToDismissBoxValue) {
    val (containerColor, contentColor, icon, alignment) = when (direction) {
        SwipeToDismissBoxValue.EndToStart -> SwipeBg(
            container = MaterialTheme.colorScheme.primary,
            content = MaterialTheme.colorScheme.onPrimary,
            icon = Icons.Filled.Check,
            alignment = Alignment.CenterEnd,
        )
        SwipeToDismissBoxValue.StartToEnd -> SwipeBg(
            container = MaterialTheme.colorScheme.errorContainer,
            content = MaterialTheme.colorScheme.onErrorContainer,
            icon = Icons.Filled.Cancel,
            alignment = Alignment.CenterStart,
        )
        SwipeToDismissBoxValue.Settled -> SwipeBg(
            container = Color.Transparent,
            content = MaterialTheme.colorScheme.onSurface,
            icon = null,
            alignment = Alignment.Center,
        )
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(containerColor)
            .padding(horizontal = 20.dp),
        contentAlignment = alignment,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.Center,
        ) {
            if (icon != null) {
                Icon(
                    icon,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(24.dp),
                )
            }
        }
    }
}

private data class SwipeBg(
    val container: Color,
    val content: Color,
    val icon: androidx.compose.ui.graphics.vector.ImageVector?,
    val alignment: Alignment,
)
