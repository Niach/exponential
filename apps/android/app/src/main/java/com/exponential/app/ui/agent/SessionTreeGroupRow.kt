package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.issue.LiveGreen
import com.exponential.app.ui.issue.NeedsInputAmber
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.issue.StaticDot
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.session.SessionRowTitle
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

/**
 * EXP-996/EXP-1050: the GROUP row of the session tree — the workflow or the
 * stack the runs below it belong to.
 *
 * The identity line is [SessionRowTitle], the same one every session row
 * draws, so the two keep one height rhythm; the concept icon takes the state
 * dot's slot, because a group has no state of its own: no live dot, no
 * machine, no Stop. The trailing count is the row's own fact — a group IS its
 * children, so how many there are is what folding hides. The chevron folds and
 * the rest of the row opens the workflow (a stack is not a place you can go:
 * its members are its only page) — the same split ×4.
 *
 * EXP-1068: a WORKFLOW group adds a status dot (contract `wfStatus`) beside
 * its icon and trails `SessionTree.workflowGroupCaption` instead of the bare
 * count; a stack group keeps its count.
 */
@Composable
internal fun SessionTreeGroupRow(
    /** By CONCEPT only — `navWorkflows` or `prStack`. */
    icon: ImageVector,
    label: String,
    /** How many nodes sit under this row. */
    count: Int,
    /** The node key (`workflow:<id>` / `stack:<issueId>`), for the row's tag. */
    nodeKey: String,
    expanded: Boolean,
    onToggle: () -> Unit,
    modifier: Modifier = Modifier,
    /** Where the row's own tap goes — a workflow group opens its workflow. */
    onClick: (() -> Unit)? = null,
    /** A workflow group's contract `wfStatus` — draws the status dot. */
    workflowStatus: String? = null,
    /** Replaces the trailing count (a workflow's `3 running · 5 of 8 done`). */
    caption: String? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("session-group-$nodeKey")
            .flatRow()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // The same fold affordance a run row wears, with a GROUP's own words:
        // "child runs" is what a run has, not a group (×4).
        Box(
            modifier = Modifier
                .size(20.dp)
                .clickable(onClick = onToggle)
                .testTag("session-fold"),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                if (expanded) ExpIcons.uiChevronDown else ExpIcons.uiChevronRight,
                contentDescription = if (expanded) {
                    SessionTree.COLLAPSE_GROUP_LABEL
                } else {
                    SessionTree.EXPAND_GROUP_LABEL
                },
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        Spacer(Modifier.width(4.dp))
        SessionRowTitle(
            identifier = null,
            title = label,
            modifier = Modifier.weight(1f),
            dot = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        icon,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                    if (workflowStatus != null) {
                        Spacer(Modifier.width(6.dp))
                        Box(Modifier.testTag("session-group-status-$workflowStatus")) {
                            StaticDot(workflowStatusColor(workflowStatus), size = 6.dp)
                        }
                    }
                }
            },
        )
        Spacer(Modifier.width(8.dp))
        Text(
            caption ?: count.toString(),
            style = MaterialTheme.typography.labelSmall,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/** The group dot's colour per contract `wfStatus` — the run rows' state-dot
 *  palette: draft/cancelled quiet, running live, paused amber, done green. */
@Composable
private fun workflowStatusColor(status: String): Color = when (status) {
    DomainContract.wfStatusRunning -> LiveGreen
    DomainContract.wfStatusPaused -> NeedsInputAmber
    DomainContract.wfStatusDone -> ReviewGreen
    else -> MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
}
