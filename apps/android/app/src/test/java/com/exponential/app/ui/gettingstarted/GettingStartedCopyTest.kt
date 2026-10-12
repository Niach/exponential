package com.exponential.app.ui.gettingstarted

import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-1270 (EXP-1259 D6): the block's copy, row geometry and test ids are the
// fixture's (`getting-started.json`) — this used to compare against the web
// copy table; the fixture is the one source now.
class GettingStartedCopyTest {
    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("getting-started.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.int(key: String): Int = getValue(key).jsonPrimitive.int

    @Test
    fun `the header copy is the fixture's`() {
        assertEquals(fixture.string("title"), GettingStartedCopy.TITLE)
        assertEquals(fixture.string("progress"), GettingStartedCopy.PROGRESS)
        assertEquals(fixture.int("meter"), GettingStartedCopy.METER)
        val docs = fixture.getValue("docs").jsonObject
        assertEquals(docs.string("label"), GettingStartedCopy.DOCS_LABEL)
        assertEquals(docs.string("icon"), GettingStartedCopy.DOCS_ICON)
        assertEquals(docs.string("url"), GettingStartedCopy.DOCS_URL)
        assertEquals("4 of 8", GettingStartedCopy.progress(4, 8))
    }

    @Test
    fun `the row geometry is the fixture's`() {
        assertEquals(fixture.string("nextLabel"), GettingStartedCopy.NEXT_LABEL)
        assertEquals(fixture.int("maxWidth"), GettingStartedCopy.MAX_WIDTH)
        val row = fixture.getValue("row").jsonObject
        assertEquals(row.int("height"), GettingStartedCopy.ROW_HEIGHT)
        assertEquals(row.int("heightPhone"), GettingStartedCopy.ROW_HEIGHT_PHONE)
        assertEquals(row.int("iconSize"), GettingStartedCopy.ROW_ICON)
        assertEquals(row.int("padding"), GettingStartedCopy.ROW_PADDING)
        assertEquals(row.int("expandedPadding"), GettingStartedCopy.ROW_EXPANDED_PADDING)
    }

    @Test
    fun `every step's title, why and cta are the fixture's`() {
        val steps = fixture.getValue("steps").jsonArray.map { it.jsonObject }
        assertEquals(steps.map { it.string("key") }, GettingStartedStepKey.entries.map { it.wire })
        steps.forEach { step ->
            val key = GettingStartedStepKey.entries.first { it.wire == step.string("key") }
            assertEquals(step.string("title"), GettingStarted.step(key).title)
            assertEquals(step.string("why"), GettingStartedCopy.why(key))
            assertEquals(step.string("cta"), GettingStartedCopy.cta(key))
        }
    }

    @Test
    fun `the test ids are the fixture's`() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("block"), GettingStartedCopy.BLOCK_TEST_ID)
        assertEquals(ids.string("row"), GettingStartedCopy.ROW_TEST_ID)
        assertEquals(ids.string("cta"), GettingStartedCopy.CTA_TEST_ID)
        assertEquals(ids.string("docs"), GettingStartedCopy.DOCS_TEST_ID)
        assertEquals(ids.string("entry"), GettingStartedCopy.ENTRY_TEST_ID)
        assertEquals("getting-started-row-board", GettingStartedCopy.rowTestId(GettingStartedStepKey.Board))
    }
}
