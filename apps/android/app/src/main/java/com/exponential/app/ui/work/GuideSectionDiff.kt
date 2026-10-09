package com.exponential.app.ui.work

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.calculateEndPadding
import androidx.compose.foundation.layout.calculateStartPadding
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.GUIDE_FACE_LABEL
import com.exponential.app.domain.GuideSectionPage
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBarCluster
import com.exponential.app.ui.components.FloatingBarEdge
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DiffCounts
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.issue.DiffFileListSheet
import com.exponential.app.ui.issue.diffOpensByDefault
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.launch

// EXP-1251: a Guide SECTION PAGE (web `guide-section-diff.tsx`, phones) — a
// Changes row's tap opens its section's changes under the Guide: the back
// row `‹ Guide · 02 / 06 · title  +A −D` over one [DiffFileCard] per file the
// section covers ([com.exponential.app.domain.guideSectionPage], the same
// coverage the row counted), and the floating bar's `[files circle] [Merge]`
// whose sheet lists only those files. `Show complete diff` is the page of
// every file. System Back returns to the Guide.

@Composable
fun GuideSectionDiff(
    padding: PaddingValues,
    /** The section's page; null while the diff is not loaded (or a stale section). */
    page: GuideSectionPage?,
    /** The diff's load state — what captions a page with nothing to show. */
    load: ChangesLoadState?,
    merge: ChangesMergeControl?,
    onBack: () -> Unit,
) {
    BackHandler(onBack = onBack)
    val files = page?.files.orEmpty()
    var sheetOpen by remember { mutableStateOf(false) }
    val expanded = remember(files) { mutableStateMapOf<String, Boolean>() }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    LaunchedEffect(page?.section) { listState.scrollToItem(0) }

    val layoutDirection = LocalLayoutDirection.current
    val bottomInset = padding.calculateBottomPadding()
    Box(
        modifier = Modifier
            .padding(
                start = padding.calculateStartPadding(layoutDirection),
                end = padding.calculateEndPadding(layoutDirection),
            )
            .fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().detailHazeSource().testTag("guide-section"),
            state = listState,
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = padding.calculateTopPadding() + 4.dp,
                bottom = BottomBarInset + bottomInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            item(key = "__back__") { GuideBackRow(page = page, onBack = onBack) }
            if (files.isEmpty()) {
                item(key = "__state__") {
                    when (load) {
                        is ChangesLoadState.Loading -> ChangesLoadingRow()
                        is ChangesLoadState.Failed -> ChangesFailureRow(load.message)
                        else -> ChangesEmptyRow()
                    }
                }
            }
            items(files.size, key = { "section_file_$it" }) { index ->
                val file = files[index]
                // EXP-916: a card opens by default unless it is huge.
                val opens = diffOpensByDefault(file, defaultCollapsed = false)
                DiffFileCard(
                    file = file,
                    expanded = expanded[file.path] ?: opens,
                    onToggle = { expanded[file.path] = !(expanded[file.path] ?: opens) },
                )
            }
        }
        FloatingBarEdge(
            bottomInset = bottomInset,
            visible = files.isNotEmpty() || merge != null,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(bottom = bottomInset),
        ) {
            FloatingBarCluster(
                left = if (files.isNotEmpty()) {
                    { FileListCircle(count = files.size, onClick = { sheetOpen = true }) }
                } else {
                    null
                },
                centre = merge?.let { control -> { MergeCapsule(control, tag = "$MergeCapsuleTag-section") } },
            )
        }
    }

    // The file sheet, filtered to the section: a pick opens that card and
    // scrolls to it (+1 for the back row).
    if (sheetOpen) {
        DiffFileListSheet(
            files = files,
            onPick = { path ->
                sheetOpen = false
                expanded[path] = true
                val index = files.indexOfFirst { it.path == path }
                if (index >= 0) scope.launch { listState.animateScrollToItem(index + 1) }
            },
            onDismiss = { sheetOpen = false },
        )
    }
}

/** `‹ Guide │ 02 / 06 · title  +A −D` — the section page's first row. */
@Composable
private fun GuideBackRow(page: GuideSectionPage?, onBack: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = onBack)
            .padding(vertical = 6.dp)
            .testTag("guide-section-back"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(ExpIcons.uiChevronLeft, contentDescription = null, modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurface)
        Text(GUIDE_FACE_LABEL, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
        if (page != null) {
            val title = listOfNotNull(page.caption, page.title).joinToString(" · ")
            Text(
                title,
                style = MaterialTheme.typography.bodySmall.copy(fontFeatureSettings = "tnum"),
                color = muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f).testTag("guide-section-caption"),
            )
            DiffCounts(additions = page.additions, deletions = page.deletions)
        } else {
            Spacer(Modifier.weight(1f))
        }
    }
}

/**
 * The bar's LEADING circle (EXP-895): the files glyph over the file count,
 * opening the `Changed files` sheet. The count is the affordance — a reader
 * sees how much is in the review before opening anything.
 */
@Composable
internal fun FileListCircle(count: Int, onClick: () -> Unit) {
    BarCircle(onClick = onClick, modifier = Modifier.testTag("changes-file-list-button")) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Icon(
                ExpIcons.navFiles,
                contentDescription = DomainContract.diffUiChangedFilesTitle,
                modifier = Modifier.size(18.dp),
                tint = Color.White,
            )
            Text(
                count.toString(),
                style = MaterialTheme.typography.labelSmall,
                color = Color.White.copy(alpha = TextEmphasis.Secondary),
            )
        }
    }
}

@Composable
internal fun ChangesLoadingRow() {
    Row(
        modifier = Modifier.padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
        Text(
            "Loading changes…",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

@Composable
internal fun ChangesFailureRow(message: String) {
    Text(
        "Couldn’t load changes: $message",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(vertical = 12.dp),
    )
}

@Composable
internal fun ChangesEmptyRow() {
    Text(
        "No changed files.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(vertical = 12.dp),
    )
}
