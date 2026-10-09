package com.exponential.app.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.ListItem
import com.exponential.app.domain.PrNodeRing
import com.exponential.app.domain.PrNodeState
import com.exponential.app.domain.TreeGuide
import com.exponential.app.domain.TreeGuides
import com.exponential.app.ui.issue.ReviewGreen
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// EXP-1248: THE pull-request row, x4 (web `@exp/ui` pr-row.tsx, desktop
// `pr_rows.rs`, iOS `PrRow.swift`; fixture `list-item.json`): ONE line,
// [ring lead · mono identifier · title · quiet word]. No branch line, no PR
// number, no counts, no age, no inline Merge. Reviews, PR trees and the
// Guide's stack card all draw it. Two shapes: a TREE nests with tree guides
// ([PrList]), a linear STACK never nests, it hangs off one rail down to its
// base branch ([StackRail]).

/** The ring lead: 12dp ring in a 14dp box (like the run mark, so tree guides
 *  land on its centre); emerald = open, filled centre = current, muted = base. */
@Composable
fun PrNode(state: PrNodeState, modifier: Modifier = Modifier) {
    val ring = when (state.ring) {
        PrNodeRing.Emerald -> ReviewGreen
        PrNodeRing.Muted -> MaterialTheme.colorScheme.onSurface.copy(alpha = PR_BASE_RING_ALPHA)
    }
    Box(
        modifier = modifier.size(ListItem.MARK_DP.dp).testTag("pr-node-${state.name.lowercase()}"),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(ListItem.PR_NODE_DP.dp)
                .border(1.5.dp, ring, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            if (state.filled) {
                Box(Modifier.size(6.dp).background(ring, CircleShape).testTag("pr-node-fill"))
            }
        }
    }
}

/** Which rail segments a stack row draws: into the node from above, out of it
 *  below. They stop at the ring, never cross it. */
data class PrRail(val above: Boolean = false, val below: Boolean = false)

