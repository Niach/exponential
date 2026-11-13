package com.exponential.app.domain

import com.exponential.app.domain.WaitingCards.DeepLinkStep
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * [DeepLinkDriver] over a simulated transcript (waiting-cards.json DEEP
 * LINKS): a chat of [total] rows loading its newest [window], Load earlier =
 * one [stepRows] page that lands a few frames later, an off-screen row measured
 * only once scrolled to, a centring scroll that lands on the next layout.
 */
class DeepLinkDriverTest {

    private class Transcript(val total: Int, window: Int, val stepRows: Int, val more: () -> Boolean = { true }) {
        var loadedFrom = maxOf(0, total - window)
        /** Frames until a requested page lands (0 = none asked). */
        var landing = 0
        var visible = false
        var centred = false
        val canPage get() = loadedFrom > 0 && more()

        fun found(target: Int) = target in loadedFrom until total
    }

    /** Runs the driver frame by frame; returns the steps taken. */
    private fun drive(view: Transcript, target: Int, startMs: Long = 0, frameMs: Long = 16, maxFrames: Int = 2_000): List<DeepLinkStep> {
        val driver = DeepLinkDriver()
        driver.take("card", startMs)
        val steps = mutableListOf<DeepLinkStep>()
        var now = startMs
        repeat(maxFrames) {
            if (driver.link == null) return steps
            now += frameMs
            // A requested page lands three frames after it was asked.
            if (view.landing > 0 && --view.landing == 0) {
                view.loadedFrom = maxOf(0, view.loadedFrom - view.stepRows)
            }
            val found = view.found(target)
            val step = driver.step(
                found = found,
                laidOut = found && view.visible,
                centred = found && view.visible && view.centred,
                more = view.canPage,
                loadedMark = view.total - view.loadedFrom,
                nowMs = now,
            ) ?: return steps
            if (steps.lastOrNull() != step) steps += step
            when (step) {
                DeepLinkStep.Page -> view.landing = 3
                // Found but not measured: the host scrolls it into the layout.
                DeepLinkStep.Wait -> if (found) view.visible = true
                DeepLinkStep.Scroll -> view.centred = true
                else -> Unit
            }
        }
        return steps
    }

    @Test
    fun `a target outside the newest 200 rows pages one page at a time, waits for its layout, scrolls, rings`() {
        val view = Transcript(total = 600, window = 200, stepRows = 100)
        val steps = drive(view, target = 120)
        // Loaded from 400; 300 → 200 → 100: three pages, each waited for
        // (the last wait runs on into the found row's layout).
        assertEquals(
            listOf(
                DeepLinkStep.Page, DeepLinkStep.Wait, DeepLinkStep.Page, DeepLinkStep.Wait,
                DeepLinkStep.Page, DeepLinkStep.Wait,
                DeepLinkStep.Scroll, DeepLinkStep.Ring,
            ),
            steps,
        )
        assertEquals(100, view.loadedFrom)
    }

    @Test
    fun `a loaded target never pages`() {
        val view = Transcript(total = 600, window = 200, stepRows = 100)
        assertEquals(listOf(DeepLinkStep.Wait, DeepLinkStep.Scroll, DeepLinkStep.Ring), drive(view, target = 590))
    }

    @Test
    fun `a missing target pages to the start, then settles as missing`() {
        val view = Transcript(total = 250, window = 200, stepRows = 100)
        val steps = drive(view, target = 999)
        assertEquals(listOf(DeepLinkStep.Page, DeepLinkStep.Wait, DeepLinkStep.Missing), steps)
        assertEquals(0, view.loadedFrom)
    }

    @Test
    fun `a page that never lands stops counting as in flight, and the link gives up past deepLinkMs`() {
        val view = Transcript(total = 600, window = 200, stepRows = 0)
        val steps = drive(view, target = 10)
        assertEquals(DeepLinkStep.Missing, steps.last())
        assertEquals(true, steps.count { it == DeepLinkStep.Page } >= 1)
    }

    @Test
    fun `past deepLinkMs a found row rings where it stands`() {
        val driver = DeepLinkDriver()
        driver.take("card", 0)
        val step = driver.step(found = true, laidOut = false, centred = false, more = true, loadedMark = 1, nowMs = WaitingCards.Constants.DEEP_LINK_MS)
        assertEquals(DeepLinkStep.Ring, step)
        assertNull(driver.link)
    }

    @Test
    fun `the reader's own scroll drops the link`() {
        val driver = DeepLinkDriver()
        driver.take("card", 0)
        assertEquals(DeepLinkStep.Page, driver.step(false, false, false, true, 1, 16))
        driver.drop()
        assertNull(driver.step(false, false, false, true, 1, 32))
    }
}
