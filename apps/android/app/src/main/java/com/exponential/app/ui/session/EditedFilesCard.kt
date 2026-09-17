package com.exponential.app.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.EditCard
import com.exponential.app.domain.diffTruncationNote
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DiffCardState
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// EXP-916 — the EDITED-FILES card: ONE card per run of consecutive edit calls
// ([com.exponential.app.domain.AgentFeedRow.Edits]), one FLUSH [DiffFileCard]
// per path inside it. Byte-identical ×4 (web `@exp/ui` `EditedFilesCard`,
// desktop `ui::edit_card`, iOS `EditedFilesCard.swift`): the title, the fold
// label and the preview count all come from `EditCard` / the contract.
//
// The card is TERMINAL: a tap opens a file's patch in place, and nothing on it
// ever navigates to the Changes face. No "Revert", no "Show changes".

/** The inline patch's ceiling — the contract's number, `max-h-72` on the web. */
private val InlineDiffMaxHeight = DomainContract.diffUiInlineDiffMaxHeight.dp

@Composable
fun EditedFilesCard(
    items: List<AgentFeedItem.Tool>,
    /** The id [com.exponential.app.domain.liveToolRowId] names, or null. */
    liveItemId: Long?,
    modifier: Modifier = Modifier,
) {
    if (items.isEmpty()) return
    val rowId = items.first().id
    val lastId = items.last().id
    // Parsing every member's patch is the expensive part, so the card is keyed
    // on what can actually change it: which members it covers, how much patch
    // has landed, how many have settled, and which row is live. Never a
    // per-recomposition re-parse.
    val totalDiffLength = items.sumOf { it.diff?.length ?: 0 }
    val settledCount = items.count { it.settled }
    val failedCount = items.count { it.failed }
    val card = remember(rowId, lastId, totalDiffLength, settledCount, failedCount, liveItemId) {
        EditCard.editCard(items, liveItemId)
    }
    val preview = DomainContract.diffUiCardPreviewFiles
    val liveIndex = card.liveIndex
    // The fold only ever hides rows the reader is not being shown live.
    var showAll by remember(rowId) { mutableStateOf(false) }
    val clamped = liveIndex != null && liveIndex >= preview
    val open = showAll || clamped
    val shown = if (open) card.rows else card.rows.take(preview)
    val more = EditCard.editCardMoreLabel(card.rows.size)

    // The card OWNS which files are open, as ONE nullable set. `null` = follow
    // the live row: while the card's last member is the LIVE tool row that row
    // is open by itself, so the reader watches the edit land, and the card
    // folds itself the moment it settles. A tap replaces it with the reader's
    // OWN set — seeded from what is effectively open, so the live row can be
    // collapsed like any other — and that set survives the settle, because
    // their opens are theirs.
    val livePath = liveIndex?.let { card.rows.getOrNull(it)?.path }
    var reader by remember(rowId) { mutableStateOf<Set<String>?>(null) }
    val openPaths = reader ?: if (livePath == null) emptySet() else setOf(livePath)

    Column(
        modifier = modifier.fillMaxWidth().glassCard().testTag("edited-files-card"),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                ExpIcons.codingDiff,
                contentDescription = null,
                modifier = Modifier.size(12.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                card.title,
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        shown.forEach { row ->
            key(row.path) {
                HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
                val expanded = row.path in openPaths
                val body: @Composable () -> Unit = {
                    DiffFileCard(
                        file = row.file ?: Diff.File(path = row.path),
                        expanded = expanded,
                        onToggle = {
                            reader = if (expanded) openPaths - row.path else openPaths + row.path
                        },
                        compact = true,
                        flush = true,
                        state = when (row.state) {
                            EditCard.RowState.READY -> DiffCardState.Ready
                            EditCard.RowState.PENDING -> DiffCardState.Pending
                            EditCard.RowState.DONE -> DiffCardState.Done
                            EditCard.RowState.FAILED -> DiffCardState.Failed
                        },
                    )
                }
                // An open patch — the live one grows while the call runs —
                // gets a bounded scroll box instead of pushing the
                // conversation off screen.
                if (expanded) {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = InlineDiffMaxHeight)
                            .verticalScroll(rememberScrollState()),
                    ) { body() }
                } else {
                    body()
                }
            }
        }
        // EXP-786: what the PUBLISHER cut off the members' patches — the same
        // note a file list carries, so a card never silently shows a short
        // diff.
        if (card.truncatedLines > 0) {
            HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            Text(
                diffTruncationNote(card.truncatedLines),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("edited-files-truncation-note")
                    .padding(horizontal = 12.dp, vertical = 5.dp),
            )
        }
        // `N more` / `Show less` — the contract's words. Clamped open while the
        // live row sits past the preview: folding it away would hide the one
        // thing the reader is watching.
        if (more != null && !clamped) {
            HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
            Text(
                if (showAll) DomainContract.diffUiShowLess else more,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .fillMaxWidth()
                    .testTag("edited-files-more")
                    .clickable { showAll = !showAll }
                    .padding(horizontal = 12.dp, vertical = 9.dp),
            )
        }
    }
}
