package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.Prompts
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.issue.AgentPrCard
import com.exponential.app.ui.issue.IssueDetailBottomBar
import com.exponential.app.ui.issue.IssueDetailViewModel
import com.exponential.app.ui.issue.IssueThreadsChips
import com.exponential.app.ui.issue.StartButtonUi
import com.exponential.app.ui.issue.StartCircle
import com.exponential.app.ui.session.SessionViewPreview
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.ChangesMergeControl
import com.exponential.app.ui.work.WorkTopBar

/** Which session-lane capture to draw (one shot each). */
enum class SessionCaptureKind(val testId: String) {
    /** sg_session-row: the raw session view (header + Stop over a transcript). */
    Session("session-capture-screen"),

    /** The issue's top with the ONE Start (the bar circle on phones). */
    IssueStart("issue-start-capture-screen"),
}

/**
 * EXP-1296: the session lane's capture-only screens — the REAL composables
 * ([SessionViewPreview] = the raw session view's header, status row,
 * transcript and composer; the issue's [WorkTopBar], [IssueThreadsChips],
 * [AgentPrCard] and [IssueDetailBottomBar] with the [StartCircle]) over the
 * drafts' data (4-session, 7-issue: Acme, APP-5 "Reduce cold start below
 * 800 ms"), built through the real domain model ([SessionTree]).
 */
@Composable
fun SessionCaptures(kind: SessionCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            when (kind) {
                SessionCaptureKind.Session -> SessionSpecimen()
                SessionCaptureKind.IssueStart -> IssueStartSpecimen()
            }
        }
    }
}

private const val TEAM = "t-acme"
private const val ROOT = "s-cold"
private const val CHILD = "s-profile"

private fun treeSession(
    id: String,
    parentId: String?,
    title: String?,
    status: String,
    busy: Boolean = false,
    needsInput: Boolean = false,
    createdAt: String,
) = SessionTree.Session(
    id = id,
    teamId = TEAM,
    parentId = parentId,
    rootId = ROOT,
    kind = DomainContract.sessionKindThread,
    userId = "u-alex",
    status = status,
    busy = busy,
    needsInput = needsInput,
    blocked = null,
    shared = true,
    title = title,
    guide = null,
    actionName = null,
    name = null,
    icon = null,
    color = null,
    sortOrder = 0.0,
    startedAt = createdAt,
    endedAt = null,
    createdAt = createdAt,
    updatedAt = "2026-10-13T09:40:00.000Z",
)

private val RootSession = treeSession(
    ROOT, null, null, DomainContract.sessionStatusRunning, busy = true,
    createdAt = "2026-10-13T09:00:00.000Z",
)
private val ProfileSession = treeSession(
    CHILD, ROOT, "Profile the launch path", DomainContract.sessionStatusRunning,
    needsInput = true, createdAt = "2026-10-13T09:05:00.000Z",
)
private val App5 = SessionTree.Issue("i-app5", "APP-5", "Reduce cold start below 800 ms")

private val ThreadTitle = SessionTree.threadTitle(RootSession, listOf(App5), emptyList()).title

@Composable
private fun SessionSpecimen() {
    val tree = SessionTree.treeOrder(listOf(RootSession, ProfileSession))
    val card = SessionTree.sessionCard(
        ProfileSession,
        ordinal = tree.indexOfFirst { it.id == CHILD } + 1,
        now = "2026-10-13T09:45:00.000Z",
    )
    SessionViewPreview(
        threadTitle = ThreadTitle,
        sessionName = card.name,
        agent = "claude",
        state = card.state,
        feed = SampleFeed,
    )
}

private val SampleFeed: List<AgentFeedItem> = listOf(
    AgentFeedItem.UserMessage(1, "Profile app launch end-to-end on the Galaxy A54 and find where the 1.6 s goes."),
    AgentFeedItem.Tool(
        2, "Bash", "./gradlew :app:startupProfile --variant=release",
        callId = "c1", toolKind = "execute", settled = true,
        output = "p50 740ms, p90 812ms",
    ),
    AgentFeedItem.Narration(
        3,
        "The cached snapshot needs to survive a cold process, so the board reducer now writes its last " +
            "state to the on-disk cache when the app backgrounds. Without it the first launch after a " +
            "reboot still paints empty.",
    ),
    AgentFeedItem.Tool(
        4, "Read", "data/sync/BoardSnapshotCache.kt",
        callId = "c2", toolKind = "read", settled = true,
    ),
    AgentFeedItem.Tool(
        5, "Edit", "app/src/main/java/com/exponential/app/data/sync/BoardSnapshotCache.kt",
        callId = "c3", toolKind = "edit", settled = true,
    ),
    AgentFeedItem.Narration(
        6,
        "One caller assumed the first emission was the full list and skipped its empty state. Guarded " +
            "it so the board never flashes \"No issues\" between the cached paint and the live snapshot.",
    ),
)

private val SamplePullRequest = PullRequestEntity(
    id = "pr-41",
    teamId = TEAM,
    rootSessionId = ROOT,
    repositoryId = "r-mobile",
    number = 41,
    url = "https://github.com/acme/mobile/pull/41",
    title = "Defer shape subscribe until after first frame",
    head = "exp/APP-5",
    base = "main",
    createdAt = "2026-10-13T09:30:00.000Z",
    updatedAt = "2026-10-13T09:30:00.000Z",
)

@Composable
private fun IssueStartSpecimen() {
    val thread = IssueDetailViewModel.IssueThread(
        rootId = ROOT,
        title = ThreadTitle,
        live = true,
        busy = true,
    )
    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            WorkTopBar(
                title = App5.identifier,
                onBack = {},
                verb = null,
                verbEnabled = false,
                onVerb = {},
                menu = null,
            )
        },
        bottomBar = {
            IssueDetailBottomBar(
                expanded = false,
                onExpandedChange = {},
                showProperties = true,
                onOpenProperties = {},
                trailing = { StartCircle(ui = StartButtonUi.Ready, onClick = {}) },
                draft = "",
                onDraftChange = {},
                sending = false,
                onSend = {},
                pendingAttachments = emptyList(),
                onAddAttachment = {},
                onRemoveAttachment = {},
                mentionMembers = emptyList(),
                modifier = Modifier.navigationBarsPadding(),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
            horizontalAlignment = Alignment.Start,
        ) {
            Text(App5.title, style = MaterialTheme.typography.headlineSmall)
            IssueThreadsChips(threads = listOf(thread), onOpen = {})
            Text(
                "Startup profiling shows most of the time goes into the sync bootstrap, not rendering. " +
                    "Target is <800ms cold on a mid-range device.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            )
            Box(Modifier.fillMaxWidth()) {
                AgentPrCard(
                    pullRequests = listOf(SamplePullRequest),
                    onOpenPullRequest = {},
                    mergeControlFor = { pr ->
                        ChangesMergeControl(
                            label = DomainContract.diffUiMergePr,
                            loading = false,
                            error = null,
                            confirmPrompt = Prompts.MergeIssuePr.prompt(pr.number),
                            onConfirm = {},
                        )
                    },
                )
            }
        }
    }
}
