package com.exponential.app.domain

import com.exponential.app.data.api.AgentLaunchDefaults
import com.exponential.app.data.api.DeviceLaunchDefaults
import org.junit.Assert.assertEquals
import org.junit.Test

/** The composer's model pin fallback chain (web `fallbackModel`). */
class ComposerModelTest {
    private fun live(model: String) =
        SessionConfigState(options = listOf(ConfigOption(id = "model", label = "Model", value = model)))

    private val device = DeviceLaunchDefaults(
        agents = mapOf(
            "claude" to AgentLaunchDefaults(model = "sonnet", threadModel = "opus"),
            "codex" to AgentLaunchDefaults(model = "gpt-codex-work", threadModel = "gpt-codex-thread"),
        ),
    )

    @Test fun liveRelayModelWins() {
        assertEquals("opus", composerModel(live("opus"), "haiku", "claude", device))
    }

    @Test fun blankLiveModelFallsThrough() {
        assertEquals("haiku", composerModel(live(""), "haiku", "claude", null))
    }

    @Test fun rowModelBeforeDeviceDefaults() {
        assertEquals("fable", composerModel(null, "fable", "claude", device))
    }

    @Test fun workSessionTakesDeviceModelThenContractDefault() {
        assertEquals("sonnet", composerModel(null, null, "claude", device))
        assertEquals(DomainContract.deviceAgentDefaultsModel, composerModel(null, null, "claude", null))
        assertEquals(DomainContract.deviceAgentDefaultsCodexModel, composerModel(null, null, "codex", null))
    }
}
