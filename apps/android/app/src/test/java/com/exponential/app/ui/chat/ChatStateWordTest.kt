package com.exponential.app.ui.chat

import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** #EXP-1732: the header's state word (session-screen.json `subtitle`), iOS alike. */
class ChatStateWordTest {
    private fun view(live: Boolean = true, busy: Boolean = false, needsYou: Boolean = false) = SessionTree.ChatView(
        rootId = "r",
        title = "Triage",
        private = false,
        focusId = null,
        tabs = emptyList(),
        rows = emptyList(),
        composer = SessionTree.Composer("r", ""),
        live = live,
        busy = busy,
        needsYou = needsYou,
    )

    @Test fun noView() = assertNull(chatStateWord(null))

    @Test fun needsYouFirst() =
        assertEquals(SessionTree.ChatRowWord.NeedsInput.text, chatStateWord(view(busy = true, needsYou = true)))

    @Test fun working() = assertEquals(Chats.Copy.PREVIEW_WORKING, chatStateWord(view(busy = true)))

    @Test fun idle() = assertEquals("Idle", chatStateWord(view()))

    @Test fun everySessionEnded() = assertEquals("Done", chatStateWord(view(live = false)))
}
