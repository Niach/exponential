package com.exponential.app.ui.actions

import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.AutomationTrigger
import org.junit.Assert.assertEquals
import org.junit.Test

// Release train 2026-10-10, F44 (web `action-triggers-section.tsx` ×4): the
// trigger row's launch caption is `agent · model`; the effort never rides.
class TriggerLaunchLabelTest {

    private fun trigger(agent: String?, model: String? = null, effort: String? = null) = ActionTrigger(
        id = "t1",
        deviceId = "dev-1",
        agent = agent,
        model = model,
        effort = effort,
        whenPart = AutomationTrigger.Schedule(interval = "daily", minuteOfDay = 540),
    )

    @Test
    fun `reads agent and model, never the effort`() {
        assertEquals("Claude Code · Opus", triggerLaunchLabel(trigger("claude", model = "opus", effort = "high")))
        assertEquals("Codex", triggerLaunchLabel(trigger("codex", effort = "high")))
    }

    @Test
    fun `an unpinned trigger has no pin text`() {
        assertEquals("", triggerLaunchLabel(trigger(agent = null, model = "opus", effort = "high")))
    }
}
