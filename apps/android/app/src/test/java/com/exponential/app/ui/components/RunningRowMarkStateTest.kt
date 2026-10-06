package com.exponential.app.ui.components

import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.domain.TreeGuides
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-1208: a live session row's run mark, ×4 (web `runningRowMarkState`,
// desktop `run_rows::running_row_mark`).
class RunningRowMarkStateTest {

    @Test
    fun `a paused run wears the bare mark whatever its state`() {
        assertNull(runningRowMarkState(CodingSessionDisplayState.Review, paused = true, working = false))
    }

    @Test
    fun `only a working row animates`() {
        assertEquals(
            CodingSessionDisplayState.Working,
            runningRowMarkState(CodingSessionDisplayState.Working, paused = false, working = true),
        )
        assertNull(runningRowMarkState(CodingSessionDisplayState.Working, paused = false, working = false))
    }

    @Test
    fun `a parked state keeps its badge`() {
        for (state in listOf(
            CodingSessionDisplayState.NeedsInput,
            CodingSessionDisplayState.Review,
            CodingSessionDisplayState.Done,
        )) {
            assertEquals(state, runningRowMarkState(state, paused = false, working = false))
        }
    }

    @Test
    fun `the mark is one indent level square`() {
        // Its centre is the gutter centre a child's elbow hangs off.
        assertEquals(TreeGuides.INDENT_DP.toFloat(), SessionRowMarkSize.value)
        assertEquals(0.5f, RUN_MARK_ENDED_ALPHA)
    }
}
