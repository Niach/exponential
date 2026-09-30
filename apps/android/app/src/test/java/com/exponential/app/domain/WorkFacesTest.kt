package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-893: the phone Work screen's pure rules — EXP-1150: face tabs, the
// body swipe (`swipeTarget`) and the Run bar's one circle (`runBarTrailing`).
// Test names mirror web
// `work-faces.test.ts` and iOS `WorkFacesTests.swift`.
class WorkFacesTest {

    private val nowMs = WireTimestamps.parseEpochMs("2026-09-15T12:00:00Z")!!

    private fun run(
        id: String,
        issueId: String? = "issue-1",
        userId: String = "me",
        status: String = "running",
        startedAt: String = "2026-09-15T11:00:00Z",
        updatedAt: String = "2026-09-15T11:59:00Z",
    ) = CodingSessionEntity(
        id = id,
        issueId = issueId,
        teamId = "team-1",
        userId = userId,
        status = status,
        startedAt = startedAt,
        createdAt = startedAt,
        updatedAt = updatedAt,
    )

    @Test
    fun `lists the available faces in issue, run, changes, results order`() {
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Changes, WorkFaceKind.Results),
            availableFaces(hasIssue = true, hasRun = true, hasChanges = true, hasResults = true),
        )
        assertEquals(
            listOf(WorkFaceKind.Run),
            availableFaces(hasIssue = false, hasRun = true, hasChanges = false, hasResults = false),
        )
        // Changes is independent of Run: an open PR with no run of mine.
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Changes),
            availableFaces(hasIssue = true, hasRun = false, hasChanges = true, hasResults = false),
        )
        // EXP-879: Results comes LAST, and only ever beside a Run of mine.
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Results),
            availableFaces(hasIssue = true, hasRun = true, hasChanges = false, hasResults = true),
        )
    }

    @Test
    fun `labels the faces, Runs once there are several`() {
        assertEquals("Issue", faceLabel(WorkFaceKind.Issue))
        assertEquals("Run", faceLabel(WorkFaceKind.Run))
        assertEquals("Runs", faceLabel(WorkFaceKind.Run, multipleRuns = true))
        assertEquals("Changes", faceLabel(WorkFaceKind.Changes))
        assertEquals("Results", faceLabel(WorkFaceKind.Results))
        assertEquals("Type / for commands", STEER_COMPOSER_PLACEHOLDER)
        assertEquals("Plan mode", PLAN_MODE_LABEL)
    }

    @Test
    fun `targets the bound run when it is mine and live`() {
        val rows = listOf(
            run("a", startedAt = "2026-09-15T10:00:00Z"),
            run("b", startedAt = "2026-09-15T11:00:00Z"),
        )
        assertEquals("a", codingTarget(rows, "issue-1", "a", "me", nowMs)?.id)
    }

    @Test
    fun `targets the newest live own run, else the newest own run`() {
        val rows = listOf(
            run("old-live", startedAt = "2026-09-15T09:00:00Z"),
            run("new-live", startedAt = "2026-09-15T11:00:00Z"),
            run("newest-ended", status = "ended", startedAt = "2026-09-15T11:30:00Z"),
            run("theirs", userId = "them", startedAt = "2026-09-15T11:45:00Z"),
        )
        assertEquals("new-live", codingTarget(rows, "issue-1", null, "me", nowMs)?.id)
        // A bound run that ENDED does not win over a live one.
        assertEquals("new-live", codingTarget(rows, "issue-1", "newest-ended", "me", nowMs)?.id)
        val ended = rows.filter { it.status == "ended" }
        assertEquals("newest-ended", codingTarget(ended, "issue-1", null, "me", nowMs)?.id)
        assertNull(codingTarget(rows, "issue-2", null, "me", nowMs))
        assertNull(codingTarget(rows, "issue-1", null, null, nowMs))
    }

    @Test
    fun `treats a stale running row as not live`() {
        val rows = listOf(
            run("stale", startedAt = "2026-09-15T11:00:00Z", updatedAt = "2026-09-15T08:00:00Z"),
            run("ended", status = "ended", startedAt = "2026-09-15T10:00:00Z"),
        )
        assertFalse(isSessionLive(rows[0], nowMs))
        // No live run: the newest own run is the stale one — still the target.
        assertEquals("stale", codingTarget(rows, "issue-1", null, "me", nowMs)?.id)
        assertEquals("stale", codingTarget(rows, "issue-1", "stale", "me", nowMs)?.id)
    }

    @Test
    fun `picks the primary action, stop first`() {
        assertEquals(PrimaryAction.Stop, primaryAction(ownLive = true, ownEndedResumable = true, canStart = true))
        assertEquals(PrimaryAction.Resume, primaryAction(ownLive = false, ownEndedResumable = true, canStart = true))
        assertEquals(PrimaryAction.Start, primaryAction(ownLive = false, ownEndedResumable = false, canStart = true))
        assertEquals(PrimaryAction.None, primaryAction(ownLive = false, ownEndedResumable = false, canStart = false))
    }

    @Test
    fun `derives the coding action off the target row`() {
        val live = run("live")
        val ended = run("ended", status = "ended")
        assertEquals(PrimaryAction.Stop, codingAction(live, "me", nowMs, resumable = true, canStart = true))
        assertEquals(PrimaryAction.Resume, codingAction(ended, "me", nowMs, resumable = true, canStart = true))
        assertEquals(PrimaryAction.Start, codingAction(ended, "me", nowMs, resumable = false, canStart = true))
        // A teammate's row is never mine to stop or resume.
        assertEquals(PrimaryAction.Start, codingAction(live, "them", nowMs, resumable = true, canStart = true))
        assertEquals(PrimaryAction.None, codingAction(null, "me", nowMs, resumable = false, canStart = false))
    }

    @Test
    fun `swipes to the neighbouring face`() {
        val all = listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Changes, WorkFaceKind.Results)
        assertEquals(WorkFaceKind.Run, swipeTarget(all, WorkFaceKind.Issue, SwipeDirection.Left))
        assertEquals(WorkFaceKind.Changes, swipeTarget(all, WorkFaceKind.Run, SwipeDirection.Left))
        assertEquals(WorkFaceKind.Run, swipeTarget(all, WorkFaceKind.Changes, SwipeDirection.Right))
        assertEquals(WorkFaceKind.Issue, swipeTarget(all, WorkFaceKind.Run, SwipeDirection.Right))
        assertNull(swipeTarget(all, WorkFaceKind.Issue, SwipeDirection.Right))
        assertNull(swipeTarget(all, WorkFaceKind.Results, SwipeDirection.Left))
        assertEquals(
            WorkFaceKind.Results,
            swipeTarget(listOf(WorkFaceKind.Issue, WorkFaceKind.Results), WorkFaceKind.Issue, SwipeDirection.Left),
        )
        assertNull(swipeTarget(listOf(WorkFaceKind.Issue), WorkFaceKind.Run, SwipeDirection.Left))
        assertNull(swipeTarget(emptyList(), WorkFaceKind.Issue, SwipeDirection.Left))
    }

    @Test
    fun `picks the run bar's trailing circle, merge first`() {
        assertEquals(RunBarTrailing.Merge, runBarTrailing(canMerge = true, offerStart = true))
        assertEquals(RunBarTrailing.Start, runBarTrailing(canMerge = false, offerStart = true))
        assertEquals(RunBarTrailing.None, runBarTrailing(canMerge = false, offerStart = false))
    }

    @Test
    fun `falls back changes to run to issue`() {
        assertEquals(WorkFaceKind.Run, fallbackFace(WorkFaceKind.Changes, listOf(WorkFaceKind.Issue, WorkFaceKind.Run)))
        assertEquals(WorkFaceKind.Issue, fallbackFace(WorkFaceKind.Changes, listOf(WorkFaceKind.Issue)))
        // EXP-879: results falls the same way — run first, then the issue.
        assertEquals(WorkFaceKind.Run, fallbackFace(WorkFaceKind.Results, listOf(WorkFaceKind.Issue, WorkFaceKind.Run)))
        assertEquals(WorkFaceKind.Issue, fallbackFace(WorkFaceKind.Results, listOf(WorkFaceKind.Issue)))
        assertEquals(
            WorkFaceKind.Results,
            fallbackFace(WorkFaceKind.Results, listOf(WorkFaceKind.Issue, WorkFaceKind.Results)),
        )
        assertEquals(WorkFaceKind.Issue, fallbackFace(WorkFaceKind.Run, listOf(WorkFaceKind.Issue)))
        assertEquals(WorkFaceKind.Run, fallbackFace(WorkFaceKind.Run, listOf(WorkFaceKind.Issue, WorkFaceKind.Run)))
        assertEquals(WorkFaceKind.Run, fallbackFace(WorkFaceKind.Issue, listOf(WorkFaceKind.Run)))
        assertNull(fallbackFace(WorkFaceKind.Run, emptyList()))
    }

    @Test
    fun `reads the session model off the config option`() {
        assertNull(sessionModel(null))
        assertNull(sessionModel(SessionConfigState(options = emptyList())))
        assertNull(sessionModel(SessionConfigState(options = listOf(ConfigOption(id = "model", label = "Model", value = "")))))
        assertEquals(
            "opus",
            sessionModel(SessionConfigState(options = listOf(ConfigOption(id = "model", label = "Model", value = "opus")))),
        )
    }

    @Test
    fun `tones the state dot off the phase`() {
        assertEquals(
            PhaseDot(SessionDotTone.Running, connecting = false),
            phaseDotTone(live = true, connecting = false, awaitingInput = false, paused = false, stale = false),
        )
        assertEquals(
            SessionDotTone.NeedsInput,
            phaseDotTone(live = true, connecting = false, awaitingInput = true, paused = false, stale = false).tone,
        )
        assertEquals(
            SessionDotTone.NeedsInput,
            phaseDotTone(live = true, connecting = false, awaitingInput = false, paused = false, stale = true).tone,
        )
        assertEquals(
            SessionDotTone.Muted,
            phaseDotTone(live = true, connecting = false, awaitingInput = false, paused = true, stale = false).tone,
        )
        assertEquals(
            SessionDotTone.Muted,
            phaseDotTone(live = false, connecting = false, awaitingInput = false, paused = false, stale = false).tone,
        )
        assertTrue(
            phaseDotTone(live = false, connecting = true, awaitingInput = false, paused = false, stale = false).connecting,
        )
        assertFalse(
            phaseDotTone(live = false, connecting = true, awaitingInput = false, paused = true, stale = false).connecting,
        )
    }

    // EXP-935 — an account switch (or a Resume) ends the live run a moment
    // before its successor syncs; the screen must WAIT for it instead of
    // popping to the list.
    @Test
    fun `an issueless run pops back only once nothing is landing in its place`() {
        // The plain end of a chat / batch run: back to the list.
        assertTrue(shouldAutoBack(ended = true, wasLive = true, issueId = null, continuationPending = false))
        // A switch / resume in flight holds the screen on the run.
        assertFalse(shouldAutoBack(ended = true, wasLive = true, issueId = null, continuationPending = true))
        // An issue-bound subject never pops — the pill flips to Resume.
        assertFalse(shouldAutoBack(ended = true, wasLive = true, issueId = "i", continuationPending = false))
        // Opened straight onto an ended run: no live edge, no pop.
        assertFalse(shouldAutoBack(ended = true, wasLive = false, issueId = null, continuationPending = false))
        // Still running.
        assertFalse(shouldAutoBack(ended = false, wasLive = true, issueId = null, continuationPending = false))
    }

    // EXP-934 — mirrored by web `work-faces.test.ts` and iOS `WorkFacesTests`.
    @Test
    fun `shows the context menu on the issue face alone`() {
        assertTrue(faceShowsContextMenu(WorkFaceKind.Issue))
        assertFalse(faceShowsContextMenu(WorkFaceKind.Run))
        assertFalse(faceShowsContextMenu(WorkFaceKind.Changes))
        assertFalse(faceShowsContextMenu(WorkFaceKind.Results))
    }

    // EXP-933: `issueResultsRun` against the shared `session-results.json`.
    @Test
    fun `every fixture issueResultsRun case picks the expected run`() {
        val section = sessionResultsFixture()["issueResultsRun"]!!.jsonObject
        val now = WireTimestamps.parseEpochMs(section["now"]!!.jsonPrimitive.content)!!
        val cases = section["cases"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        fun str(obj: JsonObject, key: String): String? =
            obj[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
        for (element in cases) {
            val case = element.jsonObject
            val rows = case["rows"]!!.jsonArray.map { rowElement ->
                val row = rowElement.jsonObject
                CodingSessionEntity(
                    id = str(row, "id")!!,
                    issueId = str(row, "issueId"),
                    teamId = "team-1",
                    userId = str(row, "userId")!!,
                    status = str(row, "status")!!,
                    startedAt = str(row, "startedAt")!!,
                    createdAt = str(row, "startedAt")!!,
                    updatedAt = str(row, "updatedAt")!!,
                    results = row["results"]?.toString(),
                )
            }
            assertEquals(
                str(case, "name"),
                str(case, "expected"),
                issueResultsRun(rows, str(case, "issueId")!!, str(case, "boundId"), str(case, "me"), now)?.id,
            )
        }
    }
}
