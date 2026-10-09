package com.exponential.app.ui.settings

import org.junit.Assert.assertEquals
import org.junit.Test

/** P95: the Email code row's caption ×4 (web `emailCodeCaption`). */
class EmailCodeCaptionTest {
    @Test
    fun `on shows the address, off appends the server note`() {
        assertEquals("a@b.c", emailCodeCaption("a@b.c", enabled = true))
        assertEquals("a@b.c · Off on this server", emailCodeCaption("a@b.c", enabled = false))
    }
}
