package com.exponential.app.ui.issue

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.Diff
import com.exponential.app.domain.DiffTree
import com.exponential.app.domain.DomainContract
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassSectionBand

// EXP-916 — the phone's file COLUMN, as the TREE every other client lists
// beside its cards (web `FileDiffTree`, desktop `domain::diff_tree`, iOS
// `DiffTree.swift`). The shape comes from `DiffTree.diffFileTree` and nothing
// else: directories before files, a lone child chain compacted into one node,
// subtree counts on every folder. A non-blank filter is a SEARCH — the tree
// gives way to the flat list of matching paths, in input order.

private val TreeFontSize = 12.sp

/** One rendered line of the tree: the node and how deep it sits. */
private data class TreeRow(val node: DiffTree.Node, val depth: Int)

private fun flatten(
    nodes: List<DiffTree.Node>,
    closed: Set<String>,
    depth: Int,
    out: MutableList<TreeRow>,
) {
    for (node in nodes) {
        out.add(TreeRow(node, depth))
        if (node.kind == DiffTree.Kind.DIR && node.path !in closed) {
            flatten(node.children, closed, depth + 1, out)
        }
    }
}

/**
 * The changed-file tree: the review's summary line, the filter field, then the
 * rows. Every folder starts OPEN — a reader who opened the list wants to see
 * what is in it — and a tap folds one away. Picking a file is the caller's
 * business ([onPick] closes the sheet and scrolls the page to that card).
 */
@Composable
fun DiffFileTree(
    files: List<Diff.File>,
    onPick: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var filter by remember { mutableStateOf("") }
    // The header counts the WHOLE diff, never the filtered slice: it is the
    // review's summary line, not a search result count.
    val summary = remember(files) {
        val totals = Diff.totals(files)
        Diff.summaryLabel(totals.files, totals.additions, totals.deletions)
    }
    val closed = remember(files) { mutableStateMapOf<String, Boolean>() }
    val nodes = remember(files, filter) { DiffTree.diffFileTree(files, filter) }
    val rows = remember(nodes, closed.toMap()) {
        val out = mutableListOf<TreeRow>()
        flatten(nodes, closed.filterValues { it }.keys, 0, out)
        out
    }
    Column(modifier = modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp)
                .glassSectionBand()
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(
                summary,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
        }
        Spacer(Modifier.height(8.dp))
        GlassSheetSearchField(
            value = filter,
            onValueChange = { filter = it },
            placeholder = DomainContract.diffUiFilterPlaceholder,
            modifier = Modifier.testTag("diff-nav-filter"),
        )
        Spacer(Modifier.height(4.dp))
        LazyColumn(modifier = Modifier.fillMaxWidth()) {
            items(rows.size, key = { "diff_tree_${rows[it].node.kind.wire}_${rows[it].node.path}" }) { index ->
                val row = rows[index]
                when (row.node.kind) {
                    DiffTree.Kind.DIR -> DiffTreeFolderRow(
                        node = row.node,
                        depth = row.depth,
                        open = closed[row.node.path] != true,
                        onToggle = { closed[row.node.path] = closed[row.node.path] != true },
                    )
                    DiffTree.Kind.FILE -> DiffTreeFileRow(
                        node = row.node,
                        depth = row.depth,
                        file = files.getOrNull(row.node.index),
                        onClick = { onPick(row.node.path) },
                    )
                }
            }
        }
    }
}

/** Indent per level — the tree's only structural chrome. */
private val TreeIndent = 12.dp

@Composable
private fun DiffTreeFolderRow(
    node: DiffTree.Node,
    depth: Int,
    open: Boolean,
    onToggle: () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("changes-file-tree-dir")
            .clickable(onClick = onToggle)
            .padding(start = 16.dp + TreeIndent * depth, end = 16.dp)
            .padding(vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            if (open) ExpIcons.uiFolderOpen else ExpIcons.uiFolder,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = muted,
        )
        Spacer(Modifier.width(8.dp))
        Text(
            node.name,
            fontFamily = FontFamily.Monospace,
            fontSize = TreeFontSize,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Spacer(Modifier.width(8.dp))
        DiffCounts(node.additions, node.deletions)
        Spacer(Modifier.width(6.dp))
        val rotation by animateFloatAsState(
            targetValue = if (open) 180f else 0f,
            animationSpec = Motion.standard(),
            label = "tree-chevron",
        )
        Icon(
            ExpIcons.uiChevronDown,
            contentDescription = if (open) "Collapse" else "Expand",
            modifier = Modifier.size(14.dp).rotate(rotation),
            tint = muted,
        )
    }
}

@Composable
private fun DiffTreeFileRow(
    node: DiffTree.Node,
    depth: Int,
    file: Diff.File?,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("changes-file-list-row")
            .clickable(onClick = onClick)
            .padding(start = 16.dp + TreeIndent * depth, end = 16.dp)
            .padding(vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DiffStatusLetter(file?.status ?: Diff.Status.MODIFIED)
        Text(
            node.name,
            fontFamily = FontFamily.Monospace,
            fontSize = TreeFontSize,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        DiffCounts(node.additions, node.deletions)
    }
}
