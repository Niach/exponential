package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ScriptCall
import com.exponential.app.domain.ScriptCardInput
import com.exponential.app.domain.ScriptCards
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WorkflowAgent
import com.exponential.app.domain.WorkflowPhase
import com.exponential.app.domain.WorkflowState
import com.exponential.app.domain.WORKFLOW_STATUS_COMPLETED
import com.exponential.app.domain.foldedPhases
import com.exponential.app.domain.workCard
import com.exponential.app.ui.components.AgentRunMark
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.modelLabel
import com.exponential.app.ui.session.CommentCallCard
import com.exponential.app.ui.session.ComposerRepliesRow
import com.exponential.app.ui.session.ScriptCardView
import com.exponential.app.ui.session.WorkCardView
import com.exponential.app.ui.thread.SessionCard
import com.exponential.app.ui.components.SessionRow
import com.exponential.app.ui.components.SessionXref
import com.exponential.app.ui.components.PrList
import com.exponential.app.ui.components.PrListRow
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.StackRail
import com.exponential.app.ui.components.StackRailMember
import com.exponential.app.ui.icons.ExpIcons
import androidx.compose.material3.Icon
import com.exponential.app.ui.thread.ThreadSuggestionActions
import com.exponential.app.ui.thread.ThreadSuggestionsCard
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis

/** Which thread card a capture shows (one shot each). */
enum class ThreadCardsKind(val testId: String) {
    Work("work-card-screen"),
    Script("script-card-screen"),
    Comment("comment-call-card-screen"),
    /** EXP-1323: the suggested tasks card in every state, a report line and
     *  a session card with its live caption; EXP-1211: the quick replies row. */
    Suggestions("thread-suggestions-screen"),
    /** EXP-1280: THE rich plan / question card (plan-card.json, shot sg_plan-card). */
    PlanCard("plan-card-screen"),
    /** S-3: THE inter-session link line in its four kinds (shot sg_session-xref). */
    SessionXref("session-xref-screen"),
    /** EXP-1553: THE PR row (web's `pr-row` entry, shot sg_pr-row). */
    PrRow("pr-row-specimen"),
    /** Thread 20b7fdc2: THE session row (web's `session-row` entry, shot sg_session-row). */
    SessionRow("session-row-specimen"),
    /** #EXP-1611: the header Merge pill in its four check states + the open
     *  Checks submenu over pr-checks.json `specimen` (shot sg_pr-checks). */
    PrChecks("pr-checks-specimen"),
}

/**
 * EXP-1240 / EXP-1279 / EXP-1241 (EXP-1259 D12-D14): the styleguide's run
 * thread cards on Android, a capture-only screen (sg_work-card,
 * sg_script-card, sg_comment-call-card) drawing each card with its REAL
 * composable over the seeded demo feed's data the web + desktop pictures
 * show (EXP-1553): the finished `cold-start-review` workflow, the 13-call
 * script with its one failure, the seeded agent comment; no captions.
 */
@Composable
fun ThreadCardsScreen(kind: ThreadCardsKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    // The PR list's bands run edge to edge, like the list's.
                    .padding(horizontal = if (kind == ThreadCardsKind.PrRow || kind == ThreadCardsKind.SessionRow) 0.dp else 16.dp, vertical = 72.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (kind) {
                    ThreadCardsKind.Work -> WorkCardSpecimens()
                    ThreadCardsKind.Script -> ScriptCardSpecimens()
                    ThreadCardsKind.Comment -> CommentCardSpecimens()
                    ThreadCardsKind.Suggestions -> SuggestionsSpecimens()
                    ThreadCardsKind.PlanCard -> PlanCardSpecimens()
                    ThreadCardsKind.SessionXref -> SessionXrefSpecimens()
                    ThreadCardsKind.PrRow -> PrRowSpecimens()
                    ThreadCardsKind.SessionRow -> SessionRowSpecimens()
                    ThreadCardsKind.PrChecks -> PrChecksSpecimens()
                }
            }
        }
    }
}

@Composable
private fun Caption(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}

/** S-3 (styleguide entry `session-xref`), EXP-1553: web's
 *  `SessionXrefSpecimen` row for row, four compact rows, no captions: the
 *  agent's conversation (its mark) and plain sessions (the session glyph). */
