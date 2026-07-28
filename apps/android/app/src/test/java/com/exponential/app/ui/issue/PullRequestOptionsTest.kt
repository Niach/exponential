package com.exponential.app.ui.issue

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-259/EXP-270: the `pr` input's options are the team's OPEN issue-linked
 * pull requests deduped by prUrl — a batch PR (several issues, one PR) must
 * appear ONCE, carrying a representative issue id and listing every linked
 * identifier.
 */
class PullRequestOptionsTest {

    private fun issue(
        id: String,
        boardId: String = "b-1",
        identifier: String = "EXP-1",
        prUrl: String? = "https://github.com/acme/web/pull/1",
        prNumber: Int? = 1,
        prState: String? = DomainContract.prStateOpen,
    ) = IssueEntity(
        id = id,
        boardId = boardId,
        number = 1,
        identifier = identifier,
        title = "t",
        status = "in_review",
        priority = "none",
        sortOrder = 0.0,
        prUrl = prUrl,
        prNumber = prNumber,
        prState = prState,
        createdAt = "2026-07-25T00:00:00Z",
        updatedAt = "2026-07-25T00:00:00Z",
    )

    @Test
    fun `dedupes a batch pull request into one option`() {
        val batchUrl = "https://github.com/acme/web/pull/7"
        val options = buildPullRequestOptions(
            listOf(
                issue(id = "i-2", identifier = "EXP-2", prUrl = batchUrl, prNumber = 7),
                issue(id = "i-1", identifier = "EXP-1", prUrl = batchUrl, prNumber = 7),
                issue(
                    id = "i-3",
                    identifier = "EXP-3",
                    prUrl = "https://github.com/acme/web/pull/9",
                    prNumber = 9,
                ),
            ),
            teamBoardIds = setOf("b-1"),
        )

        assertEquals(2, options.size)
        val batch = options.first { it.identifiers.size == 2 }
        assertEquals(listOf("EXP-1", "EXP-2"), batch.identifiers)
        // Representative id is deterministic (lowest id), not query-order bound.
        assertEquals("i-1", batch.issueId)
        assertEquals("#7 · EXP-1, EXP-2", batch.label)
        assertEquals(setOf("i-1", "i-2"), batch.linkedIssueIds.toSet())
    }

    @Test
    fun `resolves any linked issue id to the representative option`() {
        val batchUrl = "https://github.com/acme/web/pull/7"
        val options = buildPullRequestOptions(
            listOf(
                issue(id = "i-2", identifier = "EXP-2", prUrl = batchUrl, prNumber = 7),
                issue(id = "i-1", identifier = "EXP-1", prUrl = batchUrl, prNumber = 7),
                issue(
                    id = "i-3",
                    identifier = "EXP-3",
                    prUrl = "https://github.com/acme/web/pull/9",
                    prNumber = 9,
                ),
            ),
            teamBoardIds = setOf("b-1"),
        )

        // EXP-323: the Reviews row hands over its OWN representative (newest
        // issue), which is not this builder's (lowest id) — both must land on
        // the same option, and its canonical id is what the picker renders.
        assertEquals("i-1", options.optionForIssue("i-2")?.issueId)
        assertEquals("i-1", options.optionForIssue("i-1")?.issueId)
        assertEquals("i-3", options.optionForIssue("i-3")?.issueId)
        assertEquals(null, options.optionForIssue("unknown"))
    }

    @Test
    fun `skips other teams, closed PRs and rows without a url`() {
        val options = buildPullRequestOptions(
            listOf(
                issue(id = "mine"),
                issue(id = "other-team", boardId = "b-9"),
                issue(id = "merged", prState = DomainContract.prStateMerged),
                issue(id = "no-url", prUrl = null, prNumber = null),
            ),
            teamBoardIds = setOf("b-1"),
        )

        assertEquals(listOf("mine"), options.map { it.issueId })
    }

    @Test
    fun `label falls back to identifiers when the pr number is missing`() {
        val options = buildPullRequestOptions(
            listOf(issue(id = "i-1", identifier = "EXP-1", prNumber = null)),
            teamBoardIds = setOf("b-1"),
        )

        assertEquals("EXP-1", options.single().label)
    }
}
