package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.outlined.Circle
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
import com.exponential.app.data.db.UserEntity

/**
 * Wraps the [IssueRow] in a Material 3 [SwipeToDismissBox] with status-change
 * swipes that mirror the iOS issue list:
 *  - EndToStart (swipe right-to-left) → mark Done.
 *  - StartToEnd (swipe left-to-right) → move to Backlog.
 *
 * Backed by [confirmValueChange] so each gesture only fires once (when threshold is
 * crossed) and the row always snaps back to [SwipeToDismissBoxValue.Settled] — the
 * row stays in the list under its new status section.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SwipeableIssueRow(
    issue: IssueEntity,
    labels: List<LabelEntity>,
    assignee: UserEntity?,
    canMutate: Boolean,
    onMarkDone: () -> Unit,
    onMoveToBacklog: () -> Unit,
    onClick: () -> Unit,
) {
    if (!canMutate) {
        IssueRow(issue, labels, assignee, onClick)
        return
    }

    val dismissState = rememberSwipeToDismissBoxState(
        confirmValueChange = { target ->
            when (target) {
                SwipeToDismissBoxValue.EndToStart -> onMarkDone()
                SwipeToDismissBoxValue.StartToEnd -> onMoveToBacklog()
                SwipeToDismissBoxValue.Settled -> Unit
            }
            // Never actually dismiss; status changes move the row to another
            // section but it stays in the list.
            false
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
        IssueRow(issue, labels, assignee, onClick)
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
            container = MaterialTheme.colorScheme.tertiaryContainer,
            content = MaterialTheme.colorScheme.onTertiaryContainer,
            icon = Icons.Outlined.Circle,
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
