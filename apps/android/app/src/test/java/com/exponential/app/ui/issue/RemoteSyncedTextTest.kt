package com.exponential.app.ui.issue

import com.exponential.app.ui.markdown.stripDraftImages
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Reconciliation tests for RemoteSyncedText — the remote-vs-local merge the
 * issue detail screen relies on. It is @Stable Compose state but its mutations
 * are plain logic, so they exercise fine off the main thread in a JVM test
 * (same setup as EditorModelTest).
 */
class RemoteSyncedTextTest {

    @Test
    fun firstSyncSeedsAndIsClean() {
        val field = RemoteSyncedText()
        field.syncRemote("Hello")
        assertEquals("Hello", field.text)
        assertFalse(field.isDirty)
        assertNull(field.pendingRemote)
    }

    @Test
    fun unrelatedReEmissionWhileDirtyRaisesNoBanner() {
        // The Room flow re-emits the whole row when any column changes; a status
        // flip while the user is typing must not raise a spurious conflict.
        val field = RemoteSyncedText()
        field.syncRemote("base")
        field.onUserEdit("base + typing")
        field.syncRemote("base") // remote == baseline: no divergence
        assertNull(field.pendingRemote)
        assertEquals("base + typing", field.text)
        assertTrue(field.isDirty)
    }

    @Test
    fun echoOfOwnSaveThroughTrimReBaselinesAndKeepsText() {
        // Title save sends the trimmed form; the Electric echo carries it back.
        // The trailing space keeps the field byte-dirty against the new baseline.
        val field = RemoteSyncedText(normalizeForEcho = { it.trim() })
        field.syncRemote("Title")
        field.onUserEdit("New title ")
        field.syncRemote("New title") // normalizeForEcho("New title ") == remote
        assertEquals("New title ", field.text)
        assertNull(field.pendingRemote)
        assertTrue(field.isDirty)
    }

    @Test
    fun echoOfOwnSaveThroughStripDraftKeepsDraftRow() {
        // Description save strips `draft://` rows; the echo carries the stripped
        // text. The draft stays in the editor and keeps the field dirty.
        val field = RemoteSyncedText(normalizeForEcho = ::stripDraftImages)
        field.syncRemote("Original")
        val withDraft = "Progress\n\n![image](draft://abc)"
        field.onUserEdit(withDraft)
        val persisted = stripDraftImages(withDraft)
        field.syncRemote(persisted)
        assertEquals(withDraft, field.text)
        assertNull(field.pendingRemote)
        assertTrue(field.isDirty)
    }

    @Test
    fun echoOfAnEarlierSaveTheUserTypedPastRaisesNoBanner() {
        // EXP-689: the debounced autosave persists "Hello wor"; the user keeps
        // typing, and the Electric echo of that save lands once the field
        // reads "Hello world". Neither the byte check nor normalizeForEcho can
        // match it — only the save record can. No banner, re-baselined, dirty.
        val field = RemoteSyncedText(normalizeForEcho = ::stripDraftImages)
        field.syncRemote("Hello")
        field.setFocused(true)
        field.onUserEdit("Hello wor")
        field.markSaved("Hello wor")
        field.onUserEdit("Hello world")
        field.syncRemote("Hello wor")
        assertNull(field.pendingRemote)
        assertEquals("Hello world", field.text)
        assertTrue(field.isDirty)
        // The next save's echo, arriving with the text unchanged, is the plain
        // normalizeForEcho path and cleans the field.
        field.markSaved("Hello world")
        field.syncRemote("Hello world")
        assertNull(field.pendingRemote)
        assertFalse(field.isDirty)
    }

    @Test
    fun echoLandingBeforeTheSaveRecordIsDroppedByMarkSaved() {
        // Belt and braces for the ordering race: the echo was stashed first,
        // then the save record arrives — the stash is ours, drop it.
        val field = RemoteSyncedText()
        field.syncRemote("Hello")
        field.setFocused(true)
        field.onUserEdit("Hello wor")
        field.onUserEdit("Hello world")
        field.syncRemote("Hello wor")
        assertEquals("Hello wor", field.pendingRemote)
        field.markSaved("Hello wor")
        assertNull(field.pendingRemote)
        assertEquals("Hello world", field.text)
        assertTrue(field.isDirty)
    }

    @Test
    fun foreignRemoteStillStashesAfterASaveRecord() {
        // A teammate's edit that is NOT what we saved keeps the conflict path.
        val field = RemoteSyncedText()
        field.syncRemote("Hello")
        field.setFocused(true)
        field.onUserEdit("Hello wor")
        field.markSaved("Hello wor")
        field.onUserEdit("Hello world")
        field.syncRemote("Hello from a teammate")
        assertEquals("Hello from a teammate", field.pendingRemote)
        assertEquals("Hello world", field.text)
    }

    @Test
    fun cleanUnfocusedRemoteChangeLiveApplies() {
        val field = RemoteSyncedText()
        field.syncRemote("base")
        field.syncRemote("remote update")
        assertEquals("remote update", field.text)
        assertNull(field.pendingRemote)
        assertFalse(field.isDirty)
    }

    @Test
    fun dirtyRemoteChangeStashesAndReloadSwaps() {
        val field = RemoteSyncedText()
        field.syncRemote("base")
        field.onUserEdit("local edit")
        field.syncRemote("remote edit")
        assertEquals("local edit", field.text)
        assertEquals("remote edit", field.pendingRemote)
        assertTrue(field.isDirty)

        assertTrue(field.reloadPending())
        assertEquals("remote edit", field.text)
        assertNull(field.pendingRemote)
        assertFalse(field.isDirty)
        // Nothing left to reload.
        assertFalse(field.reloadPending())
    }

    @Test
    fun focusedCleanRemoteChangeStashesAndBlurApplies() {
        val field = RemoteSyncedText()
        field.syncRemote("base")
        field.setFocused(true)
        field.syncRemote("remote while focused")
        // Focused (even though untyped) stashes rather than yanking the field.
        assertEquals("base", field.text)
        assertEquals("remote while focused", field.pendingRemote)
        assertFalse(field.isDirty)

        // Blur with clean text auto-applies the stash.
        field.setFocused(false)
        assertEquals("remote while focused", field.text)
        assertNull(field.pendingRemote)
    }

    @Test
    fun foreignRemoteAfterEchoStashesToProtectDraftUpload() {
        // Byte-equality clean gate (not normalized): after an echo re-baseline
        // with a draft still in the editor, a foreign remote must stash — a
        // normalized clean check would have live-applied and wiped the draft.
        val field = RemoteSyncedText(normalizeForEcho = ::stripDraftImages)
        field.syncRemote("Original")
        val withDraft = "Progress\n\n![image](draft://xyz)"
        field.onUserEdit(withDraft)
        field.syncRemote(stripDraftImages(withDraft)) // echo re-baseline
        assertTrue(field.isDirty)

        field.syncRemote("Teammate rewrite")
        assertEquals(withDraft, field.text)
        assertEquals("Teammate rewrite", field.pendingRemote)
    }
}
