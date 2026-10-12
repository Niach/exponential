package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// SLOP-28 (EXP-1259 D4): the search list, replayed off `search-list.json`.
class SearchListTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("search-list.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.int(key: String): Int = getValue(key).jsonPrimitive.int

    @Test
    fun `typeaheadPrefill replays every case`() {
        val cases = fixture.getValue("typeaheadPrefill").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val got = SearchList.typeaheadPrefill(case.string("text"), case.int("caret"))
            val expected = case.getValue("expected")
            if (expected is JsonNull) {
                assertNull(case.string("name"), got)
            } else {
                val want = expected.jsonObject
                assertEquals(case.string("name"), TypeaheadPrefill(want.int("start"), want.string("query")), got)
            }
        }
    }

    @Test
    fun `the geometry and test ids are the fixture's`() {
        val input = fixture.getValue("input").jsonObject
        assertEquals(input.int("height"), SearchList.INPUT_HEIGHT)
        assertEquals(input.string("icon"), SearchList.INPUT_ICON)
        val row = fixture.getValue("row").jsonObject
        assertEquals(row.int("height"), SearchList.ROW_HEIGHT)
        assertEquals(row.int("heightPhone"), SearchList.ROW_HEIGHT_PHONE)
        assertEquals(fixture.int("maxVisibleRows"), SearchList.MAX_VISIBLE_ROWS)
        assertEquals(fixture.int("fadeHeight"), SearchList.FADE_HEIGHT)
        assertEquals(fixture.int("popoverWidth"), SearchList.POPOVER_WIDTH)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("list"), SearchList.LIST_TEST_ID)
        assertEquals(ids.string("input"), SearchList.INPUT_TEST_ID)
        assertEquals(ids.string("row"), SearchList.ROW_TEST_ID)
        assertEquals(ids.string("count"), SearchList.COUNT_TEST_ID)
    }

    @Test
    fun `a pick replaces the recorded token with the identifier and a space`() {
        assertEquals("Explain #EXP-12 " to 16, SearchList.spliceTypeaheadPick("Explain #", 8, 0, "EXP-12"))
        assertEquals("Explain #EXP-12 now" to 16, SearchList.spliceTypeaheadPick("Explain #sid now", 8, 3, "EXP-12"))
        assertEquals("(#EXP-1 " to 8, SearchList.spliceTypeaheadPick("(#ex", 1, 2, "EXP-1"))
        assertNull(SearchList.spliceTypeaheadPick("Explain", 3, 0, "EXP-1"))
    }

    @Test
    fun `a pick never eats the text after the caret`() {
        // Review #6: "see docs", `#` typed before "docs" (caret 5), EXP-1 picked.
        assertEquals("see #EXP-1 docs" to 11, SearchList.spliceTypeaheadPick("see #docs", 4, 0, "EXP-1"))
    }

    @Test
    fun `backspace on an empty input deletes the hash and the query typed`() {
        assertEquals("Explain " to 8, SearchList.deleteTypeaheadToken("Explain #", 8, 0))
        assertEquals("a  b" to 2, SearchList.deleteTypeaheadToken("a #x b", 2, 1))
        assertEquals("see docs" to 4, SearchList.deleteTypeaheadToken("see #docs", 4, 0))
    }
}