/** One PR row: 40dp on phones (`list-item.json` prRowPhone). */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun PrRow(
    title: String,
    modifier: Modifier = Modifier,
    node: PrNodeState = PrNodeState.Open,
    identifier: String? = null,
    /** A quiet trailing word (`stack` on a stack's top row, `draft`). */
    word: String? = null,
    depth: Int = 0,
    /** Tree shape: this row's connector geometry. */
    guide: TreeGuide? = null,
    /** Stack shape: the rail through the node centres. */
    rail: PrRail? = null,
    active: Boolean = false,
    onClick: (() -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    /** Trailing content after the word (an external-link glyph). */
    trailing: (@Composable () -> Unit)? = null,
    testTag: String = PR_ROW_TAG,
) {
    val base = node == PrNodeState.Base
    val railColor = GlassTokens.StrokeStrong
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(ListItem.PR_ROW_PHONE_DP.dp)
            .testTag(testTag)
            .treeGuides(guide)
            .then(
                if (rail == null) {
                    Modifier
                } else {
                    Modifier.drawBehind {
                        val x = TreeGuides.centre(depth, ListItem.BASE_DP.dp.toPx(), ListItem.INDENT_DP.dp.toPx())
                        val half = ListItem.PR_NODE_DP.dp.toPx() / 2f
                        val mid = size.height / 2f
                        val stroke = ListItem.RAIL_DP.dp.toPx()
                        if (rail.above) drawLine(railColor, Offset(x, 0f), Offset(x, mid - half), stroke)
                        if (rail.below) drawLine(railColor, Offset(x, mid + half), Offset(x, size.height), stroke)
                    }
                },
            )
            .flatRow(active)
            .then(
                if (onClick == null && onLongClick == null) {
                    Modifier
                } else {
                    Modifier.combinedClickable(onClick = { onClick?.invoke() }, onLongClick = onLongClick)
                },
            )
            .padding(start = ListItem.leadX(depth).dp, end = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ListItem.GAP_DP.dp),
    ) {
        PrNode(node)
        if (!identifier.isNullOrEmpty()) {
            Text(
                identifier,
                style = MaterialTheme.typography.labelMedium,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                modifier = Modifier.widthIn(min = 72.dp),
            )
        }
        Text(
            title,
            style = if (base) MaterialTheme.typography.labelMedium else MaterialTheme.typography.bodyMedium,
            fontFamily = if (base) FontFamily.Monospace else null,
            color = if (base) {
                MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
            } else {
                MaterialTheme.colorScheme.onSurface
            },
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (!word.isNullOrEmpty()) {
            Text(
                word,
                fontSize = 11.5.sp,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                modifier = Modifier.testTag("pr-row-word"),
            )
        }
        trailing?.invoke()
    }
}

/** One row of a PR tree: [depth] nests it under its parent. */
data class PrListRow(
    val key: String,
    val title: String,
    val identifier: String? = null,
    val depth: Int = 0,
    val node: PrNodeState = PrNodeState.Open,
    val word: String? = null,
    val active: Boolean = false,
    val onOpen: (() -> Unit)? = null,
    val testTag: String = PR_ROW_TAG,
    val trailing: (@Composable () -> Unit)? = null,
)

/** A PR TREE (or a flat run of single PRs): rows nest with tree guides,
 *  gapless (nothing for a connector to bridge). */
@Composable
fun PrList(rows: List<PrListRow>, modifier: Modifier = Modifier) {
    val guides = remember(rows.map { it.depth }) { TreeGuides.compute(rows.map { it.depth }) }
    Column(modifier = modifier.fillMaxWidth().testTag("pr-list")) {
        rows.forEachIndexed { index, row ->
            PrRow(
                title = row.title,
                node = row.node,
                identifier = row.identifier,
                word = row.word,
                depth = row.depth,
                guide = guides.getOrNull(index),
                active = row.active,
                onClick = row.onOpen,
                trailing = row.trailing,
                testTag = row.testTag,
            )
        }
    }
}

/** One member of a linear stack; open so a caller can carry what a row opens. */
open class StackRailMember(
    val key: String,
    val identifier: String,
    val title: String,
    /** The member the page shows: the filled node + the active wash. */
    val current: Boolean = false,
)

/**
 * A STACK, top-first, on one rail down to its base-branch row. Phones reach
 * "Merge through here" ([onMergeThrough]) from a member's LONG-PRESS menu
 * (the desktop/web hover ghost has no touch twin).
 */
@Composable
fun <M : StackRailMember> StackRail(
    /** Top member first. */
    members: List<M>,
    baseBranch: String,
    modifier: Modifier = Modifier,
    /** The quiet word on the top row (`stack` in Reviews; none on the Guide). */
    word: String? = null,
    onOpen: ((M) -> Unit)? = null,
    onMergeThrough: ((M) -> Unit)? = null,
    mergeThroughLabel: String = DomainContract.diffUiMergeThrough,
) {
    var menuFor by remember { mutableStateOf<String?>(null) }
    Column(modifier = modifier.fillMaxWidth().testTag("stack-rail")) {
        members.forEachIndexed { index, member ->
            Box {
                PrRow(
                    title = member.title,
                    node = if (member.current) PrNodeState.Current else PrNodeState.Open,
                    identifier = member.identifier,
                    word = if (index == 0) word else null,
                    rail = PrRail(above = index > 0, below = true),
                    active = member.current,
                    onClick = onOpen?.let { open -> { open(member) } },
                    onLongClick = onMergeThrough?.let { { menuFor = member.key } },
                )
                if (onMergeThrough != null) {
                    GlassDropdownMenu(
                        expanded = menuFor == member.key,
                        onDismissRequest = { menuFor = null },
                    ) {
                        GlassMenuItem(
                            text = { Text(mergeThroughLabel) },
                            onClick = {
                                menuFor = null
                                onMergeThrough(member)
                            },
                            modifier = Modifier.testTag("stack-merge-through"),
                        )
                    }
                }
            }
        }
        PrRow(
            title = baseBranch,
            node = PrNodeState.Base,
            rail = PrRail(above = members.isNotEmpty()),
            testTag = "pr-row-base",
        )
    }
}

/** Every PR row's test tag. */
const val PR_ROW_TAG = "pr-row"

/** The base branch's muted ring (web `border-muted-foreground/60`). */
private const val PR_BASE_RING_ALPHA = 0.45f
