package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-893: the phone Work screen's pure rules. Test names mirror web
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
    fun `offers the other faces as switcher targets`() {
        assertEquals(
            listOf(SwitcherTarget.Face(WorkFaceKind.Issue), SwitcherTarget.Face(WorkFaceKind.Changes)),
            switcherTargets(
                listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Changes),
                WorkFaceKind.Run,
                listOf("a"),
                "a",
                false,
            ),
        )
        assertEquals(
            emptyList<SwitcherTarget>(),
            switcherTargets(listOf(WorkFaceKind.Issue), WorkFaceKind.Issue, emptyList(), null, false),
        )
    }

    @Test
    fun `expands the run face into one row per run with two or more`() {
        assertEquals(
            listOf(SwitcherTarget.Run("a"), SwitcherTarget.Run("b")),
            switcherTargets(listOf(WorkFaceKind.Issue, WorkFaceKind.Run), WorkFaceKind.Issue, listOf("a", "b"), "a", false),
        )
        // On the Run face the shown run is not a target.
        assertEquals(
            listOf(
                SwitcherTarget.Face(WorkFaceKind.Issue),
                SwitcherTarget.Run("b"),
                SwitcherTarget.Face(WorkFaceKind.Changes),
            ),
            switcherTargets(
                listOf(WorkFaceKind.Issue, WorkFaceKind.Run, WorkFaceKind.Changes),
                WorkFaceKind.Run,
                listOf("a", "b"),
                "a",
                false,
            ),
        )
    }

    @Test
    fun `prepends start coding when the shown run ended for good`() {
        assertEquals(
            listOf(SwitcherTarget.StartCoding, SwitcherTarget.Face(WorkFaceKind.Issue)),
            switcherTargets(listOf(WorkFaceKind.Issue, WorkFaceKind.Run), WorkFaceKind.Run, listOf("a"), "a", true),
        )
    }

    @Test
    fun `hides, toggles or opens a menu by target count`() {
        assertEquals(SwitcherMode.Hidden, switcherMode(emptyList()))
        assertEquals(
            SwitcherMode.Toggle(SwitcherTarget.Face(WorkFaceKind.Run)),
            switcherMode(listOf(SwitcherTarget.Face(WorkFaceKind.Run))),
        )
        assertTrue(
            switcherMode(
                listOf(SwitcherTarget.Face(WorkFaceKind.Issue), SwitcherTarget.Face(WorkFaceKind.Changes)),
            ) is SwitcherMode.Menu,
        )
    }

    @Test
    fun `badges the circle with the session tone off the run face`() {
        assertEquals(
            SwitcherBadge.Session(SessionDotTone.Running),
            switcherBadge(WorkFaceKind.Issue, SessionDotTone.Running, true),
        )
        assertEquals(
            SwitcherBadge.Session(SessionDotTone.NeedsInput),
            switcherBadge(WorkFaceKind.Changes, SessionDotTone.NeedsInput, false),
        )
        assertNull(switcherBadge(WorkFaceKind.Issue, null, true))
    }

    @Test
    fun `badges the circle with changes on the run face`() {
        assertEquals(SwitcherBadge.Changes, switcherBadge(WorkFaceKind.Run, SessionDotTone.Running, true))
        assertNull(switcherBadge(WorkFaceKind.Run, SessionDotTone.Running, false))
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
}
