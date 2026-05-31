package com.exponential.app.ui.markdown

import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import org.junit.Assert.assertEquals
import org.junit.Test

/** Inline marks must follow the characters they cover as the text is edited. */
class MarkRemapTest {

    @Test fun insertBeforeMarkShiftsItRight() {
        val marks = listOf(InlineMark(0, 3, InlineKind.Bold)) // "abc" in "abc"
        val out = MarkRemap.remap("abc", "XXabc", marks)
        assertEquals(listOf(InlineMark(2, 5, InlineKind.Bold)), out)
    }

    @Test fun insertAfterMarkLeavesItAlone() {
        val marks = listOf(InlineMark(0, 3, InlineKind.Bold))
        val out = MarkRemap.remap("abc", "abcXX", marks)
        assertEquals(listOf(InlineMark(0, 3, InlineKind.Bold)), out)
    }

    @Test fun deleteBeforeMarkShiftsItLeft() {
        val marks = listOf(InlineMark(3, 6, InlineKind.Bold)) // "def" in "abcdef"
        val out = MarkRemap.remap("abcdef", "def", marks)
        assertEquals(listOf(InlineMark(0, 3, InlineKind.Bold)), out)
    }

    @Test fun typingInsideMarkExtendsIt() {
        val marks = listOf(InlineMark(0, 4, InlineKind.Bold)) // "abcd"
        val out = MarkRemap.remap("abcd", "abXcd", marks)
        assertEquals(listOf(InlineMark(0, 5, InlineKind.Bold)), out)
    }

    @Test fun emptyMarksUnchanged() {
        assertEquals(emptyList<InlineMark>(), MarkRemap.remap("a", "ab", emptyList()))
    }

    @Test fun identicalTextUnchanged() {
        val marks = listOf(InlineMark(0, 1, InlineKind.Italic))
        assertEquals(marks, MarkRemap.remap("a", "a", marks))
    }
}
