package com.exponential.app.ui.thread

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.data.db.PullRequestIssueEntity
import com.exponential.app.domain.Diff
import com.exponential.app.domain.EmptyStateCopy
import com.exponential.app.domain.GuideChangeSet
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.Stories
import com.exponential.app.domain.guideFileCountLabel
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.StackRail
import com.exponential.app.ui.components.StackRailMember
import com.exponential.app.ui.components.detailHazeSource
import com.exponential.app.ui.components.failedAction
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.issue.DiffCounts
import com.exponential.app.ui.issue.DiffFileCard
import com.exponential.app.ui.issue.DiffFileListSheet
import com.exponential.app.ui.issue.diffOpensByDefault
import com.exponential.app.ui.issue.toDiffFile
import com.exponential.app.ui.stories.StoriesViewModel
import com.exponential.app.ui.stories.StoryTab
import com.exponential.app.ui.stories.StoryTabState
import com.exponential.app.ui.stories.StoryViewer
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.work.ChangesLoadState
import com.exponential.app.ui.work.ChangesViewModel
import com.exponential.app.ui.work.PullRequestStack
import com.exponential.app.ui.work.StackMergeDialog
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch

/**
 * stories.json STORIES TAB ×Android (web `thread-stories.tsx`): the thread's
 * last page. ONE scroll: the PR block while the thread has its ONE pull
 * request ([SessionTree.guidePullRequest]) — in a linear open stack the
 * `Stack` band over THE [StackRail] (a tap opens that PR's thread, a
 * long-press merges through it), then ONE `Changes` row opening THE review
 * page (the complete diff, back row `‹ Stories`, the files sheet) — then
 * `leadGap` and THE tiles ([StoryTab]). A tile / thumbnail opens THE
 * [StoryViewer] on the tab's ring at that story (and picture).
 */
