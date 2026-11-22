package com.exponential.app.data.sealed

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * #EXP-1958: sealed-content.json `cardStamps` replayed on Android:
 * `tapCases` through the Kotlin twin of `cardItemWire` ([CardItemWire]),
 * `openCases` through `openCard` of the UniFFI binding (the host build of
 * exponential-net-ffi, whose enet-core lays the stamps over the items) and
 * through the app's opener [SealedContent] (its caches keyed by the whole
 * stored card, so a new stamp is a new opening).
 */
class CardStampsFixtureTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.enet.sealedFixture")
            ?: error("exponential.enet.sealedFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }
    private val stamps = fixture["cardStamps"]!!.jsonObject
    private val splits = fixture["card"]!!.jsonObject["splits"]!!.jsonArray.associate { split ->
        split.jsonObject["name"]!!.jsonPrimitive.content to split.jsonObject["envelope"]!!.jsonObject
    }
    private val keys: Map<Int, ByteArray> by lazy {
        fixture["keys"]!!.jsonObject.entries.associate { (epoch, secret) -> epoch.toInt() to secret.jsonPrimitive.content.hex() }
    }
    private val root = "00000000-0000-4000-8000-0000000000aa"

    @Before
    fun setUp() {
        SealedContent.crypto = SealedFfi
        SealedContent.clearCache()
    }

    @After
    fun tearDown() {
        SealedKeys.drop(SealedKeys.keys.value.keys)
        SealedContent.clearCache()
    }

    @Test
    fun tapCasesRouteLikeTheReference() {
        for (case in stamps["tapCases"]!!.jsonArray.map { it.jsonObject }) {
            val name = case["name"]!!.jsonPrimitive.content
            val action = CardItemAction.entries.first { it.wire == case["action"]!!.jsonPrimitive.content }
            val route = CardItemWire.route(
                sealed = case["stored"]!!.jsonPrimitive.content == "sealed",
                opened = case["opened"]!!.jsonPrimitive.boolean,
                action = action,
            )
            assertEquals(name, case["wire"]!!.jsonPrimitive.content, route.wire.wire)
            assertEquals(name, (case["call"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content, route.call)
        }
    }

    @Test
    fun openCasesLayTheStampsOverTheItems() {
        for (case in stamps["openCases"]!!.jsonArray.map { it.jsonObject }) {
            val name = case["name"]!!.jsonPrimitive.content
            val stored = JsonObject(splits.getValue(case["stored"]!!.jsonPrimitive.content) + ("stamps" to case["stamps"]!!))
            val held = if (case["keys"]!!.jsonPrimitive.content == "tree") keys else emptyMap()
            val opened = SealedFfi.openCard(stored.toString(), held)
            assertEquals(name, case["status"]!!.jsonPrimitive.content, opened.status.name.replaceFirstChar(Char::lowercase))
            val card = Json.parseToJsonElement(opened.cardJson).jsonObject
            val items = case["items"]
            if (items != null) {
                assertEquals(name, items, card["items"])
                assertFalse(name, "stamps" in card)
                // The app's opener: the same card, and a new stamp = a new opening (no stale cache).
                val viaApp = SealedContent.openCard(root, stored, mapOf(root to keys))
                assertEquals(name, items, viaApp["items"])
                val unstamped = SealedContent.openCard(root, splits.getValue(case["stored"]!!.jsonPrimitive.content), mapOf(root to keys))
                assertNull(name, unstamped["stamps"])
            } else {
                assertEquals(name, case["card"], card)
            }
        }
    }

    @Test
    fun aStampedItemIsNoLongerOpen() {
        val stored = JsonObject(splits.getValue("suggestions") + ("stamps" to Json.parseToJsonElement("""{"s1":{"here":true}}""")))
        val opened = SealedContent.openCard(root, stored, mapOf(root to keys))
        assertEquals(emptyList<String>(), CardItemWire.openIds(opened, CardItemWire::suggestionOpen))
        val replies = SealedContent.openCard(root, splits.getValue("replies"), mapOf(root to keys))
        assertEquals(listOf("r1"), CardItemWire.openIds(replies, CardItemWire::replyOpen))
    }

    @Test
    fun aStartsRequestAndIssues() {
        assertEquals("Add tests\n\nCover the readers rule", CardItemWire.request("Add tests", "Cover the readers rule"))
        val uuid = "00000000-0000-4000-8000-000000000012"
        val ids = CardItemWire.startIssueIds(listOf(uuid, "#EXP-12", "EXP-99", "nonsense")) { wanted ->
            wanted.filter { it == "EXP-12" }.associateWith { "00000000-0000-4000-8000-000000000013" }
        }
        assertEquals(listOf(uuid, "00000000-0000-4000-8000-000000000013"), ids)
    }

    /** `cardStamps` NEVER IN CLEAR: what the writer could not seal (a `clearTree` target) is refused before any request. */
    @Test
    fun aCardTapLeavesSealedOrNotAtAll() {
        val item = com.exponential.app.data.api.CardItemRef("0b6f4a8e-51c3-4d27-9e1a-7c2d3f4a5b6c", "c2a9e7d4-18f3-4b65-a0c7-9e4d2b1f6a83")
        val tap = com.exponential.app.data.api.SessionsSendInput(sessionId = root, text = "Yes, ship it", cardItem = item)
        // The writer found a clear tree: the opened reply is still plaintext.
        assertFalse(CardItemWire.leavesSealed(tap))
        assertFalse(CardItemWire.leavesSealed(tap.copy(text = null)))
        val envelope = tap.copy(text = SealedContent.BODY_PREFIX + "1.abc")
        assertTrue(CardItemWire.leavesSealed(envelope))
        // A Start whose title was not moved into `sealedName`.
        assertFalse(CardItemWire.leavesSealed(envelope.copy(name = "Add tests")))
        assertTrue(CardItemWire.leavesSealed(envelope.copy(sealedName = SealedContent.BODY_PREFIX + "1.def")))
        // A send that is no card tap keeps the composer's rules (a clear tree takes plaintext).
        assertTrue(CardItemWire.leavesSealed(tap.copy(cardItem = null)))
    }

    private fun String.hex(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
