package com.exponential.app.ui.gettingstarted

import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-1270 (EXP-1259 D6): the block's copy, geometry and test ids are the
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
    fun `the card geometry is the fixture's`() {
        val card = fixture.getValue("card").jsonObject
        assertEquals(card.int("width"), GettingStartedCopy.CARD_WIDTH)
        assertEquals(card.int("height"), GettingStartedCopy.CARD_HEIGHT)
        assertEquals(card.int("padding"), GettingStartedCopy.CARD_PADDING)
        assertEquals(card.int("icon"), GettingStartedCopy.CARD_ICON)
        assertEquals(card.int("gap"), GettingStartedCopy.CARD_GAP)
    }

    @Test
    fun `the test ids are the fixture's`() {
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.string("block"), GettingStartedCopy.BLOCK_TEST_ID)
        assertEquals(ids.string("card"), GettingStartedCopy.CARD_TEST_ID)
        assertEquals(ids.string("docs"), GettingStartedCopy.DOCS_TEST_ID)
        assertEquals(ids.string("entry"), GettingStartedCopy.ENTRY_TEST_ID)
        assertEquals("getting-started-card-board", GettingStartedCopy.cardTestId(GettingStartedStepKey.Board))
    }
}
