package com.exponential.app.ui.components

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1520 regression: typing in the composer scrambled letters because the
 * field was reconciled against a draft echo that trailed it by a frame.
 */
class ComposerDraftSyncTest {
    /** One keystroke the way the screens handle it: the field first, then the store. */
    private fun type(sync: ComposerDraftSync, store: MutableStateFlow<String>, text: String, composing: Boolean = true) {
        val next = TextFieldValue(
            text,
            TextRange(text.length),
            // The IME's in-flight word: lost whenever the field is rewritten.
            composition = if (composing) TextRange(0, text.length) else null,
        )
        sync.field = next
        store.value = text
    }

    @Test
    fun aLateEchoOfItsOwnTypingNeverRewritesTheField() {
        val store = MutableStateFlow("")
        val sync = ComposerDraftSync(store)
        type(sync, store, "h")
        type(sync, store, "he")
        type(sync, store, "hel")
        val typed = sync.field
        // The echoes of "h" and "he" arrive only now (the collector ran a frame
        // late). The old effect adopted each one, so the field jumped back to
        // "h", the next keystroke landed on it, and the IME restarted.
        repeat(3) { assertFalse(sync.adoptStored()) }
        assertSame(typed, sync.field)
        assertEquals(TextRange(0, 3), sync.field.composition)
    }

    @Test
    fun aFastTypistKeepsEveryLetterInOrder() = runBlocking {
        val store = MutableStateFlow("")
        val sync = ComposerDraftSync(store)
        var rewrites = 0
        // Undispatched, then suspended on each emission: the collector sees
        // the store whenever it gets to run, mid-word included.
        val follower = launch(Dispatchers.Unconfined, start = CoroutineStart.UNDISPATCHED) {
            sync.follow { rewrites++ }
        }
        val sentence = "the quick brown fox jumps over the lazy dog"
        for (end in 1..sentence.length) type(sync, store, sentence.take(end))
        assertEquals(sentence, sync.field.text)
        assertEquals(0, rewrites)
        follower.cancel()
    }

    @Test
    fun anOutsideRewriteIsAdoptedWithAClampedCaret() {
        val store = MutableStateFlow("")
        val sync = ComposerDraftSync(store)
        type(sync, store, "draft to send")
        // The send landed: the store clears.
        store.value = ""
        assertTrue(sync.adoptStored())
        assertEquals("", sync.field.text)
        assertEquals(TextRange(0), sync.field.selection)
        assertEquals(null, sync.field.composition)
        // A `/` pick replaces the draft.
        store.value = "/compact"
        assertTrue(sync.adoptStored())
        assertEquals("/compact", sync.field.text)
    }

    @Test
    fun aRestoredDraftOpensWithTheCaretAtTheEnd() {
        val sync = ComposerDraftSync(MutableStateFlow("kept from last time"))
        assertEquals("kept from last time", sync.field.text)
        assertEquals(TextRange(19), sync.field.selection)
    }
}
