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
        assertEquals("opus", composerModel(live("opus"), "haiku", "claude", true, device))
    }

    @Test fun blankLiveModelFallsThrough() {
        assertEquals("haiku", composerModel(live(""), "haiku", "claude", false, null))
    }

    @Test fun rowModelBeforeDeviceDefaults() {
        assertEquals("fable", composerModel(null, "fable", "claude", true, device))
    }

    @Test fun dispatcherTakesDeviceThreadModel() {
        assertEquals("opus", composerModel(null, null, "claude", true, device))
        assertEquals("gpt-codex-thread", composerModel(null, null, "codex", true, device))
    }

    @Test fun dispatcherWithoutDeviceTakesContractThreadDefault() {
        assertEquals(DomainContract.deviceAgentDefaultsThreadModel, composerModel(null, null, null, true, null))
        assertEquals("haiku", composerModel(null, "", "claude", true, DeviceLaunchDefaults()))
        assertEquals(DomainContract.deviceAgentDefaultsCodexThreadModel, composerModel(null, null, "codex", true, null))
    }

    @Test fun workSessionTakesDeviceModelThenContractDefault() {
        assertEquals("sonnet", composerModel(null, null, "claude", false, device))
        assertEquals(DomainContract.deviceAgentDefaultsModel, composerModel(null, null, "claude", false, null))
        assertEquals(DomainContract.deviceAgentDefaultsCodexModel, composerModel(null, null, "codex", false, null))
    }
}
