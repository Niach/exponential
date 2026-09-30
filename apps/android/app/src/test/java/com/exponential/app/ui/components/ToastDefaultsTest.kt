package com.exponential.app.ui.components

import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ToastKind
import com.exponential.app.domain.ToastStack
import com.exponential.app.ui.theme.DesignTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
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
    fun hostIsTopAnchoredOnTouch() {
        assertEquals("top-center", ToastDefaults.PLACEMENT)
        assertEquals(ToastStack.Constants.PLACEMENT_TOUCH, ToastDefaults.PLACEMENT)
        assertEquals(Alignment.TopCenter, ToastDefaults.HostAlignment)
        assertFalse(ToastDefaults.ANCHORED_BOTTOM)
        assertEquals(TransformOrigin(0.5f, 1f), ToastDefaults.BackCardOrigin)
        assertEquals(ToastStack.Constants.MOBILE_VIEWPORT_OFFSET.dp, ToastDefaults.TopGap)
        assertEquals(ToastStack.Constants.TOUCH_MAX_WIDTH.dp, ToastDefaults.TabletBreakpoint)
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
    }
}
