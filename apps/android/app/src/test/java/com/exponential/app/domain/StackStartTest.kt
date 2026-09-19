package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-897 — the blocked-start rule, the same three tests web
// (`stack-start.test.ts`), iOS (`StackStartTests`) and the desktop
// (`blockers_of_*`) run.
class StackStartTest {

    private fun issue(
        id: String,
        identifier: String,
        status: String = "backlog",
    ) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = identifier,
        title = "Issue $identifier",
        status = status,
        priority = "none",
        sortOrder = 1.0,
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    private fun relation(
        id: String,
        issueId: String,
        relatedIssueId: String,
        type: String = DomainContract.issueRelationTypeBlocks,
    ) = IssueRelationEntity(
        id = id,
        issueId = issueId,
        relatedIssueId = relatedIssueId,
        type = type,
        source = DomainContract.issueRelationSourceUser,
        teamId = "team-1",
        createdAt = "2026-09-10T10:00:00Z",
        updatedAt = "2026-09-10T10:00:00Z",
    )

    @Test
    fun countsOnlyBlockedByRelations() {
        val issues = listOf(
            issue("me", "APP-3"),
            issue("lower", "APP-1"),
            issue("upper", "APP-9"),
            issue("kin", "APP-5"),
        )
        val relations = listOf(
            // APP-1 blocks me — a blocker.
            relation("r1", issueId = "lower", relatedIssueId = "me"),
            // I block APP-9 — the other side, not a blocker.
            relation("r2", issueId = "me", relatedIssueId = "upper"),
            // A parent relation is not a block.
            relation("r3", issueId = "kin", relatedIssueId = "me", type = DomainContract.issueRelationTypeParent),
        )
        assertEquals(
            listOf("APP-1"),
            StackStart.openBlockers("me", relations, issues).map { it.identifier },
        )
    }

    @Test
    fun dropsABlockerThatIsDoneCancelledOrADuplicate() {
        val issues = listOf(
            issue("me", "APP-3"),
            issue("open", "APP-1"),
            issue("done", "APP-2", status = "done"),
            issue("cancelled", "APP-4", status = "cancelled"),
            issue("duplicate", "APP-5", status = "duplicate"),
        )
        val relations = listOf(
            relation("r1", issueId = "open", relatedIssueId = "me"),
            relation("r2", issueId = "done", relatedIssueId = "me"),
            relation("r3", issueId = "cancelled", relatedIssueId = "me"),
            relation("r4", issueId = "duplicate", relatedIssueId = "me"),
        )
        assertEquals(
            listOf("APP-1"),
            StackStart.openBlockers("me", relations, issues).map { it.identifier },
        )
    }

    @Test
    fun dropsABlockerWhoseIssueRowIsNotSynced() {
        val issues = listOf(issue("me", "APP-3"), issue("synced", "APP-1"))
        val relations = listOf(
            relation("r1", issueId = "synced", relatedIssueId = "me"),
            relation("r2", issueId = "elsewhere", relatedIssueId = "me"),
        )
        assertEquals(
            listOf("APP-1"),
            StackStart.openBlockers("me", relations, issues).map { it.identifier },
        )
    }

    @Test
    fun copyIsTheSharedOne() {
        assertEquals("This issue is blocked", StackStart.BLOCKED_START_TITLE)
        assertEquals("Start anyway", StackStart.START_ANYWAY_LABEL)
        assertEquals("Stacked PR", StackStart.STACKED_PR_LABEL)
        assertEquals("This issue is blocked by ", StackStart.BODY_PREFIX)
        assertEquals(". Start anyway, or start a stacked PR?", StackStart.BODY_SUFFIX)
        // EXP-980: the batch half of the same dialog.
        assertEquals("Some of these issues are blocked", StackStart.BLOCKED_BATCH_TITLE)
        assertEquals(
            "Open issues outside this batch block it. Start anyway?",
            StackStart.BLOCKED_BATCH_BODY,
        )
    }

    // EXP-980: the stacked button is never hidden any more — it is disabled
    // with one reason at a time, the most fundamental first.
    @Test
    fun namesOneReasonTheMostFundamentalFirst() {
        assertNull(StackStart.stackDisabledReason(pickedCount = 1, canStack = true, hasCycle = false))
        assertEquals(
            StackStart.StackDisabledReason.Cap,
            StackStart.stackDisabledReason(pickedCount = 1, canStack = false, hasCycle = false),
        )
        assertEquals(
            StackStart.StackDisabledReason.Batch,
            StackStart.stackDisabledReason(pickedCount = 2, canStack = false, hasCycle = false),
        )
        assertEquals(
            StackStart.StackDisabledReason.Cycle,
            StackStart.stackDisabledReason(pickedCount = 2, canStack = true, hasCycle = true),
        )
    }

    @Test
    fun hasANoteForEveryReason() {
        assertEquals(
            StackStart.STACK_NEEDS_UPDATE_NOTE,
            StackStart.stackDisabledNote(StackStart.StackDisabledReason.Cap),
        )
        assertEquals(
            StackStart.STACK_SINGLE_ISSUE_NOTE,
            StackStart.stackDisabledNote(StackStart.StackDisabledReason.Batch),
        )
        assertEquals(
            StackStart.STACK_CYCLE_NOTE,
            StackStart.stackDisabledNote(StackStart.StackDisabledReason.Cycle),
        )
        assertEquals(
            "Update Exponential on this device to start stacked PRs.",
            StackStart.STACK_NEEDS_UPDATE_NOTE,
        )
        assertEquals(
            "A stacked PR starts one issue at a time.",
            StackStart.STACK_SINGLE_ISSUE_NOTE,
        )
        assertEquals(
            "These issues block each other in a cycle. Remove one relation to stack them.",
            StackStart.STACK_CYCLE_NOTE,
        )
    }
}
