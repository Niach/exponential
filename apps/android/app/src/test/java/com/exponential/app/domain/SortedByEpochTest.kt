package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

/** Large chats: [sortedByEpoch] parses once per element and orders exactly
 *  like the `compareBy({ ms }, { tie })` comparator it replaces. */
class SortedByEpochTest {
    private data class Row(val id: String, val at: String)

    private fun ms(iso: String): Long = WireTimestamps.parseEpochMs(iso) ?: 0L

    private val rows = listOf(
        Row("b", "2026-11-01T00:00:02.000Z"),
        Row("a", "2026-11-01T00:00:02.000Z"),
        Row("c", ""),
        Row("d", "2026-11-01T00:00:01.000Z"),
        Row("e", "2026-11-01T00:00:03.000Z"),
    )

    @Test
    fun `ascending matches compareBy`() {
        var parses = 0
        val sorted = rows.sortedByEpoch(tie = compareBy { it.id }) { parses++; ms(it.at) }
        assertEquals(rows.sortedWith(compareBy<Row>({ ms(it.at) }, { it.id })), sorted)
        assertEquals(rows.size, parses)
    }

    @Test
    fun `descending matches compareByDescending then the tie`() {
        val sorted = rows.sortedByEpoch(descending = true, tie = compareBy { it.id }) { ms(it.at) }
        assertEquals(rows.sortedWith(compareByDescending<Row> { ms(it.at) }.thenBy { it.id }), sorted)
    }
}
