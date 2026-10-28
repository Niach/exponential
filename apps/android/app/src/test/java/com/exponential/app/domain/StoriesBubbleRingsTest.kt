package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

/** [Stories.bubbleRings] (the TS reference `bubbleRings`): a bubble wears
 *  its agent's first ring, every other ring follows in ring order. */
class StoriesBubbleRingsTest {

    private fun ring(key: String, agentId: String?) = Stories.StoryRing(
        key = key,
        agentId = agentId,
        rootId = if (agentId == null) key.removePrefix("root:") else null,
        title = key,
        assistant = false,
        waiting = 0,
        unseen = true,
        stories = emptyList(),
    )

    @Test
    fun bubblesWearTheirAgentsRingsAndTheRestFollowInOrder() {
        val board = ring("root:board", null)
        val triage = ring("agent:ag1", "ag1")
        val orphan = ring("agent:ag9", "ag9")
        val designer = ring("agent:ag2", "ag2")
        val ship = ring("root:ship", null)
        val split = Stories.bubbleRings(listOf("as", "ag1", "ag2", "ag3"), listOf(board, triage, orphan, designer, ship))
        assertEquals(mapOf("ag1" to triage, "ag2" to designer), split.byAgent)
        assertEquals(listOf(board, orphan, ship), split.rest)
    }

    @Test
    fun theFirstRingOfAnAgentWins() {
        val first = ring("agent:ag1", "ag1")
        val second = ring("agent:ag1-b", "ag1")
        val split = Stories.bubbleRings(listOf("ag1"), listOf(first, second))
        assertEquals(mapOf("ag1" to first), split.byAgent)
        assertEquals(listOf(second), split.rest)
    }

    @Test
    fun noBubblesLeavesEveryRingInTheRest() {
        val rings = listOf(ring("root:a", null), ring("agent:ag1", "ag1"))
        val split = Stories.bubbleRings(emptyList(), rings)
        assertEquals(emptyMap<String, Stories.StoryRing>(), split.byAgent)
        assertEquals(rings, split.rest)
    }
}
