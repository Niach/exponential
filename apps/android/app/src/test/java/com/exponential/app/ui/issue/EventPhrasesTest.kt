package com.exponential.app.ui.issue

import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.ui.icons.ExpIcons
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-169: activity rows must render the payload detail (status from/to,
// assignee, label names, PR numbers) and degrade to the bare verb when the
// payload or a lookup row is missing.
class EventPhrasesTest {

    private fun event(type: String, payload: String?) = IssueEventEntity(
        id = "evt-1",
        issueId = "issue-1",
        teamId = "ws-1",
        actorUserId = "actor-1",
        type = type,
        payload = payload,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    // eventPhrase's maps are deliberately non-defaulted in production; the
    // empty-map default lives HERE, test-only.
    private fun phrase(
        event: IssueEventEntity,
        users: Map<String, UserEntity> = emptyMap(),
        labels: Map<String, LabelEntity> = emptyMap(),
    ) = eventPhrase(event, users, labels)

    private val dana = UserEntity(
        id = "user-dana",
        name = "Dana",
        email = "dana@example.com",
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    private val bugLabel = LabelEntity(
        id = "label-bug",
        teamId = "ws-1",
        name = "bug",
        color = "#ff0000",
        sortOrder = 1.0,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    @Test
    fun statusChangedWithFromAndTo() {
        assertEquals(
            "changed the status from Backlog to In review",
            phrase(event("status_changed", """{"from":"backlog","to":"in_review"}""")),
        )
    }

    @Test
    fun statusChangedWithOnlyTo() {
        assertEquals("changed the status to Done", phrase(event("status_changed", """{"to":"done"}""")))
    }

    @Test
    fun statusChangedUnknownWireValueStaysVerbatim() {
        // An unknown status from a newer server must NOT mislabel as Backlog.
        assertEquals(
            "changed the status from Backlog to triaged new",
            phrase(event("status_changed", """{"from":"backlog","to":"triaged_new"}""")),
        )
    }

    /** EXP-314: the status ROWS' names win over the legacy enum anchors. */
    @Test
    fun statusChangedPrefersTheStatusRowNames() {
        assertEquals(
            "changed the status from Triage to Shipping",
            phrase(
                event(
                    "status_changed",
                    """{"from":"backlog","to":"in_progress","fromName":"Triage","toName":"Shipping"}""",
                )
            ),
        )
        // Only a toName: the from side still falls back to the anchor label.
        assertEquals(
            "changed the status from Backlog to Shipping",
            phrase(event("status_changed", """{"from":"backlog","to":"in_progress","toName":"Shipping"}""")),
        )
    }

    @Test
    fun statusChangedWithoutPayloadFallsBack() {
        assertEquals("changed the status", phrase(event("status_changed", null)))
    }

    @Test
    fun assigneeAssignedResolvesName() {
        assertEquals(
            "assigned Dana",
            phrase(
                event("assignee_changed", """{"from":null,"to":"user-dana"}"""),
                users = mapOf(dana.id to dana),
            ),
        )
    }

    @Test
    fun assigneeAssignedUnknownUserStillReads() {
        assertTrue(phrase(event("assignee_changed", """{"to":"user-gone"}""")).startsWith("assigned "))
    }

    @Test
    fun assigneeClearedReadsUnassigned() {
        assertEquals(
            "unassigned this issue",
            phrase(event("assignee_changed", """{"from":"user-dana","to":null}""")),
        )
    }

    @Test
    fun labelAddedResolvesName() {
        assertEquals(
            "added label bug",
            phrase(
                event("label_added", """{"labelId":"label-bug"}"""),
                labels = mapOf(bugLabel.id to bugLabel),
            ),
        )
    }

    @Test
    fun labelRemovedWithoutRowFallsBack() {
        assertEquals("removed a label", phrase(event("label_removed", """{"labelId":"label-gone"}""")))
    }

    @Test
    fun prEventsUsePayloadNumber() {
        assertEquals(
            "opened PR #91",
            phrase(event("pr_opened", """{"prUrl":"https://x","prNumber":91,"branch":"exp/EXP-1"}""")),
        )
        assertEquals(
            "merged PR #91",
            phrase(event("pr_merged", """{"prUrl":"https://x","prNumber":91}""")),
        )
    }

    @Test
    fun prEventsWithoutNumberFallBack() {
        assertEquals("opened a pull request", phrase(event("pr_opened", """{"prUrl":null}""")))
        assertEquals("merged the pull request", phrase(event("pr_merged", null)))
    }

    @Test
    fun boardMovedKeepsIdentifierDetail() {
        assertEquals(
            "moved this to another board (EXP-4 → SUP-9)",
            phrase(event("board_moved", """{"fromIdentifier":"EXP-4","toIdentifier":"SUP-9"}""")),
        )
    }

    @Test
    fun unknownTypeSpacesUnderscores() {
        assertEquals("something new", phrase(event("something_new", null)))
    }

    /** EXP-530: priority wire values render capitalized (web parity). */
    @Test
    fun priorityChangedCapitalizesWireValues() {
        assertEquals(
            "changed priority from Urgent to Low",
            phrase(event("priority_changed", """{"from":"urgent","to":"low"}""")),
        )
    }

    @Test
    fun priorityChangedMissingSidesReadNone() {
        assertEquals(
            "changed priority from None to High",
            phrase(event("priority_changed", """{"to":"high"}""")),
        )
        assertEquals(
            "changed priority from None to None",
            phrase(event("priority_changed", null)),
        )
    }

    /** EXP-530: `created` rows never render a timeline row — the issue header
     * already shows creation. Every other kind stays visible. */
    @Test
    fun createdEventsAreSuppressedEntirely() {
        assertFalse(eventRowVisible("created"))
        assertTrue(eventRowVisible("status_changed"))
        assertTrue(eventRowVisible("priority_changed"))
        assertTrue(eventRowVisible("something_new"))
    }

    // ── EXP-736: relation events ────────────────────────────────────────────

    @Test
    fun relationAddedReadsTheSideSpecificPhrase() {
        assertEquals(
            "marked as blocked by EXP-3",
            phrase(
                event(
                    "relation_added",
                    """{"type":"blocks","relatedIdentifier":"EXP-3","direction":"inverse"}""",
                ),
            ),
        )
        assertEquals(
            "no longer parent of EXP-3",
            phrase(
                event(
                    "relation_removed",
                    """{"type":"parent","relatedIdentifier":"EXP-3","direction":"forward"}""",
                ),
            ),
        )
    }

    @Test
    fun relatedReferencesReadAsAnAddedRelatedIssue() {
        assertEquals(
            "added related issue EXP-12",
            phrase(
                event(
                    "relation_added",
                    """{"type":"related","relatedIdentifier":"EXP-12","direction":"forward","source":"reference"}""",
                ),
            ),
        )
    }

    // EXP-736: a thin relation payload still phrases — the web
    // `relationEventPhrase` degrades, it never falls back to the bare verb:
    // an unknown/missing type reads as the symmetric `related`, a missing
    // counterpart is named "an issue".
    @Test
    fun relationEventsWithoutPayloadDegradeLikeTheWeb() {
        assertEquals("added related issue an issue", phrase(event("relation_added", null)))
        assertEquals("removed related issue an issue", phrase(event("relation_removed", null)))
        assertEquals(
            "marked as blocked by an issue",
            phrase(event("relation_added", """{"type":"blocks","direction":"inverse"}""")),
        )
        assertEquals(
            "added related issue EXP-3",
            phrase(
                event(
                    "relation_added",
                    """{"type":"mentioned","relatedIdentifier":"EXP-3","direction":"inverse"}""",
                ),
            ),
        )
    }

    // ── EXP-595: timeline glyphs (web `EventRow` / desktop `EventGlyph` parity) ──

    @Test
    fun glyphsMapEventTypesToTheSharedConceptIcons() {
        fun plain(type: String) =
            (eventGlyph(event(type, null), emptyList()) as EventGlyph.Plain).icon
        assertSame(ExpIcons.eventAssigneeChanged, plain("assignee_changed"))
        assertSame(ExpIcons.eventLabelAdded, plain("label_added"))
        assertSame(ExpIcons.eventLabelAdded, plain("label_removed"))
        assertSame(ExpIcons.eventBoardMoved, plain("board_moved"))
        assertSame(ExpIcons.prOpen, plain("pr_opened"))
        assertSame(ExpIcons.prMerged, plain("pr_merged"))
        assertSame(ExpIcons.eventPriorityChanged, plain("priority_changed"))
        assertSame(ExpIcons.eventRelationAdded, plain("relation_added"))
        assertSame(ExpIcons.eventRelationRemoved, plain("relation_removed"))
    }

    /** EXP-525 parity: the TARGET status's real row wins over the anchor. */
    @Test
    fun statusGlyphResolvesTheTargetRow() {
        val shipping = ResolvedIssueStatus(
            id = "row-1",
            rowId = "row-1",
            name = "Shipping",
            category = IssueStatusCategory.Started,
            colorHex = "#00ff00",
            builtinKey = null,
            iconName = "progress-2-4",
        )
        val glyph = eventGlyph(
            event("status_changed", """{"to":"in_progress","toStatusId":"row-1"}"""),
            listOf(shipping),
        ) as EventGlyph.Status
        assertEquals("row-1", glyph.status.id)
    }

    @Test
    fun statusGlyphDegradesToTheConstructedDefault() {
        // No team rows synced yet: the constructed builtin for the anchor.
        val glyph =
            eventGlyph(event("status_changed", """{"to":"done"}"""), emptyList()) as EventGlyph.Status
        assertEquals("builtin:done", glyph.status.id)
    }

    @Test
    fun unknownEventTypesKeepThePlainDot() {
        assertNull(eventGlyph(event("something_new", null), emptyList()))
    }
}