@Composable
private fun SessionXrefSpecimens() {
    val agent = com.exponential.app.data.db.AgentDefinitionEntity(
        id = "sg-agent",
        teamId = "t-team-1",
        name = "Triage",
        icon = "bot",
        color = "#8b5cf6",
    )
    androidx.compose.runtime.CompositionLocalProvider(
        com.exponential.app.ui.components.LocalAgentMarks provides mapOf(agent.id to agent),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            SessionXref(SessionTree.SessionXref("message", "sg-xref-message", "Triage", "sg-agent"), onOpen = {})
            SessionXref(SessionTree.SessionXref("report", "sg-xref-report", "Fix the sign-in redirect", null), onOpen = {})
            SessionXref(SessionTree.SessionXref("started", "sg-xref-started", "Triage", "sg-agent"), onOpen = {})
            SessionXref(SessionTree.SessionXref("reported", "sg-xref-reported", "Ship the login flow", null), onOpen = {})
        }
    }
}

/** #EXP-1611 (styleguide entry `pr-checks`), #EXP-1710: THE header merge
 *  STATUS pill (merge-train.json `statusPill`) in its states — Checks
 *  running (the spinner), Queued · 2nd, Testing with 3, Merged, Conflict and
 *  Checks failing (the red glyph), Auto-merge stuck, and Needs approval (the
 *  ONE primary action) — then THE header `…`'s Checks submenu pushed open as
 *  a static menu panel over pr-checks.json `specimen`. */
@Composable
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
private fun PrChecksSpecimens() {
    fun pill(state: String, wait: String? = null, position: Int? = null, batchSize: Int? = null, holdReason: String? = null) =
        com.exponential.app.domain.MergeTrain.statusPill(
            com.exponential.app.domain.MergeTrain.StatusPillInput(
                state = state,
                wait = wait,
                position = position,
                batchSize = batchSize,
                holdReason = holdReason,
                stack = false,
                checks = 5,
            ),
        )
    val pills = listOf(
        pill("checks", wait = "checks"),
        pill("queued", position = 2),
        pill("testing", batchSize = 4),
        pill("merged"),
        pill("conflict"),
        pill("failing", wait = "failing"),
        pill("held", holdReason = "bounces"),
        pill("checks", wait = "approval"),
    )
    // Web's specimen: the pills on ONE wrapping row.
    androidx.compose.foundation.layout.FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        pills.forEach { view -> com.exponential.app.ui.work.MergeStatusPill(view = view, onClick = {}) }
    }
    val now = com.exponential.app.domain.WireTimestamps.parseEpochMs(com.exponential.app.domain.PrChecks.SPECIMEN_NOW) ?: 0L
    val entry = com.exponential.app.ui.thread.prChecksMenuItem(com.exponential.app.domain.PrChecks.SPECIMEN, now) {} ?: return
    // The `…` menu's Checks entry (`Checks · 1 failing`), then its pushed page.
    com.exponential.app.ui.components.GlassMenuSurface(modifier = Modifier.width(280.dp)) {
        Column(modifier = Modifier.padding(vertical = 8.dp)) {
            com.exponential.app.ui.thread.ChatMenuPanel(
                groups = listOf(listOf(entry)),
                page = null,
                onPush = {},
                onBack = {},
                onPick = {},
            )
        }
    }
    com.exponential.app.ui.components.GlassMenuSurface(modifier = Modifier.width(280.dp)) {
        Column(modifier = Modifier.padding(vertical = 8.dp)) {
            com.exponential.app.ui.thread.ChatMenuPanel(
                groups = listOf(listOf(entry)),
                page = entry,
                onPush = {},
                onBack = {},
                onPick = {},
            )
        }
    }
}

/** EXP-1553 `pr-row` (web's styleguide entry, iOS's PrRowSpecimens line for
 *  line): THE PrRow in its two shapes — a PR TREE nested with tree guides, a
 *  linear STACK on its rail down to `master` (the current member washed,
 *  `stack` on the top row) — and an unlinked PR with the external-link glyph. */
