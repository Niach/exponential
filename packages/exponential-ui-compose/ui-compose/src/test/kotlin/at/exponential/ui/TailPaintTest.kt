package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.asAndroidPath
import at.exponential.ui.compose.physicalTail
import at.exponential.ui.compose.tailOutline
import at.exponential.ui.compose.withTailCorner
import at.exponential.ui.primitives.BubbleTail
import at.exponential.ui.theme.PaintStyle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * #EXP-1876: a bubble tail is ONE outline with its box (style.json): a
 * side tail 6 out / 9 down at the top corner of its side (that corner
 * square), a `down` tail 9 down, 6 wide, its tip 22 in from the end edge;
 * outside the frame, mirrored under RTL; the model carries `tail` from the
 * core's visual.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class TailPaintTest {
    private val box = Size(100f, 40f)
    private val style = PaintStyle(background = androidx.compose.ui.graphics.Color.White, radius = 16f)

    /** The outline filled into a bitmap with a 20 px margin: [inside] at frame coordinates (dp = px here). */
    private fun filled(side: String): (Float, Float) -> Boolean {
        val s = style.copy(tail = side).withTailCorner(rtl = false)
        val path = tailOutline(s, box, 1f, physicalTail(side, false)!!).asAndroidPath()
        val bmp = Bitmap.createBitmap(140, 80, Bitmap.Config.ARGB_8888)
        Canvas(bmp).apply {
            translate(20f, 20f)
            drawPath(path, Paint().apply { color = android.graphics.Color.BLACK; isAntiAlias = false })
        }
        return { x, y -> android.graphics.Color.alpha(bmp.getPixel((x + 20f).toInt(), (y + 20f).toInt())) > 0 }
    }

    @Test
    fun startTailSticksOutAtTheTopLeft() {
        val inside = filled("start")
        assertTrue("tail just inside its tip", inside(-3.5f, 0.5f))
        assertTrue("tail near the side point", inside(-0.5f, 7f))
        assertFalse("past the tip", inside(-7f, 0.5f))
        assertFalse("below the side point", inside(-1f, 12f))
        assertFalse("above the frame", inside(-3f, -1f))
        assertTrue("the top-left corner is square", inside(0.5f, 0.5f))
        assertFalse("the top-right corner stays round", inside(99f, 0.5f))
        assertFalse("nothing at the right", inside(101f, 1f))
    }

    @Test
    fun endTailMirrorsStart() {
        val inside = filled("end")
        assertTrue(inside(103.5f, 0.5f))
        assertFalse(inside(107f, 0.5f))
        assertTrue("the top-right corner is square", inside(99.5f, 0.5f))
        assertFalse("the top-left corner stays round", inside(0.5f, 0.5f))
        assertFalse(inside(-3f, 0.5f))
    }

    @Test
    fun downTailHangsBelowNearTheEnd() {
        val inside = filled("down")
        // The tip at (100 - 22, 40 + 9), the hypotenuse back up to x = 72.
        assertTrue(inside(77.5f, 46f))
        assertTrue(inside(74f, 40.5f))
        assertFalse("right of the vertical edge", inside(79f, 42f))
        assertFalse("left of the hypotenuse", inside(71f, 41f))
        assertFalse("below the tip", inside(77.5f, 50f))
        assertFalse("every corner keeps its radius", inside(0.5f, 0.5f))
        assertFalse(inside(99.5f, 0.5f))
    }

    @Test
    fun rtlMirrorsTheSides() {
        assertEquals(BubbleTail.Right, physicalTail("start", true))
        assertEquals(BubbleTail.Left, physicalTail("end", true))
        assertEquals(BubbleTail.DownLeft, physicalTail("down", true))
        assertNull(physicalTail(null, false))
        assertNull(physicalTail("none", false))
        assertEquals(listOf(16f, 0f, 16f, 16f), style.copy(tail = "start").withTailCorner(rtl = true).corners)
        assertEquals(listOf(0f, 16f, 16f, 16f), style.copy(tail = "start").withTailCorner(rtl = false).corners)
        assertEquals(style.corners, style.copy(tail = "down").withTailCorner(rtl = false).corners)
    }

    @Test
    fun insetOutlineStaysInsideTheFill() {
        val s = style.copy(tail = "start").withTailCorner(rtl = false)
        val outer = tailOutline(s, box, 2f, BubbleTail.Left).getBounds()
        val inner = tailOutline(s, box, 2f, BubbleTail.Left, inset = 1f).getBounds()
        // The tip at -6 dp (x2), rounded: bounds between the curve and its control point.
        assertTrue("tip at ${outer.left}", outer.left >= -12.01f && outer.left <= -10.4f)
        assertEquals(0f, outer.top, 0.01f)
        assertEquals(200f, outer.right, 0.01f)
        assertEquals(80f, outer.bottom, 0.01f)
        assertTrue(inner.left > outer.left && inner.top > outer.top && inner.right < outer.right && inner.bottom < outer.bottom)
    }

    @Test
    fun theBubblePartCarriesTheTail() {
        Fixtures.require()
        for (t in listOf("start", "end", "down")) {
            val m = makeModel("tail", fixed = true)
            m.setNested("""{"id":"mb","component":"MessageBubble","props":{"text":"Hi","tail":"$t"}}""")
            val bubble = m.nodes.first { it.id == "mb.bubble" }
            assertEquals(t, m.style(bubble.index).tail)
            // Paint-only: the frame is the bubble's own, the tail adds nothing to layout.
            assertTrue(m.frame(bubble.index).left >= 0f)
        }
        val m = makeModel("tail", fixed = true)
        m.setNested("""{"id":"mb","component":"MessageBubble","props":{"text":"Hi"}}""")
        assertNull(m.style(m.nodes.first { it.id == "mb.bubble" }.index).tail)
    }
}
