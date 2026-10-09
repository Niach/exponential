package com.exponential.app.domain

import com.exponential.app.data.api.OpenPull
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.TeamEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1244: the Reviews queue, locked ×4 (web `reviews-queue.test.ts`, iOS
 * `ReviewsQueueTests`, desktop `domain::reviews_queue`) against the ONE
 * contract fixture `reviews-queue.json`.
 */
class ReviewsQueueTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("reviews-queue.json")).jsonObject

    private fun JsonObject.string(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull

    private fun JsonObject.list(key: String) = getValue(key).jsonArray.map { it.jsonObject }

    private fun team(json: JsonObject) = TeamEntity(
        id = json.string("id")!!,
        name = json.string("id")!!,
        slug = json.string("id")!!,
        createdAt = "2026-10-01T00:00:00Z",
        updatedAt = "2026-10-01T00:00:00Z",
    )

    // BoardEntity.sortOrder is non-null on Android: a null sort_order sorts
    // last, which +∞ reproduces exactly.
    private fun board(json: JsonObject) = BoardEntity(
        id = json.string("id")!!,
        teamId = json.string("teamId")!!,
        name = json.string("name")!!,
        slug = json.string("id")!!,
        prefix = "EXP",
        color = "#888888",
        sortOrder = json["sortOrder"]?.jsonPrimitive?.doubleOrNull ?: Double.POSITIVE_INFINITY,
        createdAt = "2026-10-01T00:00:00Z",
        updatedAt = "2026-10-01T00:00:00Z",
    )

    private fun issue(json: JsonObject) = IssueEntity(
        id = json.string("id")!!,
        boardId = json.string("boardId")!!,
        number = 1,
        identifier = json.string("id")!!.uppercase(),
        title = "Issue ${json.string("id")}",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        prUrl = json.string("prUrl"),
        prState = json.string("prState"),
        // EXP-1248: the stack edge (absent = no edge).
        branch = json.string("branch"),
        prBaseBranch = json.string("prBaseBranch"),
        createdAt = json.string("createdAt")!!,
        updatedAt = json.string("createdAt")!!,
    )

    private fun session(json: JsonObject) = CodingSessionEntity(
        id = json.string("id")!!,
        issueId = json.string("issueId"),
        teamId = json.string("teamId")!!,
        userId = "me",
        status = "in_review",
        prUrl = json.string("prUrl"),
        prState = json.string("prState"),
        // started_at deliberately differs: the queue orders by created_at.
        startedAt = "2020-01-01T00:00:00Z",
        createdAt = json.string("createdAt")!!,
        updatedAt = json.string("createdAt")!!,
    )

    private fun pullRepo(json: JsonObject) = PullRepo(
        teamId = json.string("teamId")!!,
        repositoryId = json.string("repositoryId")!!,
        fullName = json.string("fullName")!!,
        pulls = json.list("pulls").map {
            OpenPull(number = it.getValue("number").jsonPrimitive.int, url = it.string("url")!!)
        },
    )

    @Test
    fun `the repo band caption is byte exact`() {
        assertEquals(
            fixture.getValue("labels").jsonObject.string("repoBandCaption"),
            ReviewsQueue.REPO_BAND_CAPTION,
        )
        assertEquals(
            fixture.getValue("labels").jsonObject.string("runBandCaption"),
            ReviewsQueue.RUN_BAND_CAPTION,
        )
    }

    @Test
    fun `every fixture case matches`() {
        val cases = fixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.string("name")!!
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val queue = ReviewsQueue.build(
                teams = input.list("teams").map(::team),
                boards = input.list("boards").map(::board),
                issues = input.list("issues").map(::issue),
                sessions = input.list("sessions").map(::session),
                pulls = input.list("pulls").map(::pullRepo),
            )
            assertEquals(
                "$name: boardGroups",
                expected.list("boardGroups").map { group ->
                    group.string("boardId") to group.list("entries").map { entry ->
                        entry.string("key") to entry.getValue("issueIds").jsonArray.map { it.jsonPrimitive.content }
                    }
                },
                queue.boardGroups.map { group ->
                    group.board.id to group.entries.map { entry -> entry.key to entry.issues.map { it.id } }
                },
            )
            assertEquals(
                "$name: runGroups",
                expected.list("runGroups").map { group ->
                    group.string("teamId") to group.getValue("sessionIds").jsonArray.map { it.jsonPrimitive.content }
                },
                queue.runGroups.map { group -> group.teamId to group.sessions.map { it.id } },
            )
            assertEquals(
                "$name: repoGroups",
                expected.list("repoGroups").map { group ->
                    Triple(
                        group.string("teamId"),
                        group.string("repositoryId"),
                        group.getValue("pullNumbers").jsonArray.map { it.jsonPrimitive.int },
                    )
                },
                queue.repoGroups.map { repo -> Triple(repo.teamId, repo.repositoryId, repo.pulls.map { it.number }) },
            )
            assertEquals("$name: count", expected.getValue("count").jsonPrimitive.int, queue.count)
        }
    }

    @Test
    fun `every nav case matches`() {
        val cases = fixture.list("navCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.string("name")!!
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val nav = ReviewsQueue.nav(
                yolo = input.getValue("yolo").jsonArray.map { it.jsonPrimitive.boolean },
                count = input.getValue("count").jsonPrimitive.int,
            )
            assertEquals(
                name,
                ReviewsNav(
                    dot = expected.getValue("dot").jsonPrimitive.boolean,
                    shows = expected.getValue("shows").jsonPrimitive.boolean,
                ),
                nav,
            )
        }
    }

    // EXP-1248: rule 9, a band's entries as drawn (tree nesting, stack items),
    // replayed x4 (web "reviewsQueue items", desktop
    // `queue_items_match_the_fixture`, iOS `ReviewsQueueTests.testGrouping`).
    @Test
    fun grouping() {
        val cases = fixture.list("groupingCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.string("name")!!
            val input = case.getValue("input").jsonObject
            val queue = ReviewsQueue.build(
                teams = input.list("teams").map(::team),
                boards = input.list("boards").map(::board),
                issues = input.list("issues").map(::issue),
                sessions = input.list("sessions").map(::session),
                pulls = input.list("pulls").map(::pullRepo),
            )
            val expected = case.getValue("expected").jsonObject.list("boards").map { band ->
                band.string("boardId") to band.list("items").map { item ->
                    when (item.string("kind")) {
                        "pr" -> listOf("pr", item.string("key"), item.getValue("depth").jsonPrimitive.int.toString())
                        else -> listOf("stack") +
                            item.getValue("keys").jsonArray.map { it.jsonPrimitive.content } +
                            listOf("base:${item.string("baseBranch")}")
                    }
                }
            }
            val actual = queue.boardGroups.map { group ->
                group.board.id to group.items.map { item ->
                    when (item) {
                        is ReviewsQueue.Item.Pr -> listOf("pr", item.entry.key, item.depth.toString())
                        is ReviewsQueue.Item.Stack -> listOf("stack") +
                            item.entries.map { it.key } + listOf("base:${item.baseBranch}")
                    }
                }
            }
            assertEquals(name, expected, actual)
        }
    }
}
