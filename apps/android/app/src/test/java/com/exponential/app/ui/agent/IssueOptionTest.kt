package com.exponential.app.ui.agent

import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueSearch
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.ui.markdown.chatTeamIssueRefTargets
import com.exponential.app.ui.markdown.mentionMembers
import org.junit.Assert.assertEquals
import org.junit.Test

class IssueOptionTest {
    private val stamp = "2026-01-01 00:00:00+00"

    private fun issue(id: String, status: String, statusId: String?) = IssueEntity(
        id = id, boardId = "ba", number = 1, identifier = id.uppercase(), title = id,
        status = status, priority = "none", creatorId = "u1", sortOrder = 1.0,
        createdAt = "2026-11-0${id.last()}T00:00:00Z", updatedAt = stamp,
    ).copy(statusId = statusId)

    private fun statusRow(id: String, category: IssueStatusCategory, name: String, builtinKey: String?) = IssueStatusEntity(
        id = id, teamId = "t-a", category = category.wire, name = name, sortOrder = 0.0,
        builtinKey = builtinKey, createdAt = stamp, updatedAt = stamp,
    )

    // #EXP-2096: a chat's `+` › Issue rows drew every glyph as Backlog (the
    // option dropped the target's status and resolved against no rows).
    @Test
    fun `a chat's Issue rows keep each issue's own status glyph`() {
        val board = BoardEntity(
            id = "ba", teamId = "t-a", name = "A", slug = "a", prefix = "EXP", color = "#6366f1",
            sortOrder = 0.0, createdAt = stamp, updatedAt = stamp,
        )
        val rows = listOf(
            statusRow("s-backlog", IssueStatusCategory.Backlog, "Backlog", "backlog"),
            statusRow("s-progress", IssueStatusCategory.Started, "In Progress", "in_progress"),
            statusRow("s-qa", IssueStatusCategory.Started, "QA", null),
            statusRow("s-done", IssueStatusCategory.Completed, "Done", "done"),
        )
        val issues = listOf(
            issue("exp-1", "backlog", "s-backlog"),
            issue("exp-2", "in_progress", "s-progress"),
            issue("exp-3", "in_progress", "s-qa"),
            issue("exp-4", "done", "s-done"),
        )
        val options = chatTeamIssueRefTargets(issues, listOf(board), rows, listOf("t-a")).map(::issueOption)
        assertEquals(
            mapOf("EXP-1" to "Backlog", "EXP-2" to "In Progress", "EXP-3" to "QA", "EXP-4" to "Done"),
            options.associate { it.identifier to it.glyph.name },
        )
        // The anchor rides along: done issues rank below the undone ones.
        assertEquals("EXP-4", IssueSearch.rank(options, "", exclude = emptySet()).last().identifier)
    }

    @Test
    fun `an option with only the anchor reads the builtin glyph`() {
        val option = IssueOption(
            id = "i", identifier = "EXP-9", title = "t", repositoryId = null, status = "in_progress", priority = null,
            description = null, createdAt = null, updatedAt = null, resolvedStatus = null,
        )
        assertEquals(IssueStatusCategory.Started, option.glyph.category)
    }

    // #EXP-2095 (agent-settings.json `assistant.chatTeams` (2)): the `@`
    // list's ONE order ×4, whatever order the rows arrive in.
    @Test
    fun `the at list is by name case-insensitively, then email`() {
        fun user(id: String, name: String?, email: String) = UserEntity(id = id, name = name, email = email, createdAt = stamp, updatedAt = stamp)
        val roster = listOf(
            user("u1", "Zoe", "zoe@example.com"),
            user("u2", "Sofia", "sofia@example.com"),
            user("u3", "alex", "alex@example.com"),
            user("u4", null, "hannes@example.com"),
            user("u5", "Mira", "mira@example.com"),
        )
        assertEquals(
            listOf("alex", "hannes@example.com", "Mira", "Sofia", "Zoe"),
            mentionMembers(roster).map { it.name },
        )
    }
}