@Composable
private fun PrRowSpecimens() {
    val stack = listOf(
        StackRailMember("100", "VAPP-100", "SwiftUI parity with the round-1/2 contract", current = true),
        StackRailMember("98", "VAPP-98", "Exponential UI renderer hardening round 1"),
        StackRailMember("93", "VAPP-93", "ui.exponential.at: the Exponential UI site"),
        StackRailMember("91", "VAPP-91", "Exponential UI host API ×4"),
    )
    Column {
        SectionHeader("Exponential")
        PrList(
            listOf(
                PrListRow("1250", "opening tabs closes all at some point", "EXP-1250", onOpen = {}),
                PrListRow("1252", "tabs per team on desktop", "EXP-1252", depth = 1, onOpen = {}),
                PrListRow("1253", "inbox stepping reuses one tab", "EXP-1253", depth = 1, onOpen = {}),
            ),
        )
        StackRail(members = stack, baseBranch = "master", word = "stack", onOpen = {}, onMergeThrough = {})
    }
    Column {
        SectionHeader("Not linked to an issue")
        PrList(
            listOf(
                PrListRow(
                    "1013",
                    "Dockerfiles: stub the exponential-ui workspace packages",
                    "#1013",
                    onOpen = {},
                    trailing = {
                        Icon(
                            ExpIcons.uiExternalLink,
                            contentDescription = null,
                            modifier = Modifier.size(14.dp),
                            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    },
                ),
            ),
        )
    }
}

/** Thread 20b7fdc2 `session-row` (web's styleguide entry, iOS's
 *  SessionRowSpecimens, desktop `session_row.rs`, row for row): THE
 *  SessionRow under the Running caption — the APP-5 tree (needs you, a
 *  working child, an idle grandchild), APP-14 working, an ended root; bold =
 *  unread, the first row lit. The lead = the session's run mark. */
@Composable
private fun SessionRowSpecimens() {
    data class Spec(val identifier: String?, val title: String, val depth: Int, val state: SessionTree.SessionState, val unread: Boolean = false)
    val rows = listOf(
        Spec("APP-5", "Reduce cold start below 800 ms", 0, SessionTree.SessionState.NeedsYou, unread = true),
        Spec(null, "Profile the launch path", 1, SessionTree.SessionState.Working),
        Spec(null, "Trace the splash screen", 2, SessionTree.SessionState.Idle, unread = true),
        Spec("APP-14", "Group board issues by status", 0, SessionTree.SessionState.Working),
        Spec(null, "Pre-cleanup release", 0, SessionTree.SessionState.Done),
    )
    val guides = com.exponential.app.domain.TreeGuides.compute(rows.map { it.depth })
    Column {
        SectionHeader("Running")
        rows.forEachIndexed { index, row ->
            SessionRow(
                title = row.title,
                identifier = row.identifier,
                depth = row.depth,
                guide = guides.getOrNull(index),
                active = index == 0,
                unread = row.unread,
            ) {
                AgentRunMark(agent = "claude", state = row.state, size = 14.dp, badgeSize = 6.dp)
            }
        }
    }
}

/** EXP-1553: the seeded demo feed's workflow (apps/web/scripts/
 *  screenshot-desktop.ts `wf-launch-review`), the one finished Work card the
 *  web + desktop pictures show, its done phases folded. */
private val DemoWorkflow = WorkflowState(
    id = "wf-launch-review",
    name = "cold-start-review",
    description = "Review the cold-start diff",
    status = WORKFLOW_STATUS_COMPLETED,
    phases = listOf(WorkflowPhase(1, "Review"), WorkflowPhase(2, "Verify")),
    agents = listOf(
        WorkflowAgent(1, "review:correctness", phaseIndex = 1, model = "opus", state = "done", durationMs = 41_000, toolCalls = 9),
        WorkflowAgent(2, "review:performance", phaseIndex = 1, model = "opus", state = "done", durationMs = 52_000, toolCalls = 12),
        WorkflowAgent(3, "verify:benchmarks", phaseIndex = 2, model = "sonnet", state = "done", durationMs = 64_000, toolCalls = 7),
    ),
    summary = "No blocking findings; the A54 p90 sits 12 ms over target",
)

@Composable
private fun WorkCardSpecimens() {
    val card = DemoWorkflow.workCard()
    WorkCardView(card = card, foldedPhases = DemoWorkflow.foldedPhases(card), onOpenLane = {})
}

/** EXP-1553: the demo feed's code-mode `exec` (13 calls, one failure). */
private val DemoIds = listOf("APP-5", "APP-9", "APP-10", "APP-14", "APP-16", "APP-17")

@Composable
private fun ScriptCardSpecimens() {
    val calls = DemoIds.mapIndexed { i, id -> ScriptCall("exponential.exponential_issues_get", id, ok = true, ms = 180L + i * 20) } +
        DemoIds.mapIndexed { i, id -> ScriptCall("exponential.exponential_comments_list", id, ok = true, ms = 140L + i * 15) } +
        ScriptCall("exponential.exponential_issues_pr_files", "APP-9", ok = false, ms = 95)
    ScriptCardView(
        card = ScriptCards.of(ScriptCardInput(running = false, count = 13, calls = calls, elapsedMs = 2_140)),
        source = DEMO_SCRIPT_SOURCE,
    )
}

private const val DEMO_SCRIPT_SOURCE = """const ids = ["APP-5", "APP-9", "APP-10", "APP-14", "APP-16", "APP-17"]
const issues = await Promise.all(
  ids.map((id) => tools.exponential.exponential_issues_get({ id }))
)
const threads = await Promise.all(
  ids.map((issueId) => tools.exponential.exponential_comments_list({ issueId }))
)
const files = await tools.exponential.exponential_issues_pr_files({ issueId: "APP-9" })
return issues.map((issue, i) => `${'$'}{issue.identifier} ${'$'}{threads[i].length} comments`)"""

private val Backlog = IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.Backlog }

