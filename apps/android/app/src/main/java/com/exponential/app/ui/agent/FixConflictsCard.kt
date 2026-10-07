package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FixConflictsPr
import com.exponential.app.domain.branchLine
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup

// EXP-1233 — the Fix merge conflicts CARD (web `@exp/ui` FixConflictsCard,
// iOS `FixConflictsCard`, desktop `chat_screen::fix_conflicts_card`).
//
// A merge refused by a REAL conflict opens the composer on the Fix merge
// conflicts builtin with the pull request picked; the composer then draws
// that builtin with this card in place of the generic "Pull request" field:
//
//   ┌ ⑂ #2117  ⎇ exp/APP-14 → master                            ⌄ ┐  PR row
//   │ ⚠ Merge refused: the branch has conflicts.                   │  note
//   └──────────────────────────────────────────────────────────────┘
//
// The PR ROW is the picker's trigger (the chevron says so): the open-PR glyph
// in the Reviews rows' green, the number in mono foreground, the branch glyph
// and `branch → base` in mono at 70% ([branchLine], the base omitted while
// unknown); nothing picked = the muted placeholder alone. The NOTE under a
// hairline shows only when a refused merge opened the composer (the seed's
// `conflict`) and a pull request is picked. The surface is [glassGroup], the
// form ladder's own. Words = contract `composerUi*`.

/** The 70% the branch line reads at (web `text-foreground/70`). */
private const val BRANCH_ALPHA = 0.7f

/** The 50% the trailing chevron reads at (web `text-foreground/50`). */
private const val CHEVRON_ALPHA = 0.5f

@Composable
internal fun FixConflictsCard(
    /** The picked pull request; null = nothing picked (the placeholder row). */
    pr: FixConflictsPr?,
    /** A refused merge opened the composer (the seed's `conflict`). */
    conflictRefused: Boolean,
    /** The team's open pull requests — what the row's picker lists. */
    pullRequests: List<StartPullRequestOption>,
    /** The `pr` input's current value (a representative issue id, or ""). */
    value: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val items = remember(pullRequests) {
        pullRequests.map { PickerItem(value = it.issueId, label = it.label) }
    }
    Column(
        modifier = modifier
            .fillMaxWidth()
            .glassGroup()
            .testTag("agent-composer-fix-conflicts"),
    ) {
        Picker(
            items = items,
            mode = PickerMode.Single,
            value = setOfNotNull(value.takeIf { it.isNotEmpty() }),
            onChange = { picked -> picked.firstOrNull()?.let(onSelect) },
            title = "Pull request",
            enabled = items.isNotEmpty(),
        ) { open ->
            FixConflictsPrRow(pr = pr, enabled = items.isNotEmpty(), onClick = open)
        }
        if (conflictRefused && pr != null) {
            GroupDivider()
            FixConflictsNote()
        }
    }
}

/** The PR row — the picker's trigger. */
@Composable
private fun FixConflictsPrRow(pr: FixConflictsPr?, enabled: Boolean, onClick: () -> Unit) {
    val foreground = MaterialTheme.colorScheme.onSurface
    val described = when {
        pr == null -> DomainContract.composerUiPrPlaceholder
        pr.prNumber != null -> "Pull request #${pr.prNumber}"
        else -> "Pull request"
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = described }
            .testTag("agent-composer-fix-conflicts-pr")
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        if (pr != null) {
            Icon(
                ExpIcons.prOpen,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                // The Reviews rows' open-PR green (EXP-248).
                tint = DesignTokens.Semantic.Green,
            )
            pr.prNumber?.let { number ->
                Text(
                    "#$number",
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                    color = foreground,
                    maxLines = 1,
                )
            }
            val line = branchLine(pr.branch, pr.baseBranch)
            if (line.isNotEmpty()) {
                Row(
                    modifier = Modifier.weight(1f),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        ExpIcons.uiBranch,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = foreground.copy(alpha = BRANCH_ALPHA),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        line,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = FontFamily.Monospace,
                        color = foreground.copy(alpha = BRANCH_ALPHA),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            } else {
                Spacer(Modifier.weight(1f))
            }
        } else {
            Text(
                DomainContract.composerUiPrPlaceholder,
                style = MaterialTheme.typography.bodyMedium,
                // Muted: the placeholder tier every composer field wears.
                color = foreground.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        }
        Icon(
            ExpIcons.uiChevronDown,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = foreground.copy(alpha = CHEVRON_ALPHA),
        )
    }
}

/** The refusal line under the row: the warning glyph + the contract's note, destructive. */
@Composable
private fun FixConflictsNote() {
    val tone = MaterialTheme.colorScheme.error
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("agent-composer-conflict-note")
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            ExpIcons.uiWarning,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = tone,
        )
        Text(
            DomainContract.composerUiConflictNote,
            style = MaterialTheme.typography.bodyMedium,
            color = tone,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
