package at.exponential.ui.kitchensink

import android.content.Intent
import android.util.Log
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * The TalkBack order of the kitchen sink: the app writes the walk Compose's
 * accessibility delegate hands TalkBack (`a11yDump`, [AccessibilityWalk])
 * and this test reads it. Mirrors the iOS `AccessibilityOrderTests`.
 */
@RunWith(AndroidJUnit4::class)
class AccessibilityOrderTest {
    @Test
    fun kitchenSinkReadsInPreOrder() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(context.getExternalFilesDir(null) ?: context.filesDir, "kitchen-sink-a11y.txt")
        file.delete()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra("shot", "exponential-ui-kitchen-sink")
            .putExtra("a11yDump", file.absolutePath)
        ActivityScenario.launch<MainActivity>(intent).use {
            val deadline = System.currentTimeMillis() + 20_000
            while (!file.exists() && System.currentTimeMillis() < deadline) Thread.sleep(200)
            val text = file.readText()
            val labels = text.substringBefore("\n---\n").split("\n").filter { it.isNotEmpty() }
            Log.i("a11y-walk", labels.joinToString(" | "))
            val prefix = listOf("Alex Chen", "Reddit radar", "Kitchen sink · one catalog on every client", "3", "Scan now", "Sources")
            assertEquals("walk: ${labels.take(20)}", prefix, labels.take(prefix.size))
            fun position(label: String): Int = labels.indexOfFirst { it.startsWith(label) }.let { if (it < 0) Int.MAX_VALUE else it }
            assertTrue(position("r/selfhosted") < position("r/opensource"))
            assertTrue(position("Auto-scan") < position("Drafts"))
            assertTrue(position("Scanning") < position("Quota"))
            assertTrue(position("All") < position("Archived"))
            assertTrue(position("Title") < position("Posting guidelines"))
            assertTrue("${labels.size} labels", labels.size > 60)
        }
    }
}