@Composable
fun ThreadStoriesPage(
    tab: StoryTabState,
    storiesViewModel: StoriesViewModel,
    pullRequests: List<PullRequestEntity>,
    teamPullRequests: List<PullRequestEntity>,
    pullRequestIssues: List<PullRequestIssueEntity>,
    issues: List<IssueEntity>,
    padding: PaddingValues,
    onOpenThread: (rootSessionId: String) -> Unit,
    onOpenSession: (Stories.Story) -> Unit,
) {
    val pr = SessionTree.guidePullRequest(pullRequests)
    val stackPool = remember(pullRequests, teamPullRequests) { (teamPullRequests + pullRequests).distinctBy { it.id } }
    val stackView = remember(pr, stackPool, pullRequestIssues, issues) {
        pr?.let { PullRequestStack.stackView(it, stackPool, pullRequestIssues, issues) }
    }
    val filesVm: ChangesViewModel? = pr?.let { row ->
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${row.id}") { it.create(row.id) }
    }
    val load by (filesVm?.load ?: remember { MutableStateFlow<ChangesLoadState?>(null) }).collectAsStateWithLifecycle()
    val files = remember(load) { (load as? ChangesLoadState.Loaded)?.files?.map { it.toDiffFile() } }
    val changes = remember(files) {
        files?.let { list -> GuideChangeSet(list, list.sumOf { it.additions }, list.sumOf { it.deletions }) }
    }
    var review by rememberSaveable { mutableStateOf(false) }
    var open by remember { mutableStateOf<Pair<String, Int>?>(null) }

    if (review && files != null) {
        StoriesReview(files = files, padding = padding, onBack = { review = false })
        return
    }

    val lead: (@Composable () -> Unit)? = if (pr != null) {
        {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                stackView?.let { view ->
                    StoriesStackCard(view, pr, stackPool, onOpenThread)
                }
                if (changes != null && changes.fileCount > 0) {
                    StoriesChangesRow(changes, onOpen = { review = true })
                }
            }
        }
    } else {
        null
    }

    Column(
        Modifier
            .fillMaxSize()
            .detailHazeSource()
            .verticalScroll(rememberScrollState())
            .padding(padding)
            .padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 32.dp),
    ) {
        if (tab.loading && tab.ring == null) {
            SkeletonRows()
        } else {
            StoryTab(ring = tab.ring, lead = lead, onOpen = { story, picture -> open = story.id to picture })
        }
    }

    val failure by storiesViewModel.failure.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = storiesViewModel::clearFailure)
    val ring = tab.ring
    val target = open
    if (ring != null && target != null) {
        Dialog(
            onDismissRequest = { open = null },
            properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
        ) {
            StoryViewer(
                rings = listOf(ring),
                startKey = ring.key,
                startStoryId = target.first,
                startPicture = target.second,
                rows = tab.draw.rows,
                avatar = tab.draw::avatar,
                onAnswer = storiesViewModel::answer,
                onDecidePlan = storiesViewModel::decidePlan,
                onSeen = storiesViewModel::markSeen,
                onOpenSession = { story ->
                    open = null
                    onOpenSession(story)
                },
                onClose = { open = null },
                modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

/** The `Stack` band (the `pr-stack` glyph) over THE rail, top first. */
@Composable
private fun StoriesStackCard(
    view: PullRequestStack.StackView,
    pr: PullRequestEntity,
    pool: List<PullRequestEntity>,
    onOpenThread: (String) -> Unit,
) {
    class Member(val row: PullRequestStack.StackViewRow) :
        StackRailMember(key = row.pullRequestId, identifier = row.identifier, title = row.title, current = row.isCurrent)
    val members = remember(view) { view.rows.map(::Member) }
    // The long-press: Merge through that member, its ONE confirm first; the
    // member stays picked so its merge VM (and refusal toast) outlives it.
    var throughId by remember { mutableStateOf<String?>(null) }
    var confirmOpen by remember { mutableStateOf(false) }
    val throughPr = throughId?.let { id -> pool.firstOrNull { it.id == id } }
    val throughVm: ChangesViewModel? = throughPr?.let { row ->
        hiltViewModel<ChangesViewModel, ChangesViewModel.Factory>(key = "changes:${row.id}") { it.create(row.id) }
    }
    val throughError by (throughVm?.actionError ?: remember { MutableStateFlow<String?>(null) }).collectAsStateWithLifecycle()
    val toaster = LocalToaster.current
    androidx.compose.runtime.LaunchedEffect(throughError) {
        val message = throughError ?: return@LaunchedEffect
        val vm = throughVm ?: return@LaunchedEffect
        toaster.failedAction("merge", "the stack", message, retry = vm::retryMerge)
    }
    Column(Modifier.testTag(Stories.Tab.TestIds.STACK)) {
        SectionHeader(
            title = Stories.Tab.Copy.STACK,
            leading = {
                Icon(
                    ExpIcons.prStack,
                    contentDescription = null,
                    modifier = Modifier.size(14.dp),
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                )
            },
        )
        StackRail(
            members = members,
            baseBranch = view.baseBranch,
            onOpen = { member -> if (!member.current) member.row.rootSessionId?.let(onOpenThread) },
            onMergeThrough = { member ->
                throughId = member.row.pullRequestId
                confirmOpen = true
            },
        )
    }
    if (confirmOpen && throughPr != null && throughVm != null) {
        PullRequestStack.stackConfirm(pr, pool, through = throughPr)?.let { confirm ->
            StackMergeDialog(
                confirm = confirm,
                onConfirm = { throughVm.mergePr(stack = true) },
                onDismiss = { confirmOpen = false },
            )
        }
    }
}

/** THE `Changes` row: the diff glyph, the label, `n files`, `+a −d`, the
 *  chevron; a hairline under it. */
@Composable
private fun StoriesChangesRow(changes: GuideChangeSet, onOpen: () -> Unit) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(Modifier.fillMaxWidth().testTag(Stories.Tab.TestIds.CHANGES)) {
        Row(
            Modifier
                .fillMaxWidth()
                .flatRow()
                .clickable(onClick = onOpen)
                .padding(horizontal = 4.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Icon(ExpIcons.guideChanges, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
            Text(
                Stories.Tab.Copy.CHANGES,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                modifier = Modifier.weight(1f),
            )
            Text(
                guideFileCountLabel(changes.fileCount),
                style = MaterialTheme.typography.labelSmall.copy(fontFeatureSettings = "tnum"),
                color = muted,
                maxLines = 1,
            )
            DiffCounts(additions = changes.additions, deletions = changes.deletions)
            Icon(ExpIcons.uiChevronRight, contentDescription = null, modifier = Modifier.size(16.dp), tint = muted)
        }
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
    }
}

/** THE review page: `‹ Stories │ Changes  +a −d  [files]` over one
 *  [DiffFileCard] per file; the files button opens THE files sheet. System
 *  Back returns to the tab. */
@Composable
private fun StoriesReview(files: List<Diff.File>, padding: PaddingValues, onBack: () -> Unit) {
    BackHandler(onBack = onBack)
    val expanded = remember(files) { mutableStateMapOf<String, Boolean>() }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    var sheetOpen by remember { mutableStateOf(false) }
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    LazyColumn(
        modifier = Modifier.fillMaxSize().detailHazeSource().testTag("thread-stories-review"),
        state = listState,
        contentPadding = PaddingValues(
            start = 16.dp,
            end = 16.dp,
            top = padding.calculateTopPadding() + 8.dp,
            bottom = padding.calculateBottomPadding() + 32.dp,
        ),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item(key = "__back__") {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Row(
                    Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .clickable(onClick = onBack)
                        .padding(vertical = 6.dp, horizontal = 2.dp)
                        .testTag("thread-stories-review-back"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Icon(ExpIcons.uiChevronLeft, contentDescription = null, modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurface)
                    Text(Stories.Tab.Copy.LABEL, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurface)
                }
                Box(Modifier.size(width = GlassTokens.Hairline, height = 16.dp).background(GlassTokens.StrokeCard))
                Text(
                    Stories.Tab.Copy.CHANGES,
                    style = MaterialTheme.typography.bodyMedium,
                    color = muted,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                DiffCounts(additions = files.sumOf { it.additions }, deletions = files.sumOf { it.deletions })
                if (files.size > 1) {
                    Icon(
                        ExpIcons.navFiles,
                        contentDescription = com.exponential.app.domain.DomainContract.diffUiChangedFilesTitle,
                        tint = muted,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { sheetOpen = true }
                            .padding(6.dp)
                            .size(18.dp)
                            .testTag("changes-file-list-button"),
                    )
                }
            }
        }
        if (files.isEmpty()) {
            item(key = "__empty__") { EmptyState(EmptyStateCopy.changedFiles) }
        }
        items(files.size, key = { "review_file_$it" }) { index ->
            val file = files[index]
            val opens = diffOpensByDefault(file, defaultCollapsed = false)
            DiffFileCard(
                file = file,
                expanded = expanded[file.path] ?: opens,
                onToggle = { expanded[file.path] = !(expanded[file.path] ?: opens) },
            )
        }
    }
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
