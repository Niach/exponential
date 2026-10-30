package com.exponential.app.data.sealed

import com.exponential.app.data.db.ConversationEntity
import com.exponential.app.data.db.MessageEntity
import com.exponential.app.data.db.SessionEntity
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * EXP-1459: packages/domain-contract/fixtures/sealed-content.json replayed
 * through the UniFFI binding (the host build of exponential-net-ffi, loaded
 * by JNA from `jna.library.path`), the same vectors TS, Rust and Swift
 * replay, plus the app's opener ([SealedContent]) over Room rows.
 */
class SealedContentFixtureTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.enet.sealedFixture")
            ?: error("exponential.enet.sealedFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun JsonObject.obj(key: String) = this[key]!!.jsonObject
    private fun JsonObject.str(key: String) = this[key]!!.jsonPrimitive.content
    private fun JsonObject.hex(key: String) = str(key).hexBytes()
    private fun JsonObject.epochOrNull(key: String) = (this[key] as? JsonPrimitive)?.intOrNull

    private val keys: Map<Int, ByteArray> by lazy {
        fixture.obj("keys").entries.associate { (epoch, secret) -> epoch.toInt() to secret.jsonPrimitive.content.hexBytes() }
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
    fun theWrapVectorUnwrapsWithTheInstallSecretAndTheInstallSecretDerives() {
        val wrap = fixture.obj("wrap")
        val install = fixture.obj("install")
        assertArrayEquals(wrap.hex("treeSecret"), SealedFfi.unwrapTreeKey(install.hex("secret"), wrap.str("wrapped")))
        // A fresh wrap for the install's public key opens to the same secret.
        val fresh = SealedFfi.wrapTreeKey(install.str("x25519Public"), wrap.hex("treeSecret"))
        // 81 bytes = 108 unpadded base64url chars.
        assertEquals(108, fresh.length)
        assertArrayEquals(wrap.hex("treeSecret"), SealedFfi.unwrapTreeKey(install.hex("secret"), fresh))
    }

    @Test
    fun theBodyEnvelopeOpensToItsText() {
        val body = fixture.obj("body")
        val opened = SealedFfi.openBody(body.str("envelope"), keys)
        assertEquals(SealStatus.Opened, opened.status)
        assertEquals(body.str("text"), opened.text)
        // Sealing with a fresh ephemeral never repeats the vector but opens to the text.
        val again = SealedFfi.sealBody(body.str("treePublicKey"), 1, body.str("text"))
        assertNotEquals(body.str("envelope"), again)
        assertEquals(body.str("text"), SealedFfi.openBody(again, keys).text)
    }

    @Test
    fun everyBodyCaseOpensToItsStatusEpochAndText() {
        for (case in fixture["bodyCases"]!!.jsonArray.map { it.jsonObject }) {
            val name = case.str("name")
            val opened = SealedFfi.openBody(case.str("body"), keys)
            assertEquals(name, case.str("status"), opened.status.wire)
            assertEquals(name, case.epochOrNull("epoch"), opened.epoch)
            assertEquals(name, (case["text"] as? JsonPrimitive)?.takeIf { it.isString }?.content, opened.text)
        }
    }

    @Test
    fun everyCardCaseOpensToItsStatusEpochAndCard() {
        for (case in fixture["cardCases"]!!.jsonArray.map { it.jsonObject }) {
            val name = case.str("name")
            val opened = SealedFfi.openCard(case.obj("card").toString(), keys)
            assertEquals(name, case.str("status"), opened.status.wire)
            assertEquals(name, case.epochOrNull("epoch"), opened.epoch)
            assertEquals(name, case["opened"], Json.parseToJsonElement(opened.cardJson))
        }
    }

    @Test
    fun everyCardSplitsEnvelopeOpensToTheFullCardAndResealsToItsClearPart() {
        val card = fixture.obj("card")
        for (split in card["splits"]!!.jsonArray.map { it.jsonObject }) {
            val name = split.str("name")
            val opened = SealedFfi.openCard(split.obj("envelope").toString(), keys)
            assertEquals(name, SealStatus.Opened, opened.status)
            assertEquals(name, JsonObject(split.obj("card") + split.obj("clear")), Json.parseToJsonElement(opened.cardJson))
            // Our own seal of the full card carries exactly the clear part beside `sealed`.
            val sealed = Json.parseToJsonElement(SealedFfi.sealCard(card.str("treePublicKey"), 1, split.obj("card").toString())).jsonObject
            assertEquals(name, split.obj("clear"), JsonObject(sealed - "sealed"))
            // Opening lays the clear part over the sealed card (`answered` is derived).
            assertEquals(name, JsonObject(split.obj("card") + split.obj("clear")), Json.parseToJsonElement(SealedFfi.openCard(sealed.toString(), keys).cardJson))
        }
    }

    @Test
    fun aRotatedTreeOpensOnlyWithItsEpoch() {
        val rotated = fixture.obj("rotatedTree")
        val envelope = SealedFfi.sealBody(rotated.str("publicKey"), 2, "after rotation")
        assertEquals(SealStatus.NoKey, SealedFfi.openBody(envelope, keys).status)
        val both = keys + (2 to rotated.hex("secret"))
        assertEquals("after rotation", SealedFfi.openBody(envelope, both).text)
    }

    @Test
    fun theOpenerOpensRowsItHoldsKeysForAndLeavesTheRestSealed() {
        val body = fixture.obj("body")
        val row = MessageEntity(id = "m1", rootId = root, body = body.str("envelope"))
        // No key yet: the row stays sealed (the screen shows the locked state).
        val locked = SealedContent.message(row, emptyMap())
        assertTrue(SealedContent.isSealedBody(locked.body))
        SealedKeys.merge(mapOf(root to keys))
        assertEquals(body.str("text"), SealedContent.message(row).body)
        // Another tree's key never opens it.
        assertTrue(SealedContent.isSealedBody(SealedContent.message(row.copy(rootId = "other"), mapOf("other" to emptyMap())).body))
        // Plaintext passes through untouched (the same instance).
        val plain = row.copy(body = "Hello")
        assertTrue(plain === SealedContent.message(plain))
    }

    @Test
    fun theOpenerOpensASealedCardAndKeepsALockedOneSealed() {
        val split = fixture.obj("card")["splits"]!!.jsonArray.first().jsonObject
        val row = MessageEntity(id = "m2", rootId = root, card = split.obj("envelope").toString())
        val locked = SealedContent.message(row, emptyMap())
        assertTrue(SealedContent.isSealedCard(Json.parseToJsonElement(locked.card!!).jsonObject))
        val opened = SealedContent.message(row, mapOf(root to keys))
        assertEquals(split.obj("card"), Json.parseToJsonElement(opened.card!!))
    }

    @Test
    fun aSealedTitleOpensIntoTheTitleAndALockedOneKeepsTheNeutralTitle() {
        val envelope = SealedFfi.sealBody(fixture.obj("tree").str("publicKey"), 1, "Fix the login flow")
        val session = SessionEntity(id = root, rootId = root, title = "Session", sealedTitle = envelope)
        assertEquals("Fix the login flow", SealedContent.session(session, mapOf(root to keys)).title)
        assertEquals("Session", SealedContent.session(session, emptyMap()).title)
        // A child opens with its ROOT's keys.
        val child = session.copy(id = "child", sealedTitle = envelope)
        assertEquals("Fix the login flow", SealedContent.session(child, mapOf(root to keys)).title)
    }

    @Test
    fun aDigestOpensItsBodiesAndCards() {
        val body = fixture.obj("body")
        val split = fixture.obj("card")["splits"]!!.jsonArray.first().jsonObject
        val digest = JsonObject(
            mapOf(
                "first" to JsonObject(mapOf("id" to JsonPrimitive("a"), "body" to JsonPrimitive(body.str("envelope")), "card" to JsonNull)),
                "waiting" to JsonObject(mapOf("id" to JsonPrimitive("b"), "body" to JsonPrimitive(""), "card" to split.obj("envelope"))),
                "agentTimes" to JsonArray(emptyList()),
            ),
        ).toString()
        val row = ConversationEntity(id = "c1", sessionId = root, digest = digest)
        val opened = Json.parseToJsonElement(SealedContent.conversation(row, root, mapOf(root to keys)).digest!!).jsonObject
        assertEquals(body.str("text"), opened.obj("first").str("body"))
        assertEquals(split.obj("card"), opened.obj("waiting").obj("card"))
        // Without a key (or a root) the digest stays as synced.
        assertEquals(digest, SealedContent.conversation(row, root, emptyMap()).digest)
        assertEquals(digest, SealedContent.conversation(row, null, mapOf(root to keys)).digest)
    }

    @Test
    fun aNoKeyAsksTheHolderOnceAWhile() {
        while (SealedKeys.missing.tryReceive().isSuccess) Unit
        val row = MessageEntity(id = "m3", rootId = root, body = fixture.obj("body").str("envelope"))
        SealedContent.message(row, emptyMap())
        assertTrue(SealedKeys.missing.tryReceive().isSuccess)
        SealedContent.message(row, emptyMap())
        assertFalse(SealedKeys.missing.tryReceive().isSuccess)
    }
}

private fun String.hexBytes(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
