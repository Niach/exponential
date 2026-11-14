package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1830: the home widgets' folds, replayed off `home-widgets.json` (TS
// `home-widgets.test.ts`, same case names).
class HomeWidgetsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("home-widgets.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun statuses(input: JsonObject) = input.getValue("statuses").jsonArray.map {
        val s = it.jsonObject
        HomeWidgets.Status(s.str("id")!!, s.str("category")!!, s.str("color")!!, s.getValue("sortOrder").jsonPrimitive.double)
    }

    private fun issues(input: JsonObject) = input.getValue("issues").jsonArray.map {
        val i = it.jsonObject
        HomeWidgets.Issue(
            i.str("id")!!, i.str("identifier")!!, i.str("title")!!, i.str("statusId")!!, i.str("boardId")!!,
            i.str("teamId")!!, i.str("priority")!!, i.getValue("sortOrder").jsonPrimitive.double, i.str("assigneeId"),
            i.str("updatedAt")!!,
        )
    }

    private fun issueList(expected: JsonObject) = HomeWidgets.WidgetList(
        expected.getValue("count").jsonPrimitive.int,
        expected.getValue("rows").jsonArray.map {
            val r = it.jsonObject
            HomeWidgets.IssueRow(r.str("id")!!, r.str("identifier")!!, r.str("title")!!, r.str("statusCategory")!!, r.str("statusColor")!!)
        },
    )

    @Test
    fun `copy, icons and look are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("myIssues"), HomeWidgets.MY_ISSUES)
        assertEquals(copy.str("needsYou"), HomeWidgets.NEEDS_YOU)
        assertEquals(copy.str("emptyIssues"), HomeWidgets.EMPTY_ISSUES)
        assertEquals(copy.str("emptyNeedsYou"), HomeWidgets.EMPTY_NEEDS_YOU)
        assertEquals(copy.str("signedOut"), HomeWidgets.SIGNED_OUT)
        assertEquals(copy.str("sealedRow"), HomeWidgets.SEALED_ROW)
        assertEquals(copy.str("boardGallery"), HomeWidgets.BOARD_GALLERY)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(icons.str("myIssues"), HomeWidgets.ICON_MY_ISSUES)
        assertEquals(icons.str("needsYou"), HomeWidgets.ICON_NEEDS_YOU)
        assertEquals(icons.str("plan"), HomeWidgets.ICON_PLAN)
        assertEquals(icons.str("question"), HomeWidgets.ICON_QUESTION)
        assertEquals(icons.str("pr"), HomeWidgets.ICON_PR)
        assertEquals(fixture.getValue("openCategories").jsonArray.map { it.jsonPrimitive.content }, HomeWidgets.OPEN_CATEGORIES)
        assertEquals(fixture.getValue("categoryOrder").jsonArray.map { it.jsonPrimitive.content }, HomeWidgets.CATEGORY_ORDER)
        val capacity = fixture.getValue("capacity").jsonObject
        assertEquals(capacity.getValue("small").jsonPrimitive.int, HomeWidgets.CAPACITY_SMALL)
        assertEquals(capacity.getValue("medium").jsonPrimitive.int, HomeWidgets.CAPACITY_MEDIUM)
        assertEquals(capacity.getValue("large").jsonPrimitive.int, HomeWidgets.CAPACITY_LARGE)
        val look = fixture.getValue("look").jsonObject
        assertEquals(look.getValue("padding").jsonPrimitive.int, HomeWidgets.LOOK_PADDING)
        assertEquals(look.getValue("headerGap").jsonPrimitive.int, HomeWidgets.LOOK_HEADER_GAP)
        assertEquals(look.getValue("rowHeight").jsonPrimitive.int, HomeWidgets.LOOK_ROW_HEIGHT)
        assertEquals(look.getValue("glyph").jsonPrimitive.int, HomeWidgets.LOOK_GLYPH)
    }

    @Test
    fun boardCases() {
        for (case in cases("boardCases")) {
            val input = case.getValue("input").jsonObject
            val got = HomeWidgets.boardWidget(input.str("boardId")!!, statuses(input), issues(input), input.getValue("max").jsonPrimitive.int)
            assertEquals(case.str("name"), issueList(case.getValue("expected").jsonObject), got)
        }
    }

    @Test
    fun myIssuesCases() {
        for (case in cases("myIssuesCases")) {
            val input = case.getValue("input").jsonObject
            val got = HomeWidgets.myIssuesWidget(
                input.str("me")!!,
                input.getValue("teamIds").jsonArray.map { it.jsonPrimitive.content },
                statuses(input),
                issues(input),
                input.getValue("max").jsonPrimitive.int,
            )
            assertEquals(case.str("name"), issueList(case.getValue("expected").jsonObject), got)
        }
    }

    @Test
    fun needsYouCases() {
        for (case in cases("needsYouCases")) {
            val input = case.getValue("input").jsonObject
            val rows = input.getValue("rows").jsonArray.map {
                val r = it.jsonObject
                HomeWidgets.NeedsYouInput(
                    r.str("messageId")!!, r.str("sessionId")!!, r.str("rootId")!!, r.str("kind")!!,
                    r.str("opens")!!, r.str("title")!!, r.str("age")!!,
                )
            }
            val got = HomeWidgets.needsYouWidget(
                rows,
                input.getValue("sealedRootIds").jsonArray.map { it.jsonPrimitive.content },
                input.getValue("max").jsonPrimitive.int,
            )
            val expected = case.getValue("expected").jsonObject
            val want = HomeWidgets.WidgetList(
                expected.getValue("count").jsonPrimitive.int,
                expected.getValue("rows").jsonArray.map {
                    val r = it.jsonObject
                    HomeWidgets.NeedsYouRow(
                        r.str("messageId")!!, r.str("sessionId")!!, r.str("rootId")!!, r.str("opens")!!,
                        r.str("icon")!!, r.str("title")!!, r.str("age")!!,
                    )
                },
            )
            assertEquals(case.str("name"), want, got)
        }
    }
}
