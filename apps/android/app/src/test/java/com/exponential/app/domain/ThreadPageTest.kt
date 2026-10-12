package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.ThreadEntity
import com.exponential.app.data.db.ThreadItemEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1284: the phone thread page's own rules around the fixture-locked
 * model (web `thread-page.ts` twins): the reply box's target, the fresh
 * child ring, the thread's PR, the Guide a reader shows and the bundles the
 * lists partition.
 */
class ThreadPageTest {

    // 2026-10-12T11:00:00Z
    private val nowMs = 1_791_802_800_000L

    private fun run(
        id: String,
        userId: String = "u1",
        status: String = "running",
        startedAt: String = "2026-10-12T10:00:00Z",
        updatedAt: String = "2026-10-12T10:59:00Z",
        prUrl: String? = null,
        prState: String? = null,
        endedBy: String? = null,
        actionName: String? = "Chat",
        agentTitle: String? = null,
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = userId,
        status = status,
        endedBy = endedBy,
        prUrl = prUrl,
        prState = prState,
        actionName = actionName,
        agentTitle = agentTitle,
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = updatedAt,
    )

    private fun issue(id: String, prUrl: String? = null) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = id.uppercase(),
        title = "Issue $id",
        status = "backlog",
        priority = "none",
        sortOrder = 1.0,
        prUrl = prUrl,
        createdAt = "2026-10-12T09:00:00Z",
        updatedAt = "2026-10-12T09:00:00Z",
    )

    private fun modelRun(id: String, userId: String, status: String, startedAt: String) = ThreadView.Run(
        id = id, name = id, userId = userId, deviceLabel = null, status = status, needsInput = false,
        agentBusy = false, issueId = null, parentSessionId = null, resumedFromId = null,
        startedAt = startedAt, endedAt = null,
    )

    @Test
    fun `the reply sends to my live run, resumes my latest, else starts`() {
        val mineLive = modelRun("a", "me", "running", "2026-10-12T10:00:00Z")
        val theirsNewer = modelRun("b", "them", "running", "2026-10-12T10:05:00Z")
        assertEquals(ThreadPage.ReplyTarget.Send("a"), ThreadPage.replyTarget(listOf(mineLive, theirsNewer), "me", true))

        val mineEnded = modelRun("c", "me", "ended", "2026-10-12T10:10:00Z")
        assertEquals(ThreadPage.ReplyTarget.Resume("c"), ThreadPage.replyTarget(listOf(theirsNewer, mineEnded), "me", true))

        assertEquals(ThreadPage.ReplyTarget.Start, ThreadPage.replyTarget(listOf(mineEnded, theirsNewer.copy(startedAt = "2026-10-12T10:20:00Z")), "me", true))
        assertEquals(ThreadPage.ReplyTarget.Start, ThreadPage.replyTarget(emptyList(), "me", true))
        assertNull(ThreadPage.replyTarget(listOf(mineLive), "me", canSteer = false))
        assertNull(ThreadPage.replyTarget(listOf(mineLive), null, canSteer = true))
    }

    @Test
    fun `a child is fresh within ten seconds of its creation`() {
        val created = "2026-10-12T11:00:00Z"
        assertTrue(ThreadPage.isFreshChild(created, nowMs + 9_000))
        assertFalse(ThreadPage.isFreshChild(created, nowMs + 11_000))
        assertFalse(ThreadPage.isFreshChild(created, nowMs - 5_000))
        assertFalse(ThreadPage.isFreshChild(null, nowMs))
    }

    @Test
    fun `the thread's PR is the newest open one on its sessions and issues, else the newest merged`() {
        val openOld = run("r1", startedAt = "2026-10-12T09:00:00Z", updatedAt = "2026-10-12T09:30:00Z", prUrl = "https://x/pull/1", prState = "open")
        val mergedNew = run("r2", startedAt = "2026-10-12T10:00:00Z", updatedAt = "2026-10-12T10:30:00Z", prUrl = "https://x/pull/2", prState = "merged")
        // T8: a newer MERGED PR never hides an open one.
        assertEquals("https://x/pull/1", ThreadPage.prUrl(listOf(mergedNew, openOld), emptyList()))
        assertEquals("https://x/pull/2", ThreadPage.prUrl(listOf(mergedNew), emptyList()))
        // Issues' PRs are candidates too.
        val issueOpen = issue("a", "https://x/pull/7").copy(prState = "open", updatedAt = "2026-10-12T11:00:00Z")
        assertEquals("https://x/pull/7", ThreadPage.prUrl(listOf(mergedNew), listOf(issueOpen)))
        // Closed only = none.
        assertNull(ThreadPage.prUrl(listOf(run("r3", prUrl = "https://x/pull/3", prState = "closed")), emptyList()))
    }

    @Test
    fun `a session block shows only its own Guide entries`() {
        val thread = ThreadEntity(
            id = "t1",
            results = """[{"topic":"Summary","text":"legacy"},{"topic":"Summary","text":"one","sessionId":"r1"},{"topic":"Summary","text":"two","sessionId":"r2"}]""",
        )
        val first = ThreadPage.sessionGuide(run("r1"), thread, firstSessionId = "r1")
        assertTrue(first!!.contains("\"one\"") && !first.contains("\"two\""))
        val second = ThreadPage.sessionGuide(run("r2"), thread, firstSessionId = "r1")!!
        assertTrue(second.contains("\"two\"") && !second.contains("legacy"))
        // No thread Guide yet: the session's own mirror.
        assertNull(ThreadPage.sessionGuide(run("r3"), null, firstSessionId = "r1"))
    }

    @Test
    fun `my live sessions no thread names list as orphans`() {
        val items = listOf(ThreadItemEntity(id = "i1", threadId = "t1", kind = "run", sessionId = "placed"))
        val orphans = ThreadPage.orphanLiveSessions(
            listOf(
                run("placed", userId = "me"),
                run("orphan", userId = "me"),
                run("theirs", userId = "them"),
                run("dead", userId = "me", updatedAt = "2026-10-12T05:00:00Z"),
            ),
            items,
            "me",
            nowMs,
        )
        assertEquals(listOf("orphan"), orphans.map { it.id })
    }

    @Test
    fun `a reader shows the thread's Guide once it has one`() {
        assertEquals("[2]", ThreadPage.guideResults("[1]", "[2]"))
        assertEquals("[1]", ThreadPage.guideResults("[1]", null))
        assertNull(ThreadPage.guideResults(null, null))
    }

    @Test
    fun `a heartbeat-dead run reads ended, a sweep end stays live`() {
        assertEquals("running", ThreadPage.runStatus(run("r"), nowMs))
        assertEquals("ended", ThreadPage.runStatus(run("r", updatedAt = "2026-10-12T05:00:00Z"), nowMs))
        assertEquals(
            "running",
            ThreadPage.runStatus(run("r", status = "ended", endedBy = DomainContract.codingSessionEndedByStale), nowMs),
        )
        assertEquals("ended", ThreadPage.runStatus(run("r", status = "ended", endedBy = "user"), nowMs))
    }

    @Test
    fun `bundles name runs by identity and lists split mine-live from the rest`() {
        val threads = listOf(
            ThreadEntity(id = "t1", teamId = "team-1", updatedAt = "2026-10-12T10:00:00Z"),
            ThreadEntity(id = "t2", teamId = "team-1", updatedAt = "2026-10-12T10:30:00Z"),
            ThreadEntity(id = "t3", teamId = "team-1", updatedAt = "2026-10-12T10:40:00Z"),
        )
        val items = listOf(
            ThreadItemEntity(id = "i1", threadId = "t1", kind = "run", sessionId = "r1", sortOrder = 1.0),
            ThreadItemEntity(id = "i2", threadId = "t2", kind = "run", sessionId = "r2", sortOrder = 1.0),
            ThreadItemEntity(id = "i3", threadId = "t2", kind = "issue", issueId = "a", sortOrder = 2.0),
        )
        val sessions = listOf(
            run("r1", userId = "me", agentTitle = "Fix the picker"),
            run("r2", userId = "me", status = "ended"),
        )
        val bundles = ThreadPage.bundles(threads, items, sessions, listOf(issue("a")), emptyList(), "me", nowMs)
        val t1 = bundles.first { it.thread.id == "t1" }
        assertEquals("Fix the picker", t1.view.title)
        assertTrue(t1.mineLive)
        assertEquals("r1", t1.myLiveRun?.id)
        // A chat still named Chat takes its first issue's title.
        assertEquals("Issue a", bundles.first { it.thread.id == "t2" }.view.title)

        val list = ThreadPage.list(bundles)
        assertEquals(listOf("t1"), list.running.map { it.thread.id })
        // t3 holds nothing it can show.
        assertEquals(listOf("t2"), list.recent.map { it.thread.id })
    }
}
