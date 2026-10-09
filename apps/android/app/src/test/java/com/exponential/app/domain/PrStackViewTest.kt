package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1248: tree vs stack + the stack rail, locked x4 against
 * `pr-stack-view.json` (web `pr-stack.test.ts` "openPrShape + stackView",
 * desktop `stack_view_matches_the_fixture`, iOS `PrStackViewTests`).
 */
class PrStackViewTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("pr-stack-view.json")).jsonObject

    private fun JsonObject.string(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull

    private fun row(json: JsonObject) = stackFixtureIssue(json)

    @Test
    fun `stack_view matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.string("name")!!
            val issues = case.getValue("issues").jsonArray.map { row(it.jsonObject) }
            val issue = issues.first { it.id == case.string("issue") }
            assertEquals(name, case.string("shape"), PrStack.openPrShape(issue, issues).wire)
            val viewJson = case.getValue("view")
            val expected = if (viewJson is JsonNull) null else viewJson.jsonObject.let { view ->
                PrStack.StackView(
                    rows = view.getValue("rows").jsonArray.map { it.jsonObject }.map { r ->
                        PrStack.StackViewRow(
                            issueId = r.string("issueId")!!,
                            identifier = r.string("identifier")!!,
                            title = r.string("title")!!,
                            prNumber = r["prNumber"]?.jsonPrimitive?.intOrNull,
                            isCurrent = r.getValue("isCurrent").jsonPrimitive.boolean,
                        )
                    },
                    baseBranch = view.string("baseBranch"),
                )
            }
            assertEquals(name, expected, PrStack.stackView(issue, issues))
        }
    }

    @Test
    fun `pr_graph_shape tells a fork from a line`() {
        val bottom = issue("bottom", "exp/BOTTOM", "master")
        val middle = issue("middle", "exp/MIDDLE", "exp/BOTTOM")
        val top = issue("top", "exp/TOP", "exp/MIDDLE")
        assertEquals(PrStack.PrGraphShape.Single, PrStack.prGraphShape(listOf(bottom)))
        assertEquals(PrStack.PrGraphShape.Stack, PrStack.prGraphShape(listOf(bottom, middle, top)))
        val left = issue("left", "exp/LEFT", "exp/BOTTOM")
        assertEquals(PrStack.PrGraphShape.Tree, PrStack.prGraphShape(listOf(bottom, middle, left)))
    }

    @Test
    fun `pr_component walks the whole component from any member`() {
        val bottom = issue("bottom", "exp/BOTTOM", "master")
        val middle = issue("middle", "exp/MIDDLE", "exp/BOTTOM")
        val top = issue("top", "exp/TOP", "exp/MIDDLE")
        val stray = issue("stray", "exp/STRAY", "main")
        assertEquals(
            listOf("top", "bottom", "middle"),
            PrStack.prComponent(top, listOf(stray, top, bottom, middle)).map { it.id },
        )
    }

    private fun issue(id: String, branch: String, base: String) = IssueEntity(
        id = id,
        boardId = "board-1",
        number = 1,
        identifier = id.uppercase(),
        title = id,
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        branch = branch,
        prBaseBranch = base,
        prState = "open",
        createdAt = "2026-10-09T10:00:00Z",
        updatedAt = "2026-10-09T10:00:00Z",
    )

    @Test
    fun `the base row falls back to the board default branch, then the literal`() {
        assertEquals("release", PrStack.baseRowLabel("release", "master"))
        assertEquals("master", PrStack.baseRowLabel(null, "master"))
        assertEquals("master", PrStack.baseRowLabel("", "master"))
        assertEquals("default branch", PrStack.baseRowLabel(null, null))
        assertEquals("default branch", PrStack.baseRowLabel(null, " "))
    }
}

/** One fixture issue row of `pr-stack-view.json` / `stack-merge-choice.json`. */
internal fun stackFixtureIssue(json: JsonObject): IssueEntity {
    fun string(key: String): String? = json[key]?.jsonPrimitive?.contentOrNull
    return IssueEntity(
        id = string("id")!!,
        boardId = "board-1",
        number = 1,
        identifier = string("identifier")!!,
        title = string("title") ?: "Issue ${string("identifier")}",
        status = "in_review",
        priority = "none",
        sortOrder = 1.0,
        branch = string("branch"),
        prBaseBranch = string("prBaseBranch"),
        prState = string("prState"),
        prUrl = string("prUrl"),
        prNumber = json["prNumber"]?.jsonPrimitive?.intOrNull,
        createdAt = "2026-10-09T10:00:00Z",
        updatedAt = "2026-10-09T10:00:00Z",
    )

}
