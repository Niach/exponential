package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MarkOpsTest {

    @Test fun addMarkOverPlainRange() {
        val marks = MarkOps.addMark(emptyList(), 2, 5, InlineKind.Bold)
        assertEquals(1, marks.size)
        assertEquals(InlineMark(2, 5, InlineKind.Bold), marks.first())
    }

    @Test fun addMergesAdjacentSameKind() {
        val base = listOf(InlineMark(0, 3, InlineKind.Bold))
        val merged = MarkOps.addMark(base, 3, 6, InlineKind.Bold)
        assertEquals(1, merged.size)
        assertEquals(InlineMark(0, 6, InlineKind.Bold), merged.first())
    }

    @Test fun removeSplitsMarkAroundMiddle() {
        val base = listOf(InlineMark(0, 10, InlineKind.Bold))
        val result = MarkOps.removeMark(base, 3, 6, InlineKind.Bold).sortedBy { it.start }
        assertEquals(listOf(InlineMark(0, 3, InlineKind.Bold), InlineMark(6, 10, InlineKind.Bold)), result)
    }

    @Test fun hasMarkOverFullyCovered() {
        val base = listOf(InlineMark(0, 10, InlineKind.Italic))
        assertTrue(MarkOps.hasMarkOver(base, 2, 8, InlineKind.Italic))
        assertFalse(MarkOps.hasMarkOver(base, 2, 8, InlineKind.Bold))
    }

    @Test fun hasMarkOverPartialIsFalse() {
        val base = listOf(InlineMark(0, 5, InlineKind.Bold))
        assertFalse(MarkOps.hasMarkOver(base, 2, 8, InlineKind.Bold))
    }

    @Test fun sliceToLocalCoordinates() {
        val base = listOf(InlineMark(5, 10, InlineKind.Bold))
        val local = MarkOps.slice(base, 4, 12)
        assertEquals(listOf(InlineMark(1, 6, InlineKind.Bold)), local)
    }

    @Test fun offsetShiftsAll() {
        val base = listOf(InlineMark(0, 3, InlineKind.Bold))
        assertEquals(listOf(InlineMark(5, 8, InlineKind.Bold)), MarkOps.offset(base, 5))
    }
}
