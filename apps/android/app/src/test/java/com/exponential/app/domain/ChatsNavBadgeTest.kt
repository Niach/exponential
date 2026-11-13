package com.exponential.app.domain

import java.time.Instant
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #PERF-9 / #FEED-232: the Chats tab's count badge, derived without building
 * the list, says exactly what `NavBadges.chatsBadge` says over
 * [SessionTree.partitionChats], over randomized trees that cross Recent's
 * cut, the 24 h window, archive marks and read marks.
 */
class ChatsNavBadgeTest {
    private val now = Instant.parse("2026-11-12T12:00:00Z")

    private fun iso(minutesAgo: Long) = now.minusSeconds(minutesAgo * 60).toString()

    @Test
    fun theBadgeMatchesTheList() {
        var mutedSeen = 0
        var attentionSeen = 0
        repeat(400) { round ->
            val rnd = Random(round)
            val sessions = mutableListOf<SessionTree.Session>()
            val messages = mutableListOf<SessionTree.Message>()
            val reads = mutableListOf<SessionTree.SessionRead>()
            val roots = rnd.nextInt(0, 40)
            for (r in 0 until roots) {
                val rootId = "r$r"
                val team = if (rnd.nextInt(10) == 0) "other" else "team"
                val children = rnd.nextInt(0, 3)
                for (c in 0..children) {
                    val id = if (c == 0) rootId else "$rootId-c$c"
                    // Mostly idle and ended trees (Recent), some running / needing you.
                    val status = when (rnd.nextInt(6)) {
                        0 -> DomainContract.sessionStatusQueued
                        1, 2 -> DomainContract.sessionStatusEnded
                        else -> DomainContract.sessionStatusRunning
                    }
                    sessions += SessionTree.Session(
                        id = id,
                        teamId = team,
                        parentId = if (c == 0) null else rootId,
                        rootId = rootId,
                        userId = "u",
                        status = status,
                        busy = rnd.nextInt(8) == 0,
                        needsInput = rnd.nextInt(15) == 0,
                        blocked = null,
                        shared = true,
                        title = null,
                        guide = null,
                        startedAt = null,
                        endedAt = null,
                        createdAt = iso(5_000),
                        updatedAt = iso(rnd.nextLong(0, 3_000)),
                        agentId = if (c == 0 && rnd.nextInt(12) == 0) "agent" else null,
                    )
                    if (rnd.nextBoolean()) {
                        messages += SessionTree.Message(
                            id = "m-$id",
                            sessionId = id,
                            authorKind = if (rnd.nextInt(3) == 0) "user" else DomainContract.messageAuthorKindAgent,
                            authorId = null,
                            body = "hi",
                            card = null,
                            answeredAt = null,
                            consumedAt = null,
                            createdAt = iso(rnd.nextLong(0, 3_000)),
                        )
                    }
                }
                if (rnd.nextBoolean()) {
                    reads += SessionTree.SessionRead(
                        sessionId = rootId,
                        readAt = iso(rnd.nextLong(0, 3_000)),
                        archivedAt = if (rnd.nextInt(4) == 0) iso(1) else null,
                    )
                }
            }
            val team = if (round % 5 == 0) null else "team"
            val groups = SessionTree.partitionChats(sessions, emptyList(), emptyList(), messages, team, reads, now.toString())
            val expected = NavBadges.chatsBadge(groups, reads)
            if (expected.count > 1 && expected.tone == NavBadges.Tone.Muted) mutedSeen++
            if (expected.count > 1 && expected.tone == NavBadges.Tone.Attention) attentionSeen++
            assertEquals("round $round", expected, SessionTree.chatsNavBadge(sessions, messages, team, reads, now.toString()))
        }
        assertTrue("the rounds exercise a muted count", mutedSeen > 20)
        assertTrue("the rounds exercise an attention count", attentionSeen > 20)
    }
}
