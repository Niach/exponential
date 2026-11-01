package com.exponential.app.ui.session

import com.exponential.app.domain.AgentPhase
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1538 (dock.json LOCAL FIRST): the last-known Queue draws while the stream (re)joins. */
class DockStreamSlotsTest {
    @Test
    fun lastKnownSlotsShowWhileTheStreamIsNotLiveYet() {
        listOf(
            AgentPhase.Idle,
            AgentPhase.Connecting,
            AgentPhase.Starting,
            AgentPhase.Live,
            AgentPhase.Closed(reconnecting = true),
        ).forEach { phase -> assertTrue("$phase", dockStreamSlots(phase, sessionEnded = false)) }
    }

    @Test
    fun aSessionThatIsOverShowsNone() {
        assertFalse(dockStreamSlots(AgentPhase.Ended(), sessionEnded = false))
        assertFalse(dockStreamSlots(AgentPhase.Live, sessionEnded = true))
    }
}
