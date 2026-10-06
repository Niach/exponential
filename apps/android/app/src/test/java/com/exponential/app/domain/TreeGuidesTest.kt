package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-965 — the nested lists' connector, the same four cases every client's
// tree-guide test runs.
class TreeGuidesTest {

    @Test
    fun `a root draws nothing`() {
        val guides = TreeGuides.compute(listOf(0, 0))
        assertTrue(guides.all { it.isEmpty })
        assertEquals(listOf(null, null), guides.map { it.elbowAt })
    }

    @Test
    fun `a chain hangs each row off the level above it`() {
        val guides = TreeGuides.compute(listOf(0, 1, 2))
        assertEquals(listOf(null, 0, 1), guides.map { it.elbowAt })
        // Nothing follows any of them: no tee, and no ancestor to carry on.
        assertTrue(guides.none { it.tee })
        assertTrue(guides.all { it.passThrough.isEmpty() })
    }

    @Test
    fun `siblings tee until the last one`() {
        val guides = TreeGuides.compute(listOf(0, 1, 1, 1))
        assertEquals(listOf(null, 0, 0, 0), guides.map { it.elbowAt })
        assertEquals(listOf(false, true, true, false), guides.map { it.tee })
    }

    @Test
    fun `a nested sibling keeps its ancestors line`() {
        //  root
        //  ├ a
        //  │ └ a1      ← the root's line must pass through
        //  └ b
        val guides = TreeGuides.compute(listOf(0, 1, 2, 1))
        assertEquals(listOf(null, 0, 1, 0), guides.map { it.elbowAt })
        assertEquals(listOf(false, true, false, false), guides.map { it.tee })
        assertEquals(listOf(0), guides[2].passThrough)
        // The LAST child of the last branch carries nothing through.
        assertTrue(guides[3].passThrough.isEmpty())
    }

    @Test
    fun `the last child stops at its elbow`() {
        //  root
        //  ├ a
        //  │ ├ a1
        //  │ └ a2
        //  └ b
        val guides = TreeGuides.compute(listOf(0, 1, 2, 2, 1))
        assertTrue(guides[2].tee)
        assertFalse(guides[3].tee)
        // Both of a's children still hang under the root's ongoing branch.
        assertEquals(listOf(0), guides[2].passThrough)
        assertEquals(listOf(0), guides[3].passThrough)
    }

    @Test
    fun `the gutters start at the rows own 12dp padding`() {
        // EXP-1208: the ×4 geometry (web `treeGuideCentre`): a depth-d row's
        // mark sits at 12 + 14·d, so its centre is 12 + 14·d + 7, and a
        // child's elbow stub ends exactly where the child's mark starts.
        assertEquals(19f, TreeGuides.centre(0))
        assertEquals(33f, TreeGuides.centre(1))
        assertEquals(26f, TreeGuides.stubEnd(0))
        assertEquals(40f, TreeGuides.stubEnd(1))
        // A container that nests from its own edge passes base 0.
        assertEquals(7f, TreeGuides.centre(0, base = 0f))
    }
}
