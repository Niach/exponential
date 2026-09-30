package com.exponential.app.ui.components

import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ToastKind
import com.exponential.app.domain.ToastStack
import com.exponential.app.ui.theme.DesignTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1031: the host/card numbers are the contract's, never written down twice. */
class ToastDefaultsTest {
    @Test
    fun hostNumbersPinToTheContract() {
        assertEquals(16.dp, ToastDefaults.HorizontalInset)
        assertEquals(ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp, ToastDefaults.HorizontalInset)
        assertEquals(356.dp, ToastDefaults.Width)
        assertEquals(ToastStack.Constants.WIDTH.dp, ToastDefaults.Width)
        assertEquals(45.dp, ToastDefaults.SwipeThreshold)
        assertEquals(ToastStack.Constants.SWIPE_THRESHOLD.dp, ToastDefaults.SwipeThreshold)
    }

    @Test
    fun cardIsTheLgRadiusRung() {
        assertEquals(DesignTokens.Radius.Lg, ToastDefaults.CornerRadius)
        assertEquals(12.dp, ToastDefaults.CornerRadius)
    }

    @Test
    fun kindColourIsTheSemanticToken() {
        assertEquals(DesignTokens.Semantic.Green, ToastDefaults.color(ToastKind.Success))
        assertEquals(DesignTokens.Semantic.Red, ToastDefaults.color(ToastKind.Error))
        assertEquals(DesignTokens.Semantic.Blue, ToastDefaults.color(ToastKind.Info))
        assertEquals(DesignTokens.Semantic.Yellow, ToastDefaults.color(ToastKind.Warning))
    }

    @Test
    fun toasterStacksNewestLastAndDismisses() {
        val toaster = Toaster()
        val a = toaster.info("One")
        val b = toaster.error("Two", "detail")
        assertNotEquals(a, b)
        assertEquals(listOf("One", "Two"), toaster.toasts.map { it.title })
        toaster.expanded = true
        toaster.dismiss(a)
        toaster.dismiss(b)
        assertEquals(0, toaster.toasts.size)
        assertEquals(false, toaster.expanded)
    }

    @Test
    fun everyShowBumpsTheGenerationThatReRaisesTheWindow() {
        val toaster = Toaster()
        assertEquals(0, toaster.generation)
        toaster.info("One")
        assertEquals(1, toaster.generation)
        toaster.error("Two")
        toaster.success("Three")
        assertEquals(3, toaster.generation)
        // Dismissing never re-raises; raise() does, but only while drawn.
        toaster.dismiss(toaster.toasts.first().id)
        assertEquals(3, toaster.generation)
        toaster.raise()
        assertEquals(4, toaster.generation)
        Toaster().apply { raise() }.let { assertEquals(0, it.generation) }
    }

    @Test
    fun windowRecreationKeepsItemsAndReplaysNoEnter() {
        var now = 1_000L
        val toaster = Toaster(clock = { now })
        val a = toaster.info("One")
        toaster.heights[a] = 60.0
        toaster.expanded = true
        toaster.remainingMs[a] = 1_234L
        // The first window: the fresh toast enters.
        assertTrue(toaster.isEntering(a))
        val first = toaster.visibilityOf(a)
        assertFalse(first.currentState)

        now += ToastDefaults.ENTER_MS + 1
        val b = toaster.error("Two")
        // The re-created window (a new generation) finds everything in Toaster.
        assertEquals(listOf(a, b), toaster.rendered.map { it.id })
        assertEquals(60.0, toaster.heights[a])
        assertEquals(true, toaster.expanded)
        assertEquals(1_234L, toaster.remainingMs[a])
        assertFalse(toaster.isEntering(a))
        assertTrue(toaster.isEntering(b))
        // A state lost with the old window is rebuilt already shown for the old toast.
        toaster.visibility.clear()
        assertTrue(toaster.visibilityOf(a).currentState)
        assertFalse(toaster.visibilityOf(b).currentState)
    }

    @Test
    fun anExitedToastLeavesTheRenderedListOnlyWhenForgotten() {
        val toaster = Toaster()
        val a = toaster.info("One")
        toaster.dismiss(a)
        assertEquals(0, toaster.toasts.size)
        assertEquals(listOf(a), toaster.rendered.map { it.id })
        toaster.forget(a)
        assertEquals(0, toaster.rendered.size)
    }

    @Test
    fun cardWidthFillsPhonesAndPinsTablets() {
        assertEquals(328.dp, ToastDefaults.cardWidth(328.dp))
        assertEquals(ToastDefaults.Width, ToastDefaults.cardWidth(800.dp))
        assertEquals(DesignTokens.Motion.Duration.Standard.toLong(), ToastDefaults.ENTER_MS)
    }
}
