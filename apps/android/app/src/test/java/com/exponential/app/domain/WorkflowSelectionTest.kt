package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1087: the workflow page's chip selection — All at 0, DAG order after. */
class WorkflowSelectionTest {

    private fun chip(id: String) = NodeChip(
        id = id,
        title = id,
        display = WorkflowNodeDisplayState.QUEUED,
        caption = "Queued",
        stacked = false,
        members = 0,
        live = false,
        needsYou = false,
    )

    private val strip = listOf(
        StripWave(0, listOf(chip("c"))),
        StripWave(1, listOf(chip("a"), chip("b"))),
        StripWave(2, listOf(chip("d"))),
    )
    private val order = WorkflowSelection.order(strip)

    @Test
    fun `the order is waves then lanes`() {
        assertEquals(listOf("c", "a", "b", "d"), order)
    }

    @Test
    fun `a fresh selection is All`() {
        val selection = WorkflowSelection()
        assertTrue(selection.isAll)
        assertNull(selection.nodeId)
        assertEquals(0, selection.position(order))
    }

    @Test
    fun `select picks one node and null goes back to All`() {
        val picked = WorkflowSelection().select("b")
        assertEquals("b", picked.nodeId)
        assertFalse(picked.isAll)
        assertEquals(3, picked.position(order))
        assertTrue(picked.select(null).isAll)
    }

    @Test
    fun `tapping the selected chip again goes back to All`() {
        val picked = WorkflowSelection().toggle("a")
        assertEquals("a", picked.nodeId)
        assertTrue(picked.toggle("a").isAll)
        assertEquals("d", picked.toggle("d").nodeId)
    }

    @Test
    fun `stepping walks the DAG order with All at zero and clamps`() {
        var selection = WorkflowSelection()
        selection = selection.step(1, order)
        assertEquals("c", selection.nodeId)
        selection = selection.step(2, order)
        assertEquals("b", selection.nodeId)
        selection = selection.step(10, order)
        assertEquals("d", selection.nodeId)
        selection = selection.step(-1, order)
        assertEquals("b", selection.nodeId)
        selection = selection.step(-10, order)
        assertTrue(selection.isAll)
        assertTrue(WorkflowSelection().step(-1, order).isAll)
        assertTrue(WorkflowSelection().step(1, emptyList()).isAll)
    }

    @Test
    fun `a node that left the workflow reads as All`() {
        val gone = WorkflowSelection("zz")
        assertEquals(0, gone.position(order))
        assertEquals("c", gone.step(1, order).nodeId)
        assertTrue(gone.reconcile(order).isAll)
        assertEquals("a", WorkflowSelection("a").reconcile(order).nodeId)
    }
}
