package at.exponential.ui.kitchensink

import android.content.Intent
import android.view.KeyEvent
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import at.exponential.ui.model.setOpen

/**
 * The hardware keyboard on the kitchen sink (round 1 §6): Tab walks the
 * controls in pre-order (`:focus-visible` from the keyboard), arrows rove
 * the Tabs and activate them, Escape closes the top layer.
 */
@RunWith(AndroidJUnit4::class)
class KeyboardTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()

    private fun key(code: Int, times: Int = 1) = repeat(times) {
        instrumentation.sendKeyDownUpSync(code)
        instrumentation.waitForIdleSync()
    }

    private fun waitFor(what: String, check: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 10_000
        while (!check() && System.currentTimeMillis() < deadline) Thread.sleep(100)
        assertTrue(what, check())
    }

    @Test
    fun tabArrowsAndEscape() {
        val context = instrumentation.targetContext
        val intent = Intent(context, MainActivity::class.java).putExtra("shot", "exponential-ui-kitchen-sink")
        ActivityScenario.launch<MainActivity>(intent).use {
            waitFor("the first pass") { (MainActivity.live?.passCount ?: 0) > 0 }
            val model = MainActivity.live!!
            key(KeyEvent.KEYCODE_TAB)
            waitFor("Tab focuses the first control (${model.keyboardFocus})") { model.keyboardFocus == "app-bar.back" }
            assertTrue("focus-visible from the keyboard", model.statesOf("app-bar.back").contains("focus-visible"))
            val walk = ArrayList<String?>()
            repeat(12) {
                key(KeyEvent.KEYCODE_TAB)
                Thread.sleep(150)
                walk.add(model.keyboardFocus)
            }
            android.util.Log.i("kbd-walk", walk.joinToString(" | "))
            // Pre-order: the AppBar's search, then the breadcrumb links, then the header's button.
            val order = listOf("app-bar-search", "breadcrumb.item.0.link", "breadcrumb.item.1.link", "hdr-scan")
            assertEquals("Tab walks in pre-order: $walk", order, walk.filter { it in order }.distinct().take(order.size))
            // Into the tabs BY TABBING (no seeded focus), then ArrowRight selects the next.
            var hops = 0
            while (model.keyboardFocus != "tabs.tab.0" && hops < 80) {
                key(KeyEvent.KEYCODE_TAB)
                hops += 1
            }
            waitFor("Tab reaches the first tab (${model.keyboardFocus} after $hops)") { model.keyboardFocus == "tabs.tab.0" }
            key(KeyEvent.KEYCODE_DPAD_RIGHT)
            waitFor("ArrowRight selects the next tab") { model.indexOf("tabs.tab.1")?.let(model::node)?.selected == true }
            assertEquals("tabs.tab.1", model.keyboardFocus)
            // The Escape KEY (the hardware path: the surface's key handler, or
            // the native sheet's window) closes the top layer.
            instrumentation.runOnMainSync { model.setOpen("drawer", true) }
            waitFor("the drawer opens") { model.layers.any { it.owner == "drawer" } }
            Thread.sleep(400)
            key(KeyEvent.KEYCODE_ESCAPE)
            waitFor("Escape closes it") { model.layers.none { it.owner == "drawer" } }
        }
    }
}
