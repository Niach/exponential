package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1876: the tail's outline = style.json's numbers (a 100 × 40 box, radius 16). */
class BubbleTailTest {
    private val corners = listOf(16f, 16f, 16f, 16f)

    private fun near(want: Float, got: Float, what: String) = assertEquals(what, want, got, 0.002f)

    @Test
    fun startTailFromTheSpec() {
        val s = bubbleTailOutline(100f, 40f, corners, BubbleTail.Left)
        val m = s.first() as OutlineStep.Move
        near(-4f, m.x, "starts 2 in from the tip"); near(0f, m.y, "on the top edge")
        // The top edge runs straight to the top-right corner (top-left square).
        assertEquals(OutlineStep.Line(84f, 0f), s[1])
        val quad = s[s.size - 2] as OutlineStep.Quad
        near(-6f, quad.cx, "tip x"); near(0f, quad.cy, "tip y")
        near(-4f, quad.x, "tip end x"); near(0f, quad.y, "tip end y")
        val toTip = s[s.size - 3] as OutlineStep.Line
        near(-4.891f, toTip.x, "2 back along the hypotenuse x"); near(1.664f, toTip.y, "2 back along the hypotenuse y")
        val side = s[s.size - 4] as OutlineStep.Line
        near(0f, side.x, "side point x"); near(9f, side.y, "side point y")
        assertEquals(OutlineStep.Close, s.last())
    }

    @Test
    fun endTailMirrors() {
        val start = bubbleTailOutline(100f, 40f, corners, BubbleTail.Left)
        val end = bubbleTailOutline(100f, 40f, corners, BubbleTail.Right)
        val q = end[end.size - 2] as OutlineStep.Quad
        near(106f, q.cx, "tip at W + 6"); near(104f, q.x, "W + 4")
        assertEquals(start.size, end.size)
    }

    @Test
    fun downTailFromTheSpec() {
        val s = bubbleTailOutline(100f, 40f, corners, BubbleTail.Down)
        val q = s.filterIsInstance<OutlineStep.Quad>().single()
        near(78f, q.cx, "tip x = W - 22"); near(49f, q.cy, "tip y = H + 9")
        near(78f - 1.109f, q.x, "rounded tip x"); near(49f - 1.664f, q.y, "rounded tip y")
        val i = s.indexOf(q)
        assertEquals(OutlineStep.Line(78f, 47f), s[i - 1])
        assertEquals(OutlineStep.Line(72f, 40f), s[i + 1])
        // Every corner keeps its radius: four arcs.
        assertEquals(4, s.count { it is OutlineStep.Cubic })
        val left = bubbleTailOutline(100f, 40f, corners, BubbleTail.DownLeft).filterIsInstance<OutlineStep.Quad>().single()
        near(22f, left.cx, "RTL mirrors")
    }

    @Test
    fun insetStaysInside() {
        val s = bubbleTailOutline(100f, 40f, corners, BubbleTail.Left, inset = 0.5f)
        val q = s[s.size - 2] as OutlineStep.Quad
        assertTrue(q.cx > -6f && q.cy == 0.5f)
        val side = s[s.size - 4] as OutlineStep.Line
        assertEquals(0.5f, side.x)
        assertTrue(side.y < 9f)
    }

    @Test
    fun logicalSides() {
        assertEquals(BubbleTail.Left, BubbleTail.of("start", false))
        assertEquals(BubbleTail.Right, BubbleTail.of("start", true))
        assertEquals(BubbleTail.Right, BubbleTail.of("end", false))
        assertEquals(BubbleTail.DownLeft, BubbleTail.of("down", true))
        assertNull(BubbleTail.of("none", false))
        assertEquals(listOf(0f, 16f, 16f, 16f), BubbleTailGeometry.corners(corners, BubbleTail.Left))
        assertEquals(corners, BubbleTailGeometry.corners(corners, BubbleTail.Down))
    }
}
