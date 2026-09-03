package com.exponential.app.data.api

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-712: wire-format lock for the board repository/branch writes.
 *
 * Three server contracts are being honoured here, and each one breaks
 * silently if the input drifts:
 *  - `boards.create` takes `defaultBranch` only alongside a `repository`, and
 *    its `repository` is a `z.union` whose two arms have different shapes;
 *  - `boards.update` applies a field only when the KEY IS PRESENT, so "follow
 *    the repo again" has to travel as a literal null (the shared Json drops
 *    nulls off `@Serializable` classes — the ActionsWireFormatTest story);
 *  - `boards.setRepository` RESETS the board's branch unless `defaultBranch`
 *    rides along, so an unpinned retarget must OMIT the key.
 */
class BoardsWireFormatTest {

    @Test
    fun `create sends the branch alongside a registry repo`() {
        assertEquals(
            """{"teamId":"team-1","name":"Backend API","prefix":"API","color":"#6366f1",""" +
                """"icon":"square-kanban","repository":{"repositoryId":"repo-1"},""" +
                """"defaultBranch":"develop"}""",
            createBoardInput(
                teamId = "team-1",
                name = "Backend API",
                prefix = "API",
                color = "#6366f1",
                icon = "square-kanban",
                repository = BoardRepositoryChoice.Registry("repo-1"),
                defaultBranch = "develop",
            ).toString(),
        )
    }

    @Test
    fun `create omits an unpinned branch and an absent repo`() {
        assertEquals(
            """{"teamId":"team-1","name":"Design","prefix":"DES","icon":"square-kanban"}""",
            createBoardInput(
                teamId = "team-1",
                name = "Design",
                prefix = "DES",
                color = null,
                icon = "square-kanban",
                repository = null,
                defaultBranch = null,
            ).toString(),
        )
    }

    @Test
    fun `create carries the inline connect arm`() {
        assertEquals(
            """{"teamId":"team-1","name":"Mobile","prefix":"MOB","icon":"square-kanban",""" +
                """"repository":{"fullName":"acme/app","defaultBranch":"main","private":true},""" +
                """"defaultBranch":"release/26"}""",
            createBoardInput(
                teamId = "team-1",
                name = "Mobile",
                prefix = "MOB",
                color = null,
                icon = "square-kanban",
                repository = BoardRepositoryChoice.Inline(
                    fullName = "acme/app",
                    defaultBranch = "main",
                    isPrivate = true,
                ),
                defaultBranch = "release/26",
            ).toString(),
        )
    }

    @Test
    fun `clearing the board branch travels as a literal null`() {
        assertEquals(
            """{"boardId":"board-1","defaultBranch":null}""",
            updateBoardBranchInput(boardId = "board-1", defaultBranch = null).toString(),
        )
        assertEquals(
            """{"boardId":"board-1","defaultBranch":"develop"}""",
            updateBoardBranchInput(boardId = "board-1", defaultBranch = "develop").toString(),
        )
    }

    @Test
    fun `a retarget omits the branch key so the server resets the pin`() {
        assertEquals(
            """{"boardId":"board-1","repositoryId":"repo-2"}""",
            setBoardRepositoryInput(
                boardId = "board-1",
                repositoryId = "repo-2",
                defaultBranch = null,
            ).toString(),
        )
    }

    @Test
    fun `detaching a board sends a null repositoryId`() {
        assertEquals(
            """{"boardId":"board-1","repositoryId":null}""",
            setBoardRepositoryInput(
                boardId = "board-1",
                repositoryId = null,
                defaultBranch = null,
            ).toString(),
        )
    }

    @Test
    fun `a retarget may name the new repo's branch`() {
        assertEquals(
            """{"boardId":"board-1","repositoryId":"repo-2","defaultBranch":"develop"}""",
            setBoardRepositoryInput(
                boardId = "board-1",
                repositoryId = "repo-2",
                defaultBranch = "develop",
            ).toString(),
        )
    }
}
