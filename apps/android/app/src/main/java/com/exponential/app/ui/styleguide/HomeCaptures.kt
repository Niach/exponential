package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.domain.AccountOption
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.AgentHealth
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.ComposerMenuConditions
import com.exponential.app.domain.mcpPickValue
import com.exponential.app.ui.agent.AgentActionPickerPage
import com.exponential.app.ui.agent.AgentComposer
import com.exponential.app.ui.agent.AgentComposerHeadline
import com.exponential.app.ui.agent.AgentIssuePickerPage
import com.exponential.app.ui.agent.AgentOptionsRow
import com.exponential.app.ui.agent.AgentPageFrame
import com.exponential.app.ui.agent.ComposerPlusMenuSheet
import com.exponential.app.ui.agent.ComposerSubject
import com.exponential.app.ui.agent.IssueOption
import com.exponential.app.ui.agent.LaunchDraft
import com.exponential.app.ui.agent.StartBoardOption
import com.exponential.app.ui.agent.StartPullRequestOption
import com.exponential.app.ui.agent.composerPlaceholder
import com.exponential.app.ui.agent.homeRunningBand
import com.exponential.app.ui.components.picker.IssuePickerIssue
import com.exponential.app.ui.theme.AppBackground

/** Which Home capture to draw (one shot each). */
enum class HomeCaptureKind(val testId: String) {
    /** sg_agent-home: the Agent page — the dawn under the title, nothing
     *  picked, the Running band under the column. */
    Home("home-capture-screen"),
    /** sg_chat-issues: the composer with two issue chips. */
    ChatIssues("chat-issues-capture-screen"),
    /** A saved action picked, its typed inputs filled (no catalog shot since EXP-1295). */
    ChatAction("chat-action-capture-screen"),
    /** sg_composer-menu: the "+" menu open over the home. */
    ComposerMenu("composer-menu-capture-screen"),
}

/**
 * EXP-1296: the Agent home as a capture-only screen — the REAL page frame
 * ([AgentPageFrame]: the top bar + the dawn), [homeRunningBand] (EXP-1306),
 * [AgentComposerHeadline], [AgentComposer], [AgentOptionsRow] and [ComposerPlusMenuSheet] over the
 * draft's data (8-phones panel 1: Acme, Alex's MacBook Pro, APP-5 / APP-14).
 */
