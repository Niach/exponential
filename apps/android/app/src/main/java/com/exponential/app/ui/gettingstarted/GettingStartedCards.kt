package com.exponential.app.ui.gettingstarted

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

/**
 * "What to do next", under an empty board or an empty team (EXP-698 r5) — the
 * mobile twin of web's `GettingStartedCards` and the IDE's inline cards, with
 * the same entries, order, copy and progress count.
 *
 * An empty screen is where a checklist belongs: the two places it mounts are
 * exactly the two where the app has nothing else to show, and it disappears on
 * its own once every entry is done.
 */
@Composable
fun GettingStartedCards(
    state: GettingStartedState,
    onAction: (GettingStartedEntryKey) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (state.entries.isEmpty() || state.complete) return
    Column(
        modifier = modifier.fillMaxWidth().testTag("getting-started"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SectionHeader(
            GettingStartedCopy.SECTION_TITLE,
            trailing = {
                Text(
                    "${state.done}/${state.total} done",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            },
        )
        state.entries.forEachIndexed { index, entry ->
            EntryCard(
                entry = entry,
                stepNumber = index + 1,
                loading = state.loading,
                onAction = { onAction(entry.key) },
            )
        }
    }
}

@Composable
private fun EntryCard(
    entry: GettingStartedEntry,
    stepNumber: Int,
    /** Signals still resolving: the entry reads as NEUTRAL (web parity) — no
     *  action offered on a state that may be about to flip to done. */
    loading: Boolean,
    onAction: () -> Unit,
) {
    val locked = entry.state == GettingStartedEntryState.Locked
    Column(
        modifier = Modifier
            .fillMaxWidth()
            // A locked step is still worth reading — it says what to do first
            // — so it dims rather than disappears (web parity).
            .alpha(if (locked) 0.6f else 1f)
            .glassCard()
            .padding(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            StateGlyph(state = entry.state, stepNumber = stepNumber)
            Spacer(Modifier.width(10.dp))
            Icon(
                entryIcon(entry.key),
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                GettingStartedCopy.title(entry.key),
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        }
        Spacer(Modifier.height(4.dp))
        Text(
            if (locked && entry.lockedBy != null) {
                GettingStartedCopy.lockedHint(entry.key, entry.lockedBy)
            } else {
                GettingStartedCopy.description(entry.key)
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            // Descriptions and locked hints alike are ONE SENTENCE by
            // contract and wrap on web — cut to a single line they ended
            // mid-word behind an ellipsis. Two lines fits every string in the
            // copy table at this width; the overflow is a guard, not a design.
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        // A locked step has nothing to tap yet, and a done one nothing left to
        // do — the pill is for the step you can actually take.
        if (entry.state == GettingStartedEntryState.Available && !loading) {
            Spacer(Modifier.height(10.dp))
            GlassPill(
                GettingStartedCopy.action(entry.key),
                size = PillSize.Sm,
                onClick = onAction,
            )
        }
    }
}

/** Done / locked / the step's number — the checklist's own left column. */
@Composable
private fun StateGlyph(state: GettingStartedEntryState, stepNumber: Int) {
    when (state) {
        GettingStartedEntryState.Done -> Icon(
            ExpIcons.uiSelected,
            contentDescription = null,
            modifier = Modifier.size(GlyphSize),
            tint = DesignTokens.Semantic.Green,
        )
        GettingStartedEntryState.Locked -> Icon(
            ExpIcons.uiPrivate,
            contentDescription = null,
            modifier = Modifier.size(GlyphSize),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        GettingStartedEntryState.Available -> Box(
            modifier = Modifier
                .size(GlyphSize)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                stepNumber.toString(),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

private val GlyphSize = 22.dp

private fun entryIcon(key: GettingStartedEntryKey): ImageVector = when (key) {
    GettingStartedEntryKey.Desktop -> ExpIcons.uiDevice
    GettingStartedEntryKey.Github -> ExpIcons.uiGithub
    GettingStartedEntryKey.Invite -> ExpIcons.uiInvite
    GettingStartedEntryKey.Board -> ExpIcons.navBoards
    GettingStartedEntryKey.Coding -> ExpIcons.navTerminal
    GettingStartedEntryKey.Action -> ExpIcons.actionCreate
    GettingStartedEntryKey.Server -> ExpIcons.uiServer
}
