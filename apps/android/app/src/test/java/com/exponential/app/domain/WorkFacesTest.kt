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

// EXP-893: the phone Work screen's pure rules — EXP-1150: face tabs and the
// body swipe (`swipeTarget`).
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
    fun `lists the available faces in issue, run, guide order`() {
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Guide),
            availableFaces(hasIssue = true, hasRun = true, hasResults = true, hasDiff = true),
        )
        assertEquals(
            listOf(WorkFaceKind.Run),
            availableFaces(hasIssue = false, hasRun = true, hasResults = false, hasDiff = false),
        )
        // The Guide is independent of Run: an open PR with no run of mine.
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Guide),
            availableFaces(hasIssue = true, hasRun = false, hasResults = false, hasDiff = true),
        )
        // Results alone are enough for a Guide.
        assertEquals(
            listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Guide),
            availableFaces(hasIssue = true, hasRun = true, hasResults = true, hasDiff = false),
        )
    }

    @Test
    fun `labels the faces, Runs once there are several`() {
        assertEquals("Issue", faceLabel(WorkFaceKind.Issue))
        assertEquals("Run", faceLabel(WorkFaceKind.Run))
        assertEquals("Runs", faceLabel(WorkFaceKind.Run, multipleRuns = true))
        assertEquals(DomainContract.diffUiGuideFace, faceLabel(WorkFaceKind.Guide))
        assertEquals("Guide", GUIDE_FACE_LABEL)
        assertEquals("Open Guide", OPEN_RESULTS_LABEL)
        assertEquals("Type / for commands", STEER_COMPOSER_PLACEHOLDER)
        assertEquals("Plan mode", PLAN_MODE_LABEL)
    }

    // EXP-1251: a Stack card swap holds the Guide until the member's row loads.
    @Test
    fun `a swapped subject holds the Guide until its issue row loads`() {
        assertTrue(holdSwappedGuide(WorkFaceKind.Issue, WorkFaceKind.Guide, swappedRowLoaded = false))
        assertFalse(holdSwappedGuide(WorkFaceKind.Issue, WorkFaceKind.Guide, swappedRowLoaded = true))
        assertFalse(holdSwappedGuide(WorkFaceKind.Guide, WorkFaceKind.Guide, swappedRowLoaded = false))
        assertFalse(holdSwappedGuide(WorkFaceKind.Issue, WorkFaceKind.Run, swappedRowLoaded = false))
    }

    // EXP-1251: web "the Guide URL" (legacy results / diff land on the Guide).
    @Test
    fun `the Guide URL maps the legacy faces onto the Guide`() {
        assertEquals(WorkFaceKind.Guide, workFaceFromParam("guide"))
        assertEquals(WorkFaceKind.Guide, workFaceFromParam("results"))
        assertEquals(WorkFaceKind.Guide, workFaceFromParam("Changes"))
        assertEquals(WorkFaceKind.Run, workFaceFromParam("run"))
        assertEquals(WorkFaceKind.Issue, workFaceFromParam("issue"))
        assertNull(workFaceFromParam("nope"))
        assertNull(workFaceFromParam(null))
    }

    // EXP-1251: web `lib/work-faces.test.ts` describe "guideSectionPage".
    @Test
    fun `guideSectionPage opens a numbered section, other changes, the complete diff and nothing stale`() {
        val groups = parseSessionResultGroups(
            """[{"topic":"Summary","text":"s","files":["a.ts"]},{"topic":"nav","text":"n","files":["b.ts"]},{"topic":"api","text":"x","files":["c.ts"]}]""",
        )
        val files = listOf(
            Diff.File(path = "a.ts", additions = 1, deletions = 0),
            Diff.File(path = "b.ts", additions = 5, deletions = 2),
            Diff.File(path = "c.ts", additions = 3, deletions = 1),
            Diff.File(path = "d.ts", additions = 2, deletions = 2),
        )
        val second = guideSectionPage(groups, files, GuideSectionKey.Numbered(2))!!
        assertEquals("02 / 02", second.caption)
        assertEquals("api", second.title)
        assertEquals(listOf("c.ts"), second.files.map { it.path })
        assertEquals("+3 \u22121 · 1 file", guideSectionSummary(second))
        val other = guideSectionPage(groups, files, GuideSectionKey.Other)!!
        assertEquals("Other changes", other.title)
        assertEquals(listOf("d.ts"), other.files.map { it.path })
        assertNull(other.caption)
        val lead = guideSectionPage(groups, files, GuideSectionKey.Lead)!!
        assertEquals("Summary", lead.title)
        val all = guideSectionPage(groups, files, GuideSectionKey.All)!!
        assertEquals("Changes", all.title)
        assertEquals(4, all.files.size)
        assertEquals(11, all.additions)
        assertNull(guideSectionPage(groups, files, GuideSectionKey.Numbered(3)))
        assertNull(guideSectionPage(groups, null, GuideSectionKey.All))
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
        val all = listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Guide)
        assertEquals(WorkFaceKind.Run, swipeTarget(all, WorkFaceKind.Issue, SwipeDirection.Left))
        assertEquals(WorkFaceKind.Guide, swipeTarget(all, WorkFaceKind.Run, SwipeDirection.Left))
        assertEquals(WorkFaceKind.Run, swipeTarget(all, WorkFaceKind.Guide, SwipeDirection.Right))
        assertEquals(WorkFaceKind.Issue, swipeTarget(all, WorkFaceKind.Run, SwipeDirection.Right))
        assertNull(swipeTarget(all, WorkFaceKind.Issue, SwipeDirection.Right))
        assertNull(swipeTarget(all, WorkFaceKind.Guide, SwipeDirection.Left))
        assertEquals(
            WorkFaceKind.Guide,
            swipeTarget(listOf(WorkFaceKind.Issue, WorkFaceKind.Guide), WorkFaceKind.Issue, SwipeDirection.Left),
        )
        assertNull(swipeTarget(listOf(WorkFaceKind.Issue), WorkFaceKind.Run, SwipeDirection.Left))
        assertNull(swipeTarget(emptyList(), WorkFaceKind.Issue, SwipeDirection.Left))
    }

    @Test
    fun `falls back guide to run to issue`() {
        assertEquals(WorkFaceKind.Run, fallbackFace(WorkFaceKind.Guide, listOf(WorkFaceKind.Issue, WorkFaceKind.Run)))
        assertEquals(WorkFaceKind.Issue, fallbackFace(WorkFaceKind.Guide, listOf(WorkFaceKind.Issue)))
        assertEquals(
            WorkFaceKind.Guide,
            fallbackFace(WorkFaceKind.Guide, listOf(WorkFaceKind.Issue, WorkFaceKind.Guide)),
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
        assertFalse(faceShowsContextMenu(WorkFaceKind.Guide))
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
                issueResultsRun(
                    rows,
                    str(case, "issueId")!!,
                    str(case, "boundId"),
                    str(case, "me"),
                    now,
                    str(case, "prUrl"),
                )?.id,
            )
        }
    }

    // ── EXP-1175: the Run face's status row (`run-row.json`, ×4) ─────────────
    private val runRowFixture = kotlinx.serialization.json.Json
        .parseToJsonElement(contractFixtureJson("run-row.json")).jsonObject

    @Test
    fun `labels the Show work switch off the fixture`() {
        assertEquals(runRowFixture["showWorkLabel"]!!.jsonPrimitive.content, showWorkLabel(false))
        assertEquals(runRowFixture["hideWorkLabel"]!!.jsonPrimitive.content, showWorkLabel(true))
        assertEquals(runRowFixture["showWorkDefault"]!!.jsonPrimitive.content.toBoolean(), SHOW_WORK_DEFAULT)
    }

    @Test
    fun `every fixture captions case reads the expected caption and tone`() {
        val cases = runRowFixture["captions"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        fun str(obj: JsonObject, key: String): String? =
            obj[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
        for (element in cases) {
            val case = element.jsonObject
            val name = str(case, "name")
            val state = RunRowState.entries.first { it.wire == str(case, "state") }
            val caption = runRowCaption(
                state = state,
                device = str(case, "device")!!,
                startedAt = str(case, "startedAt"),
                endedAt = str(case, "endedAt"),
                nowMs = WireTimestamps.parseEpochMs(str(case, "now")!!)!!,
            )
            val expected = case["expected"]!!.jsonObject
            assertEquals(name, str(expected, "text"), caption.text)
            assertEquals(name, str(expected, "tone"), caption.tone.wire)
        }
    }

    // EXP-1245: `run-row.json` `turnCaptions` (desktop
    // `turn_row_caption_matches_the_shared_fixture`).
    @Test
    fun `every fixture turnCaptions case reads the expected caption`() {
        val cases = runRowFixture["turnCaptions"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        fun str(obj: JsonObject, key: String): String? =
            obj[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
        for (element in cases) {
            val case = element.jsonObject
            val name = str(case, "name")
            val turn = case["turn"]!!.jsonObject
            val caption = turnRowCaption(
                startedAt = str(turn, "startedAt")?.let { WireTimestamps.parseEpochMs(it) },
                endedAt = str(turn, "endedAt")?.let { WireTimestamps.parseEpochMs(it) },
                state = RunRowState.entries.first { it.wire == str(case, "state") },
                device = str(case, "device")!!,
                runEndedAt = str(case, "runEndedAt"),
                nowMs = WireTimestamps.parseEpochMs(str(case, "now")!!)!!,
            )
            val expected = case["expected"]
            if (expected == null || expected is JsonNull) {
                assertNull(name, caption)
                continue
            }
            assertEquals(name, str(expected.jsonObject, "text"), caption?.text)
            assertEquals(name, str(expected.jsonObject, "tone"), caption?.tone?.wire)
        }
    }

    @Test
    fun `every fixture states case resolves the row state`() {
        val cases = runRowFixture["states"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        val displays = mapOf(
            "working" to CodingSessionDisplayState.Working,
            "needs_input" to CodingSessionDisplayState.NeedsInput,
            "review" to CodingSessionDisplayState.Review,
            "done" to CodingSessionDisplayState.Done,
        )
        for (element in cases) {
            val case = element.jsonObject
            fun flag(key: String) = case[key]!!.jsonPrimitive.content.toBoolean()
            val name = case["name"]!!.jsonPrimitive.content
            val state = runRowState(
                paused = flag("paused"),
                ended = flag("ended"),
                awaitingInput = flag("awaitingInput"),
                working = flag("working"),
                display = displays.getValue(case["display"]!!.jsonPrimitive.content),
            )
            assertEquals(name, case["expected"]!!.jsonPrimitive.content, state.wire)
        }
    }

    // EXP-1154 / web M8 (`prDescriptionGroups`): the PR body as ONE group.
    @Test
    fun `the PR body is one group with the fallback title and body`() {
        val blank = prDescriptionGroup(title = " ", body = "", files = null)
        assertEquals(PR_FALLBACK_TITLE, blank.topic)
        assertEquals(PR_FALLBACK_EMPTY_BODY, blank.text)
        assertTrue(blank.files.isEmpty())
    }

    @Test
    fun `the PR body group claims every diff path, so ONE Changes section and no Other changes`() {
        val files = listOf(
            Diff.File(path = "a.ts", additions = 1),
            Diff.File(path = "b.ts", additions = 2, deletions = 1),
        )
        val group = prDescriptionGroup(title = "Fix it", body = "Body.", files = files)
        assertEquals(listOf("a.ts", "b.ts"), group.files)
        val coverage = guideCoverage(listOf(group), files)
        assertNull(coverage.lead)
        assertEquals(1, coverage.sections.size)
        assertEquals(2, coverage.sections.single().changes?.fileCount)
        assertNull(coverage.other)
        assertEquals(2, coverage.complete?.fileCount)
        // Its Changes row opens the section page titled with the PR title.
        val page = guideSectionPage(listOf(group), files, GuideSectionKey.Numbered(1))!!
        assertEquals("Fix it", page.title)
        assertEquals(2, page.files.size)
        assertNull(guideSectionPage(listOf(group), files, GuideSectionKey.Other))
        assertEquals(2, guideSectionPage(listOf(group), files, GuideSectionKey.All)?.files?.size)
    }
}
