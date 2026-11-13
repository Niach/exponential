package com.exponential.app.data.share

import com.exponential.app.data.share.ShareIntentPlan.ACTION_PROCESS_TEXT
import com.exponential.app.data.share.ShareIntentPlan.ACTION_SEND
import com.exponential.app.data.share.ShareIntentPlan.ACTION_SEND_MULTIPLE
import com.exponential.app.data.share.ShareIntentPlan.Fields
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ShareIntentPlanTest {
    private val mimes = mapOf("png" to "image/png", "jpg" to "image/jpeg", "pdf" to "application/pdf")
    private val exts = mimes.entries.associate { (k, v) -> v to k }

    @Test fun `a single image share plans its stream`() {
        val plan = ShareIntentPlan.plan(Fields(ACTION_SEND, streams = listOf("content://media/1")))!!
        assertEquals(listOf("content://media/1"), plan.streams)
        assertNull(plan.text)
    }

    @Test fun `multiple images keep the sender's order, clip items it already names are not doubled`() {
        val plan = ShareIntentPlan.plan(
            Fields(
                ACTION_SEND_MULTIPLE,
                streams = listOf("content://a", "content://b"),
                clipUris = listOf("content://a", "content://b", "content://c"),
            ),
        )!!
        assertEquals(listOf("content://a", "content://b", "content://c"), plan.streams)
    }

    @Test fun `a sender that fills only ClipData still shares its streams`() {
        val plan = ShareIntentPlan.plan(Fields(ACTION_SEND, clipUris = listOf("content://x")))!!
        assertEquals(listOf("content://x"), plan.streams)
    }

    @Test fun `text and subject ride along, styled CharSequence text included`() {
        val styled: CharSequence = StringBuilder("look at this")
        val plan = ShareIntentPlan.plan(
            Fields<String>(ACTION_SEND, text = styled, subject = "Crash report", shortcutId = "share-chat:s1"),
        )!!
        assertEquals("look at this", plan.text)
        assertEquals("Crash report", plan.subject)
        assertEquals("share-chat:s1", plan.directShareId)
    }

    @Test fun `blank text and no streams is no share`() {
        assertNull(ShareIntentPlan.plan(Fields<String>(ACTION_SEND, text = "   ")))
    }

    @Test fun `process text takes the selection only`() {
        val plan = ShareIntentPlan.plan(
            Fields(ACTION_PROCESS_TEXT, processText = "selected", streams = listOf("content://ignored")),
        )!!
        assertEquals("selected", plan.text)
        assertTrue(plan.streams.isEmpty())
        assertNull(ShareIntentPlan.plan(Fields<String>(ACTION_PROCESS_TEXT, processText = "")))
    }

    @Test fun `other actions are not shares`() {
        assertNull(ShareIntentPlan.plan(Fields(ACTION_VIEW, streams = listOf("content://a"))))
        assertFalse(ShareIntentPlan.isShareAction(null))
    }

    @Test fun `the provider's type wins`() {
        assertEquals("image/png", ShareIntentPlan.mimeOf("image/png", "a.pdf", "*/*", mimes::get))
    }

    @Test fun `a provider with no type falls back to the name, then the intent's type`() {
        assertEquals("image/jpeg", ShareIntentPlan.mimeOf(null, "IMG_1.jpg", "*/*", mimes::get))
        assertEquals("image/jpeg", ShareIntentPlan.mimeOf("application/octet-stream", "IMG_1.jpg", null, mimes::get))
        assertEquals("image/png", ShareIntentPlan.mimeOf(null, "1000000033", "image/png", mimes::get))
        assertTrue(ShareIntentPlan.isImage(ShareIntentPlan.mimeOf(null, "1000000033", "image/*", mimes::get)))
        assertNull(ShareIntentPlan.mimeOf(null, "blob", "*/*", mimes::get))
    }

    @Test fun `the cached name keeps the sender's name and gains an extension its type implies`() {
        assertEquals("IMG_1.jpg", ShareIntentPlan.cachedName("IMG_1.jpg", "image/jpeg", exts::get))
        assertEquals("1000000033.png", ShareIntentPlan.cachedName("1000000033", "image/png", exts::get))
        assertEquals("report.pdf", ShareIntentPlan.cachedName("../../report.pdf", "application/pdf", exts::get))
        assertEquals("image.webp", ShareIntentPlan.cachedName(null, "image/webp", exts::get))
        assertEquals("file", ShareIntentPlan.cachedName("..", null, exts::get))
    }

    @Test fun `an intent is consumed once - a recreation or a Recents relaunch never re-applies it`() {
        assertTrue(ShareIntentPlan.isFreshDelivery(hasSavedState = false, flags = 0))
        assertFalse(ShareIntentPlan.isFreshDelivery(hasSavedState = true, flags = 0))
        assertFalse(
            ShareIntentPlan.isFreshDelivery(
                hasSavedState = false,
                flags = ShareIntentPlan.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY,
            ),
        )
    }

    private companion object {
        const val ACTION_VIEW = "android.intent.action.VIEW"
    }
}
