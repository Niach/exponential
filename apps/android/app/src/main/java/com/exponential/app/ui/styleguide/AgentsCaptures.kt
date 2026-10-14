package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.domain.AgentTrigger
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.TriggerWhen
import com.exponential.app.ui.actions.ActionFormState
import com.exponential.app.ui.actions.ActionPromptForm
import com.exponential.app.ui.actions.NEW_ACTION_TITLE
import com.exponential.app.ui.components.picker.AccountPickerOption
import com.exponential.app.ui.components.picker.DevicePickerDevice
import com.exponential.app.ui.settings.AgentSettingsContent
import com.exponential.app.ui.settings.AgentsSettingsListContent
import com.exponential.app.ui.theme.AppBackground

/** Which Settings → Agents / Actions surface a capture shows (one shot each). */
enum class AgentsCaptureKind(val testId: String) {
    AgentsList("agents-settings-list-screen"),
    AgentPage("agent-settings-page-screen"),
    ActionCreate("action-create-screen"),
}

/**
 * EXP-1296 (Threads v2): the capture-only styleguide screen of lane AGENTS —
 * the Team settings Agents section, an agent's page (Triage) and the
 * `New action` form, each drawn by its REAL composable over draft-6 data
 * built with the real domain model (no server for shots).
 */
@Composable
fun AgentsCaptures(kind: AgentsCaptureKind) {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(kind.testId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .padding(top = 16.dp),
            ) {
                when (kind) {
                    AgentsCaptureKind.AgentsList -> {
                        Title("Settings")
                        AgentsSettingsListContent(
                            agents = SampleAgents,
                            canCreate = true,
                            onOpenAgent = {},
                            onNewAgent = {},
                            modifier = Modifier.padding(16.dp),
                        )
                    }
                    AgentsCaptureKind.AgentPage -> {
                        Title("Triage")
                        AgentPageSample()
                    }
                    AgentsCaptureKind.ActionCreate -> {
                        Title(NEW_ACTION_TITLE)
                        val form = remember { SampleActionForm() }
                        ActionPromptForm(
                            form = form,
                            editable = true,
                            modifier = Modifier
                                .fillMaxWidth()
                                .verticalScroll(rememberScrollState())
                                .padding(vertical = 8.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun Title(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        textAlign = TextAlign.Center,
        modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp),
    )
}

private val SampleAgents = listOf(
    SessionTree.AgentRow("agent-triage", "team-acme", "Triage", "bug", "#a855f7", busy = false, needsYou = false),
    SessionTree.AgentRow("agent-designer", "team-acme", "Designer", "palette", "#ec4899", busy = false, needsYou = false),
    SessionTree.AgentRow("agent-release", "team-acme", "Release", "package", "#22c55e", busy = false, needsYou = false),
)

@Composable
private fun AgentPageSample() {
    val devices = listOf(
        DevicePickerDevice(id = "dev-build", name = "Acme build server", isServer = true),
        DevicePickerDevice(id = "dev-mac", name = "Alex's MacBook Pro"),
    )
    val accounts = listOf(
        AccountPickerOption(key = "claude:p1", agent = "claude", email = "demo@exponential.at"),
    )
    AgentSettingsContent(
        isNew = false,
        editable = true,
        name = "Triage",
        onNameChange = {},
        onNameCommit = {},
        icon = "bug",
        onIconChange = {},
        color = "#a855f7",
        onColorChange = {},
        devices = devices,
        deviceId = "dev-build",
        onDeviceChange = {},
        accounts = accounts,
        accountKey = "claude:p1",
        defaultAccountKey = "claude:p1",
        onAccountChange = {},
        triggers = listOf(
            AgentTrigger(id = "trigger-daily", enabled = true, whenPart = TriggerWhen.Schedule("daily", 9 * 60)),
        ),
        onToggleTrigger = { _, _ -> },
        onEditTrigger = {},
        onAddTrigger = {},
        instructions = "Look after the boards: merge duplicates, add labels and priorities, " +
            "start a thread for anything that needs code.",
        instructionsLoading = false,
        onInstructionsChange = {},
        onInstructionsCommit = {},
        errors = emptyList(),
        busy = false,
        onCreate = null,
        onDelete = {},
        modifier = Modifier.fillMaxSize(),
    )
}

private fun SampleActionForm(): ActionFormState = ActionFormState().apply {
    name = "Reduce cold start"
    icon = "rocket"
    promptPlaceholder = "Which screen, which target time"
    inputs = listOf(
        ActionInputDto(key = "issue", label = "Issue", type = "issue", required = true),
        ActionInputDto(key = "repository", label = "Repository", type = "repo"),
    )
    body = "Profile the app's cold start for the picked issue.\n\n" +
        "1. Measure the current time to first frame.\n" +
        "2. Move work off the main thread until it is below 800 ms.\n" +
        "3. Open a pull request with the before/after numbers."
}