@Composable
fun HomeCaptures(kind: HomeCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            val subject: ComposerSubject? = when (kind) {
                HomeCaptureKind.ChatIssues -> ComposerSubject.Issues(listOf(APP_5.id, APP_14.id))
                HomeCaptureKind.ChatAction -> ComposerSubject.Action(
                    SampleAction.id,
                    mapOf("issue" to APP_5.id, "repo" to SampleRepo.id),
                )
                else -> null
            }
            val action = SampleAction.takeIf { subject is ComposerSubject.Action }
            val issueChips = (subject as? ComposerSubject.Issues)?.ids.orEmpty()
                .mapNotNull { id -> SampleIssues.firstOrNull { it.id == id } }
            val promptSubject = when {
                action != null -> AgentComposerPrompt.Subject.Action
                issueChips.isNotEmpty() -> AgentComposerPrompt.Subject.Issues(issueChips.size)
                else -> AgentComposerPrompt.Subject.None
            }
            val draft = when (kind) {
                HomeCaptureKind.ChatIssues -> "Keep both fixes in one PR."
                else -> ""
            }
            val (runningRows, runningMeta) = remember { sampleHomeRunning() }
            AgentPageFrame(onBack = null, dawn = promptSubject == AgentComposerPrompt.Subject.None) {
                if (promptSubject != AgentComposerPrompt.Subject.None) {
                    item(key = "headline") {
                        AgentComposerHeadline(
                            headline = AgentComposerPrompt.headline(promptSubject),
                            issueChips = issueChips,
                            onRemoveIssue = {},
                            actionChip = action,
                            onClearAction = {},
                        )
                    }
                }
                item(key = "composer") {
                    AgentComposer(
                        value = TextFieldValue(draft, TextRange(draft.length)),
                        onValueChange = {},
                        onValueRewrite = {},
                        fieldModifier = Modifier,
                        placeholder = composerPlaceholder(subject, action, home = true),
                        actionChip = action,
                        inputValues = (subject as? ComposerSubject.Action)?.inputs.orEmpty(),
                        repos = listOf(SampleRepo),
                        boards = SampleBoards,
                        pullRequests = emptyList<StartPullRequestOption>(),
                        onInputChange = { _, _ -> },
                        pendingImages = emptyList(),
                        imageError = null,
                        sending = false,
                        onOpenMenu = {},
                        onRemoveImage = {},
                        submitLabel = AgentComposerPrompt.submitTitle(promptSubject),
                        canSubmit = promptSubject != AgentComposerPrompt.Subject.None,
                        onSubmit = {},
                        issues = SampleIssues.map { IssuePickerIssue(it.id, it.identifier, it.title) },
                    )
                }
                item(key = "options") {
                    AgentOptionsRow(
                        devices = listOf(SampleDevice),
                        device = SampleDevice,
                        onDeviceChange = {},
                        launch = SampleLaunch,
                        accountOptions = listOf(SampleAccount),
                        onAccountChange = {},
                        onModelChange = {},
                        onPlanModeChange = {},
                        resumeCandidate = null,
                        resume = false,
                        onResumeChange = {},
                    )
                }
                // EXP-1306: the phone home's Running band (iOS parity).
                if (kind == HomeCaptureKind.Home) homeRunningBand(runningRows, runningMeta) {}
            }
            if (kind == HomeCaptureKind.ComposerMenu) {
                ComposerPlusMenuSheet(
                    layout = ComposerMenu.composerMenuLayout(
                        ComposerMenuConditions(subagentModel = true, ultracode = true, mcp = true, computerUse = true),
                    ),
                    codex = false,
                    effortValue = "High",
                    effortEnabled = true,
                    subagentsValue = "Default",
                    ultracode = false,
                    mcpValue = mcpPickValue(1),
                    computerUse = false,
                    canAttach = true,
                    onRow = {},
                    onUltracodeChange = {},
                    onComputerUseChange = {},
                    onDismiss = {},
                    issuePage = { close ->
                        AgentIssuePickerPage(issues = SampleIssues, checkedIds = emptySet(), onToggle = {}, onDone = close)
                    },
                    actionPage = { close ->
                        AgentActionPickerPage(
                            actions = listOf(SampleAction),
                            error = null,
                            selectedId = null,
                            onPick = { close() },
                        )
                    },
                )
            }
        }
    }
}

private const val TEAM = "t-acme"

private fun sampleIssue(id: String, identifier: String, title: String, status: String) = IssueOption(
    id = id,
    identifier = identifier,
    title = title,
    repositoryId = "r-mobile",
    status = status,
    priority = null,
    description = null,
    createdAt = "2026-10-10T09:00:00.000Z",
    updatedAt = "2026-10-12T09:00:00.000Z",
)

private val APP_5 = sampleIssue("i-app-5", "APP-5", "Reduce cold start below 800 ms", "in_progress")
private val APP_14 = sampleIssue("i-app-14", "APP-14", "Group board issues by status", "backlog")
private val APP_15 = sampleIssue("i-app-15", "APP-15", "Batch-edit labels on the board", "backlog")
private val SampleIssues = listOf(APP_5, APP_14, APP_15)

private val SampleRepo = TeamRepo(id = "r-mobile", fullName = "acme/mobile-app")

private val SampleBoards = listOf(
    StartBoardOption(id = "b-mobile", name = "Mobile App", icon = "board-code", colorHex = "#3b82f6"),
)

private val SampleAction = ActionDto(
    id = "a-release-notes",
    teamId = TEAM,
    name = "Write release notes",
    icon = "action-default",
    inputs = listOf(
        ActionInputDto(key = "issue", label = "Issue", type = "issue", required = true),
        ActionInputDto(key = "repo", label = "Repository", type = "repo", required = true),
    ),
    promptPlaceholder = "Anything the notes should mention?",
)

private val SampleDevice = SteerDevice(
    deviceId = "d-macbook",
    deviceLabel = "Alex's MacBook Pro",
    agents = listOf("claude"),
    acpAgents = listOf("claude"),
)

private val SampleAccount = AccountOption(
    id = "p-demo",
    agent = "claude",
    email = "demo@exponential.at",
    isLastUsed = true,
    health = AgentHealth.Ok,
)

private val SampleLaunch = LaunchDraft(
    pickedDeviceId = SampleDevice.deviceId,
    agent = "claude",
    model = "fable",
    account = SampleAccount.id,
)
