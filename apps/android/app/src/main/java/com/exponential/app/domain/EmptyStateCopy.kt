package com.exponential.app.domain

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-1274: THE empty-state copy ×4 (`packages/domain-contract/fixtures/
 * empty-state.json`, web `lib/empty-states.ts`, desktop `empty_state_copy.rs`,
 * iOS `EmptyStateCopy.swift`), drawn by `ui/components/Scaffolding.kt`
 * `EmptyState`. Titles are at most four words with no trailing period; a
 * description exists only on `page`; a `list` state never carries a button
 * (the section band's trailing pill is the call to action). Locked to the
 * fixture by `EmptyStateCopyTest`.
 */
object EmptyStateCopy {
    enum class Size { Page, List }

    /** One fixture state. [iconConcept] = the icons.json concept id. */
    data class State(
        val id: String,
        val size: Size,
        val iconConcept: String,
        val title: String,
        val description: String? = null,
        val action: String? = null,
    ) {
        val icon: ImageVector get() = conceptIcon(iconConcept) ?: ExpIcons.uiEmptySearch
    }

    val inbox = State("inbox", Size.Page, "nav-inbox", "All caught up", null, null)
    val myIssues = State("my-issues", Size.Page, "nav-my-issues", "No issues assigned to you", null, null)
    val drafts = State("drafts", Size.Page, "nav-drafts", "No drafts", null, null)
    val threads = State("threads", Size.Page, "nav-threads", "No sessions yet", null, null)
    val boards = State("boards", Size.Page, "nav-boards", "No boards yet", "Create a board to start tracking work.", "Create a board")
    val issues = State("issues", Size.Page, "nav-issues", "No issues yet", "Create an issue to start tracking work.", "New issue")
    val noTeam = State("no-team", Size.Page, "ui-team", "No team yet", "Create a team, or join one with an invite link.", null)
    val runNotFound = State("run-not-found", Size.Page, "action-run", "Session not found", null, "Go to Agent")
    val threadNotFound = State(
        "thread-not-found",
        Size.Page,
        "work-faces",
        "Thread not found",
        "It may still be syncing, or it was deleted.",
        "Go to Agent",
    )
    val noSelectionIssue = State("no-selection-issue", Size.Page, "ui-issue", "No issue selected", null, null)
    val noSelectionNotification = State("no-selection-notification", Size.Page, "nav-inbox", "No notification selected", null, null)
    val boardSwitcher = State("board-switcher", Size.List, "nav-boards", "No boards yet", null, null)
    val boardIssues = State("board-issues", Size.List, "nav-issues", "No issues", null, null)
    val teamBoards = State("team-boards", Size.List, "settings-boards", "No boards yet", null, null)
    val archivedBoards = State("archived-boards", Size.List, "ui-archive", "No archived boards", null, null)
    val teams = State("teams", Size.List, "ui-team", "No teams yet", null, null)
    val noTeamSelected = State("no-team-selected", Size.List, "ui-team", "No team selected", null, null)
    val actionRuns = State("action-runs", Size.List, "action-run", "No sessions yet", null, null)
    val actionTriggers = State("action-triggers", Size.List, "trigger-event", "No triggers yet", null, null)
    val recentRuns = State("recent-runs", Size.List, "coding-ended", "No recent sessions", null, null)
    val runningAgents = State("running-agents", Size.List, "coding-running", "No agents running", null, null)
    val devices = State("devices", Size.List, "nav-devices", "No devices yet", null, null)
    val signedInDevices = State("signed-in-devices", Size.List, "ui-device", "No signed-in devices", null, null)
    val mcpServers = State("mcp-servers", Size.List, "settings-mcp", "No MCP servers yet", null, null)
    val builtinToolsNoMatch = State("builtin-tools-no-match", Size.List, "ui-empty-search", "No tools match", null, null)
    val widgets = State("widgets", Size.List, "settings-widget", "No widgets yet", null, null)
    val labels = State("labels", Size.List, "settings-labels", "No labels yet", null, null)
    val statuses = State("statuses", Size.List, "settings-statuses", "No statuses yet", null, null)
    val passkeys = State("passkeys", Size.List, "auth-passkey", "No passkeys yet", null, null)
    val attachments = State("attachments", Size.List, "ui-attach", "No attachments yet", null, null)
    val transcript = State("transcript", Size.List, "coding-assistant", "Nothing from this agent yet", null, null)
    val changedFiles = State("changed-files", Size.List, "coding-diff", "No changed files", null, null)
    val guide = State("guide", Size.List, "work-results", "No report yet", null, null)
    val activity = State("activity", Size.List, "ui-clock", "No activity yet", null, null)

    /** Every state, fixture order. */
    val all: List<State> = listOf(
        inbox,
        myIssues,
        drafts,
        threads,
        boards,
        issues,
        noTeam,
        runNotFound,
        threadNotFound,
        noSelectionIssue,
        noSelectionNotification,
        boardSwitcher,
        boardIssues,
        teamBoards,
        archivedBoards,
        teams,
        noTeamSelected,
        actionRuns,
        actionTriggers,
        recentRuns,
        runningAgents,
        devices,
        signedInDevices,
        mcpServers,
        builtinToolsNoMatch,
        widgets,
        labels,
        statuses,
        passkeys,
        attachments,
        transcript,
        changedFiles,
        guide,
        activity,
    )

    /** A concept id → its glyph; null for a concept this file never names. */
    fun conceptIcon(concept: String): ImageVector? = when (concept) {
        "action-run" -> ExpIcons.actionRun
        "auth-passkey" -> ExpIcons.authPasskey
        "coding-assistant" -> ExpIcons.codingAssistant
        "coding-diff" -> ExpIcons.codingDiff
        "coding-ended" -> ExpIcons.codingEnded
        "coding-running" -> ExpIcons.codingRunning
        "nav-boards" -> ExpIcons.navBoards
        "nav-devices" -> ExpIcons.navDevices
        "nav-drafts" -> ExpIcons.navDrafts
        "nav-inbox" -> ExpIcons.navInbox
        "nav-issues" -> ExpIcons.navIssues
        "nav-my-issues" -> ExpIcons.navMyIssues
        "nav-threads" -> ExpIcons.navThreads
        "settings-boards" -> ExpIcons.settingsBoards
        "settings-labels" -> ExpIcons.settingsLabels
        "settings-mcp" -> ExpIcons.settingsMcp
        "settings-statuses" -> ExpIcons.settingsStatuses
        "settings-widget" -> ExpIcons.settingsWidget
        "trigger-event" -> ExpIcons.triggerEvent
        "ui-archive" -> ExpIcons.uiArchive
        "ui-attach" -> ExpIcons.uiAttach
        "ui-clock" -> ExpIcons.uiClock
        "ui-device" -> ExpIcons.uiDevice
        "ui-empty-search" -> ExpIcons.uiEmptySearch
        "ui-issue" -> ExpIcons.uiIssue
        "ui-team" -> ExpIcons.uiTeam
        "work-faces" -> ExpIcons.workFaces
        "work-results" -> ExpIcons.workResults
        else -> null
    }
}
