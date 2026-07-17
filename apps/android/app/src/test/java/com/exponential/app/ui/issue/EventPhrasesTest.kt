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
        workspaceId = "ws-1",
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
    fun projectMovedKeepsIdentifierDetail() {
        assertEquals(
            "moved this to another project (EXP-4 → SUP-9)",
            phrase(event("project_moved", """{"fromIdentifier":"EXP-4","toIdentifier":"SUP-9"}""")),
        )
    }

    @Test
    fun unknownTypeSpacesUnderscores() {
        assertEquals("something new", phrase(event("something_new", null)))
    }
}
