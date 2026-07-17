package com.exponential.app.ui.issue

import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-169: activity rows must render the payload detail (status from/to,
// assignee, label names, PR numbers) and degrade to the bare verb when the
// payload or a lookup row is missing.
class EventPhrasesTest {

    private fun event(type: String, payload: String?) = IssueEventEntity(
        id = "evt-1",
        issueId = "issue-1",
        workspaceId = "ws-1",
        actorUserId = "actor-1",
        type = type,
        payload = payload,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    private val dana = UserEntity(
        id = "user-dana",
        name = "Dana",
        email = "dana@example.com",
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    private val bugLabel = LabelEntity(
        id = "label-bug",
        workspaceId = "ws-1",
        name = "bug",
        color = "#ff0000",
        sortOrder = 1.0,
        createdAt = "2026-07-01 10:00:00+00",
        updatedAt = "2026-07-01 10:00:00+00",
    )

    @Test
    fun statusChangedWithFromAndTo() {
        val phrase = eventPhrase(
            event("status_changed", """{"from":"backlog","to":"in_review"}"""),
        )
        assertEquals("changed the status from Backlog to In review", phrase)
    }

    @Test
    fun statusChangedWithOnlyTo() {
        val phrase = eventPhrase(event("status_changed", """{"to":"done"}"""))
        assertEquals("changed the status to Done", phrase)
    }

    @Test
    fun statusChangedUnknownWireValueStaysVerbatim() {
        // An unknown status from a newer server must NOT mislabel as Backlog.
        val phrase = eventPhrase(
            event("status_changed", """{"from":"backlog","to":"triaged_new"}"""),
        )
        assertEquals("changed the status from Backlog to triaged new", phrase)
    }

    @Test
    fun statusChangedWithoutPayloadFallsBack() {
        assertEquals("changed the status", eventPhrase(event("status_changed", null)))
    }

    @Test
    fun assigneeAssignedResolvesName() {
        val phrase = eventPhrase(
            event("assignee_changed", """{"from":null,"to":"user-dana"}"""),
            usersById = mapOf(dana.id to dana),
        )
        assertEquals("assigned Dana", phrase)
    }

    @Test
    fun assigneeAssignedUnknownUserStillReads() {
        val phrase = eventPhrase(event("assignee_changed", """{"to":"user-gone"}"""))
        assertTrue(phrase.startsWith("assigned "))
    }

    @Test
    fun assigneeClearedReadsUnassigned() {
        val phrase = eventPhrase(
            event("assignee_changed", """{"from":"user-dana","to":null}"""),
        )
        assertEquals("unassigned this issue", phrase)
    }

    @Test
    fun labelAddedResolvesName() {
        val phrase = eventPhrase(
            event("label_added", """{"labelId":"label-bug"}"""),
            labelsById = mapOf(bugLabel.id to bugLabel),
        )
        assertEquals("added label bug", phrase)
    }

    @Test
    fun labelRemovedWithoutRowFallsBack() {
        val phrase = eventPhrase(event("label_removed", """{"labelId":"label-gone"}"""))
        assertEquals("removed a label", phrase)
    }

    @Test
    fun prEventsUsePayloadNumber() {
        assertEquals(
            "opened PR #91",
            eventPhrase(event("pr_opened", """{"prUrl":"https://x","prNumber":91,"branch":"exp/EXP-1"}""")),
        )
        assertEquals(
            "merged PR #91",
            eventPhrase(event("pr_merged", """{"prUrl":"https://x","prNumber":91}""")),
        )
    }

    @Test
    fun prEventsWithoutNumberFallBack() {
        assertEquals("opened a pull request", eventPhrase(event("pr_opened", """{"prUrl":null}""")))
        assertEquals("merged the pull request", eventPhrase(event("pr_merged", null)))
    }

    @Test
    fun projectMovedKeepsIdentifierDetail() {
        val phrase = eventPhrase(
            event("project_moved", """{"fromIdentifier":"EXP-4","toIdentifier":"SUP-9"}"""),
        )
        assertEquals("moved this to another project (EXP-4 → SUP-9)", phrase)
    }

    @Test
    fun unknownTypeSpacesUnderscores() {
        assertEquals("something new", eventPhrase(event("something_new", null)))
    }
}
