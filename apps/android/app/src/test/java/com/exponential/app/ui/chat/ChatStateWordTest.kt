package com.exponential.app.ui.chat

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.domain.SessionScreen
import org.junit.Assert.assertEquals
import org.junit.Test

/** #EXP-1732: the header's state word = session-screen.json STATUS WORD
 *  (`SessionScreenTest` replays `statusCases`); this locks the synced rows'
 *  adapter: the ROOT is the row whose id is the chat's, and every row of the
 *  tree counts (never the root alone). */
class ChatStateWordTest {
    private fun row(id: String, status: String = "idle", busy: Boolean = false, needsInput: Boolean = false) =
        SessionEntity(id = id, rootId = "r", status = status, busy = busy, needsInput = needsInput)

    @Test fun emptyTreeIsIdle() = assertEquals(SessionScreen.Copy.Status.IDLE, chatStateWord("r", emptyList()))

    @Test fun idleRootIsLowercase() = assertEquals("idle", chatStateWord("r", listOf(row("r"))))

    @Test fun aChildNeedingInputIsTheTreesWord() = assertEquals(
        SessionScreen.Copy.Status.NEEDS_INPUT,
        chatStateWord("r", listOf(row("r"), row("c", needsInput = true))),
    )

    @Test fun aBusyChildMakesTheTreeWorking() = assertEquals(
        SessionScreen.Copy.Status.WORKING,
        chatStateWord("r", listOf(row("r", status = "ended"), row("c", status = "running", busy = true))),
    )

    @Test fun theRootQueued() = assertEquals(SessionScreen.Copy.Status.QUEUED, chatStateWord("r", listOf(row("r", status = "queued"))))

    @Test fun onlyTheRootEndingSaysDone() {
        assertEquals("done", chatStateWord("r", listOf(row("r", status = "ended"), row("c", status = "idle"))))
        // An ended child never makes the tree done.
        assertEquals("idle", chatStateWord("r", listOf(row("r"), row("c", status = "ended"))))
    }
}