/** EXP-1553: the seeded agent comment (seed-screenshots.ts
 *  DEMO_AGENT_COMMENT_ID) on APP-10, posted 20 minutes ago over MCP. */
@Composable
private fun CommentCardSpecimens() {
    CommentCallCard(
        issue = {
            IssueChip(
                identifier = "APP-10",
                title = "Migrate image cache to on-disk LRU",
                status = Backlog,
            )
        },
        author = "Alex Carter",
        caption = "via MCP · 20 minutes ago",
        mark = { AgentRunMark(agent = "claude", state = null, size = 14.dp, badgeSize = 0.dp, modifier = Modifier.size(14.dp)) },
        body = "From APP-5: the board snapshot cache now persists on `ON_STOP` (`BoardSnapshotCache.persistOnStop`), " +
            "so the LRU can reuse its directory and eviction hook instead of opening a second one.\n\n" +
            "Cold start after the change: **740 ms** p50 on the Pixel 6a.",
        onOpen = {},
    )
}

private fun specimenCard(id: String, name: String, state: SessionTree.SessionState, elapsed: String?) =
    SessionTree.SessionCard(
        sessionId = id,
        icon = SessionTree.CardCopy.ICONS.getValue(state),
        name = name,
        kindWord = SessionTree.CardCopy.STATES.getValue(state),
        state = state,
        tone = SessionTree.CardCopy.TONES.getValue(state),
        elapsed = elapsed,
        running = state == SessionTree.SessionState.Working,
        testId = SessionTree.CardCopy.TEST_ID_CARD,
    )

/** EXP-1323: the `session-thread.json` live dispatcher case, drawn by the
 *  real thread composables (read-only actions: nothing is sent). */
@Composable
private fun SuggestionsSpecimens() {
    val actions = ThreadSuggestionActions(
        onStart = { _, _, _, _ -> },
        onStartAll = {},
        onHere = { _, _ -> },
        onDismiss = { _, _ -> },
        onFile = { _, _ -> },
        models = DomainContract.codingModelValues.map { it to modelLabel(it) },
    )
    Caption("Session")
    SessionCard(
        card = specimenCard("r", "Speed up the shop", SessionTree.SessionState.Working, "9m 00s"),
        onClick = {},
    )
    Caption("Suggested tasks")
    ThreadSuggestionsCard(
        row = SessionTree.ThreadRow.Suggestions(
            id = "m2",
            sessionId = "r",
            header = "5 suggested tasks · 2 started",
            startAll = false,
            items = listOf(
                SessionTree.SuggestionRow(
                    id = "s1",
                    title = "Add width and height to cart thumbnails",
                    text = "Avoid layout shift in the cart.",
                    state = SessionTree.SuggestionState.Started,
                    card = specimenCard(
                        "c1", "Add width and height to cart thumbnails", SessionTree.SessionState.Working, "6m 00s",
                    ),
                    issueId = null,
                    identifier = null,
                ),
                SessionTree.SuggestionRow(
                    id = "s2",
                    title = "Lazy-load product images",
                    text = "Below-the-fold images load late.",
                    state = SessionTree.SuggestionState.Started,
                    card = specimenCard("c2", "Lazy-load product images", SessionTree.SessionState.Done, "3m 00s"),
                    issueId = null,
                    identifier = null,
                ),
                SessionTree.SuggestionRow(
                    "s3", "Cache headers", "Long-cache the static assets.",
                    SessionTree.SuggestionState.Filed, null, "i-9", "EXP-9",
                ),
                SessionTree.SuggestionRow(
                    "s4", "Measure LCP", "Run Lighthouse here.",
                    SessionTree.SuggestionState.Here, null, null, null,
                ),
                SessionTree.SuggestionRow(
                    "s6", "Preload the hero font", "Saves a round trip.",
                    SessionTree.SuggestionState.Open, null, null, null,
                ),
            ),
        ),
        actions = actions,
        onOpenSession = {},
    )
    Caption("Report")
    SessionXref(
        SessionTree.SessionXref("report", "c2", "Lazy-load product images", null),
        onOpen = {},
    )
    // EXP-1211: the quick replies above the session composer.
    Caption(SessionTree.RepliesCopy.LABEL)
    ComposerRepliesRow(
        replies = SessionTree.ComposerReplies(
            messageId = "m3",
            items = listOf(
                SessionTree.ComposerReply("q1", "Yes, open the PR"),
                SessionTree.ComposerReply("q2", "Run the tests first"),
                SessionTree.ComposerReply("q3", "Show me the diff before you push anything"),
            ),
        ),
        pending = false,
        onSend = {},
        onDismiss = {},
    )
}
