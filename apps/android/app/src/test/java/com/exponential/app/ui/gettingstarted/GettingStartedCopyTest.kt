package com.exponential.app.ui.gettingstarted

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The copy is asserted BYTE-FOR-BYTE against web's canonical table by
// `apps/web/src/components/getting-started/getting-started-copy.test.ts`, which
// reads GettingStartedCopy.kt as text. That test can only find a value if it is
// a plain double-quoted literal, so this suite locks the shape those
// assertions depend on rather than re-typing the strings a second time.
class GettingStartedCopyTest {

    private val keys = GettingStartedEntryKey.entries

    @Test
    fun everyEntryHasCopy() {
        assertEquals(7, keys.size)
        for (key in keys) {
            assertTrue(key.name, GettingStartedCopy.title(key).isNotBlank())
            assertTrue(key.name, GettingStartedCopy.description(key).isNotBlank())
            assertTrue(key.name, GettingStartedCopy.action(key).isNotBlank())
        }
    }

    @Test
    fun copyStaysQuotableInASingleLineLiteral() {
        // A `"`, a backslash or a newline would have to be escaped in the
        // Kotlin source, and the drift test's `includes("<value>")` would then
        // never match it.
        for (key in keys) {
            for (value in listOf(
                GettingStartedCopy.title(key),
                GettingStartedCopy.description(key),
                GettingStartedCopy.action(key),
            )) {
                assertFalse(value, value.contains('"'))
                assertFalse(value, value.contains('\\'))
                assertFalse(value, value.contains('\n'))
                assertEquals(value, value.trim(), value)
            }
        }
    }

    @Test
    fun lockedEntriesNameTheirFeederStep() {
        assertTrue(
            GettingStartedCopy
                .lockedHint(GettingStartedEntryKey.Coding, GettingStartedEntryKey.Desktop)
                .contains("Connect a machine first"),
        )
        assertTrue(
            GettingStartedCopy
                .lockedHint(GettingStartedEntryKey.Coding, GettingStartedEntryKey.Github)
                .contains("Connect a GitHub repo first"),
        )
        // The fallback names the blocking step by its own title.
        assertTrue(
            GettingStartedCopy
                .lockedHint(GettingStartedEntryKey.Server, GettingStartedEntryKey.Board)
                .contains(GettingStartedCopy.BOARD_TITLE),
        )
    }
}
