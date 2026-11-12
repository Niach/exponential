package com.exponential.app.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.domain.ClosePr
import com.exponential.app.domain.MergeTrain
import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.NeedsYou
import com.exponential.app.domain.Prompts
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassSubmitButton
import com.exponential.app.ui.components.SubmitSize
import com.exponential.app.ui.components.PromptAlert
import com.exponential.app.ui.work.STACK_CONFIRM_CANCEL_LABEL
import com.exponential.app.ui.work.MergeNowConfirm
import com.exponential.app.ui.work.NeedsApprovalConfirm
import com.exponential.app.ui.components.ChatEntryRow
import com.exponential.app.ui.components.CountBadge
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LocalPageHeaderActions
import com.exponential.app.ui.components.PageHeader
import com.exponential.app.ui.components.PageHeaderActions
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.chats.ChatsViewModel

// #FEED-191 NEEDS YOU ×Android (fixtures/needs-you.json; web
// `packages/ui/src/needs-you.tsx`, iOS ExpUI `NeedsYou.swift`): the model is
// [NeedsYou.needsYou]; these only paint it.
//
//   NeedsYouEntryRow  the first row of Chats (the Archived row's box)
//   needsYouList      one reading column of THE inbox page's row look, else
//                     THE EmptyState
//   NeedsYouPage      the pushed page: the page header with back over the list

/** THE ENTRY on phones: as tall as the Archived row, the glyph in the needs-you
 *  tone centred in the lead disc's column (no disc), the title `copy.entry`,
 *  the count Badge in the needs-you tone trailing. */
@Composable
fun NeedsYouEntryRow(count: Int, onClick: (() -> Unit)?, modifier: Modifier = Modifier) {
    ChatEntryRow(
        glyph = ExpIcons.navNeedsYou,
        glyphTint = DesignTokens.Semantic.Yellow,
        title = NeedsYou.Copy.ENTRY,
        onClick = onClick,
        modifier = modifier,
        testTag = NeedsYou.TestIds.ENTRY,
    ) {
        CountBadge(count, NavBadges.Tone.Attention, testTag = "needs-you-count")
    }
}

private val NeedsYou.Row.glyph: ImageVector
    get() = when (kind) {
        NeedsYou.Kind.Plan -> ExpIcons.codingPlan
        NeedsYou.Kind.Question -> ExpIcons.uiHelp
        NeedsYou.Kind.Pr -> ExpIcons.prOpen
    }

/** One row (THE inbox page's flat row): the leading glyph in the needs-you tone,
 *  line 1 the title then the age muted trailing, line 2 the chat then ` · meta`,
 *  muted. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NeedsYouRowItem(
    row: NeedsYou.Row,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    /** #EXP-1710: a stuck PR row's action (Retry, Merge now, Close PR). */
    onAction: (NeedsYou.Row, MergeTrain.StuckAction) -> Unit = { _, _ -> },
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        modifier
            .fillMaxWidth()
            .flatRow()
            .clickable(onClick = onClick)
            .testTag(NeedsYou.TestIds.ROW)
            .then(if (row.actions.isNotEmpty()) Modifier.testTag(MergeTrain.TestIds.STUCK_ROW) else Modifier)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(
            row.glyph,
            contentDescription = null,
            tint = DesignTokens.Semantic.Yellow,
            modifier = Modifier.padding(top = 2.dp).size(16.dp),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    row.title,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(row.age, style = MaterialTheme.typography.bodySmall, color = muted, maxLines = 1)
            }
            Text(
                if (row.meta.isEmpty()) row.chat else "${row.chat} · ${row.meta}",
                style = MaterialTheme.typography.bodySmall,
                color = muted,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            // #EXP-1710 (merge-train.json NEEDS YOU, web
            // `needs-you-stuck-actions.tsx`): a stuck PR's actions, THE
            // Button sm outline, one wrapping row under the meta line.
            if (row.actions.isNotEmpty()) {
                FlowRow(
                    modifier = Modifier.padding(top = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    row.actions.forEach { action ->
                        GlassSubmitButton(
                            label = stuckActionLabel(action),
                            onClick = { onAction(row, action) },
                            size = SubmitSize.Small,
                            outline = true,
                            modifier = Modifier.testTag("${MergeTrain.TestIds.STUCK_ROW}-${action.wire}"),
                        )
                    }
                }
            }
        }
    }
}

/** #EXP-1710: an `approval`-mode ready PR row (a PR row with no stuck
 *  actions: `auto` mode lists none): its press is THE pill's Needs approval. */
internal val NeedsYou.Row.needsApproval: Boolean
    get() = kind == NeedsYou.Kind.Pr && actions.isEmpty()

private fun stuckActionLabel(action: MergeTrain.StuckAction): String = when (action) {
    MergeTrain.StuckAction.Retry -> MergeTrain.Copy.NeedsYou.RETRY
    MergeTrain.StuckAction.MergeNow -> MergeTrain.Copy.NeedsYou.MERGE_NOW
    MergeTrain.StuckAction.ClosePr -> MergeTrain.Copy.NeedsYou.CLOSE
}

/** THE list's items: one reading column; no row = THE EmptyState. */
fun LazyListScope.needsYouList(
    rows: List<NeedsYou.Row>,
    onOpen: (NeedsYou.Row) -> Unit,
    onAction: (NeedsYou.Row, MergeTrain.StuckAction) -> Unit = { _, _ -> },
) {
    if (rows.isEmpty()) {
        item(key = "__empty__") {
            Box(Modifier.fillMaxWidth().height(320.dp)) {
                EmptyState(title = NeedsYou.Copy.EMPTY, icon = ExpIcons.navNeedsYou)
            }
        }
    } else {
        items(rows, key = { "needs_you_${it.messageId}" }) { row ->
            NeedsYouRowItem(row, onClick = { onOpen(row) }, modifier = Modifier.padding(horizontal = 8.dp), onAction = onAction)
        }
    }
}

