package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueRelationEntity
import org.junit.Assert.assertEquals
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
    }
}
