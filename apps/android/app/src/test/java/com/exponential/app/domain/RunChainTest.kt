package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-974: the resume chain (`domain/RunChain.kt`). The same six cases, by
// name, as web `run-chain.test.ts`, desktop `queries::run_chain` tests and
// iOS `RunChainTests`.
class RunChainTest {
    private fun row(id: String, resumedFromId: String?, createdAt: String) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = "user-1",
        resumedFromId = resumedFromId,
        startedAt = createdAt,
        createdAt = createdAt,
        updatedAt = createdAt,
    )

    private fun ids(rows: List<CodingSessionEntity>) = rows.map { it.id }

    private val a = row("a", null, "2026-09-01T10:00:00Z")
    private val b = row("b", "a", "2026-09-01T11:00:00Z")
    private val c = row("c", "b", "2026-09-01T12:00:00Z")

    // An unrelated run of the same person, interleaved in time.
    private val x = row("x", null, "2026-09-01T11:30:00Z")

    @Test
    fun `returns just the session when nothing resumed it and it resumed nothing`() {
        assertEquals(listOf("x"), ids(runChain(listOf(a, x), "x")))
    }

    @Test
    fun `walks resumedFromId backwards to the first row`() {
        assertEquals(listOf("a", "b", "c"), ids(runChain(listOf(c, x, a, b), "c")))
    }

    @Test
    fun `walks forwards to the latest resume from a middle or first row`() {
        assertEquals(listOf("a", "b", "c"), ids(runChain(listOf(c, x, a, b), "a")))
        assertEquals(listOf("a", "b", "c"), ids(runChain(listOf(c, x, a, b), "b")))
    }

    @Test
    fun `follows the newest successor at a fork`() {
        val c1 = row("c1", "b", "2026-09-01T12:00:00Z")
        val c2 = row("c2", "b", "2026-09-01T13:00:00Z")
        val d2 = row("d2", "c2", "2026-09-01T14:00:00Z")
        val rows = listOf(a, b, c1, c2, d2)
        // From the root or the fork point: the newest branch, to its end.
        assertEquals(listOf("a", "b", "c2", "d2"), ids(runChain(rows, "a")))
        assertEquals(listOf("a", "b", "c2", "d2"), ids(runChain(rows, "b")))
        // From the older sibling: its own past plus itself — never the other
        // branch.
        assertEquals(listOf("a", "b", "c1"), ids(runChain(rows, "c1")))
        // Same stamp: the id breaks the tie, so every client picks the same row.
        val c3 = row("c3", "b", "2026-09-01T13:00:00Z")
        assertEquals(listOf("a", "b", "c3"), ids(runChain(listOf(a, b, c2, c3), "b")))
    }

    @Test
    fun `yields empty for an unknown id`() {
        assertEquals(emptyList<String>(), ids(runChain(listOf(a, b, c), "nope")))
        assertEquals(emptyList<String>(), ids(runChain(emptyList(), "a")))
    }

    @Test
    fun `tolerates a predecessor the sweep deleted (dangling resumedFromId)`() {
        // `b` resumed `a`, but `a` is gone: the chain starts at `b`.
        assertEquals(listOf("b", "c"), ids(runChain(listOf(b, c), "c")))
        assertEquals(listOf("b", "c"), ids(runChain(listOf(b, c), "b")))
    }

    @Test
    fun `never loops on a cyclic resumedFromId`() {
        val p = row("p", "q", "2026-09-01T10:00:00Z")
        val q = row("q", "p", "2026-09-01T11:00:00Z")
        assertEquals(listOf("q", "p"), ids(runChain(listOf(p, q), "p")))
        assertEquals(listOf("p", "q"), ids(runChain(listOf(p, q), "q")))
    }
}