/** The page's stateless body (the `needs-you` walk and the live route). */
@Composable
fun NeedsYouPageContent(
    rows: List<NeedsYou.Row>,
    onBack: () -> Unit,
    onOpen: (NeedsYou.Row) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Int,
    modifier: Modifier = Modifier,
    onAction: (NeedsYou.Row, MergeTrain.StuckAction) -> Unit = { _, _ -> },
) {
    val shell = LocalPageHeaderActions.current
    LazyColumn(
        modifier = modifier.fillMaxSize().navigationBarsPadding().testTag(NeedsYou.TestIds.LIST),
        contentPadding = PaddingValues(top = 4.dp, bottom = 16.dp),
    ) {
        item(key = "__header__") {
            PageHeader(
                title = NeedsYou.Copy.ENTRY,
                onBack = onBack,
                actions = PageHeaderActions(
                    onSearch = { shell?.onSearch?.invoke() },
                    onInbox = onOpenInbox,
                    inboxUnread = inboxUnread,
                ),
            )
        }
        needsYouList(rows, onOpen, onAction)
    }
}

/** #FEED-191: the pushed page (needs-you.json THE LIST), the ACTIVE team's rows. */
@Composable
fun NeedsYouPage(
    onBack: () -> Unit,
    onOpen: (NeedsYou.Row) -> Unit,
    onOpenInbox: () -> Unit,
    inboxUnread: Int,
    viewModel: ChatsViewModel = hiltViewModel(),
) {
    val rows by viewModel.needsYouRows.collectAsStateWithLifecycle()
    val pullRequests by viewModel.needsYouPullRequests.collectAsStateWithLifecycle()
    // #EXP-1710: a stuck row's Merge now asks `merge-now-pr`, Close PR asks
    // close-pr.json's confirm (the chat's); Retry resumes at once.
    var mergeNowFor by remember { mutableStateOf<NeedsYou.Row?>(null) }
    var closeFor by remember { mutableStateOf<NeedsYou.Row?>(null) }
    // #EXP-1710 (needs-you.json): an `approval` ready PR row's press = THE
    // pill's Needs approval confirm (Merge queues it, Cancel leaves the row).
    // [approveFor] stays set after the confirm closes so a refused merge
    // still toasts; [approveOpen] is the confirm itself.
    var approveFor by remember { mutableStateOf<String?>(null) }
    var approveOpen by remember { mutableStateOf(false) }
    Scaffold(containerColor = Color.Transparent) { padding ->
        NeedsYouPageContent(
            rows = rows,
            onBack = onBack,
            onOpen = { row ->
                if (row.needsApproval && pullRequests.any { it.id == row.messageId }) {
                    approveFor = row.messageId
                    approveOpen = true
                } else {
                    onOpen(row)
                }
            },
            onOpenInbox = onOpenInbox,
            inboxUnread = inboxUnread,
            modifier = Modifier.padding(top = padding.calculateTopPadding()),
            onAction = { row, action ->
                when (action) {
                    MergeTrain.StuckAction.Retry -> viewModel.retryPullRequest(row.messageId)
                    MergeTrain.StuckAction.MergeNow -> mergeNowFor = row
                    MergeTrain.StuckAction.ClosePr -> closeFor = row
                }
            },
        )
    }
    approveFor?.let { id -> pullRequests.firstOrNull { it.id == id } }?.let { pr ->
        NeedsApprovalConfirm(
            newest = pr,
            stackPool = pullRequests.filter { it.teamId == pr.teamId },
            confirmOpen = approveOpen,
            onDismiss = { approveOpen = false },
        )
    }
    mergeNowFor?.let { row ->
        val pr = pullRequests.firstOrNull { it.id == row.messageId }
        if (pr != null) {
            MergeNowConfirm(
                pr = pr,
                stackPool = pullRequests.filter { it.teamId == pr.teamId },
                onDismiss = { mergeNowFor = null },
                onMerge = { viewModel.mergePullRequestNow(row.messageId) },
            )
        } else {
            PromptAlert(
                prompt = Prompts.MergeNowPr.prompt(row.pullRequestNumber?.toInt()),
                onDismiss = { mergeNowFor = null },
                handlers = mapOf(
                    "merge" to {
                        mergeNowFor = null
                        viewModel.mergePullRequestNow(row.messageId)
                    },
                ),
            )
        }
    }
    closeFor?.let { row ->
        val linked by remember(row.messageId) { viewModel.linkedIssueCount(row.messageId) }.collectAsStateWithLifecycle(0)
        GlassAlert(
            title = ClosePr.TITLE,
            body = ClosePr.body((linked - 1).coerceAtLeast(0)),
            onDismiss = { closeFor = null },
            trailing = listOf(
                GlassAlertAction(STACK_CONFIRM_CANCEL_LABEL, onClick = { closeFor = null }),
                GlassAlertAction(
                    ClosePr.CONFIRM,
                    destructive = true,
                    testTag = "close-pr-confirm",
                    onClick = {
                        closeFor = null
                        viewModel.closePullRequest(row.messageId)
                    },
                ),
            ),
            defaultAction = 0,
        )
    }
}
