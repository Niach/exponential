package com.exponential.app.data.push

import com.exponential.app.data.sealed.OpenedBody
import com.exponential.app.data.sealed.SealedCrypto
import com.exponential.app.data.sealed.SealedFfi
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #VAPP-45: packages/domain-contract/fixtures/question-push.json replayed:
 * `textCases` (the chat ask preview, cut at bodyMax) and `bodyCases` (the
 * sealed text opened through the UniFFI host build, or the push's body).
 */
class QuestionPushFixtureTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.push.questionFixture")
            ?: error("exponential.push.questionFixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun JsonObject.str(key: String) = this[key]!!.jsonPrimitive.content

    @Test
    fun theBodyCapMatchesTheFixture() {
        assertEquals(fixture.str("bodyMax").toInt(), QuestionPush.BODY_MAX)
        val keys = fixture["dataKeys"]!!.jsonObject
        assertEquals(keys.str("sealedText"), QuestionPush.SEALED_TEXT_KEY)
        assertEquals(keys.str("rootId"), QuestionPush.ROOT_ID_KEY)
    }

    @Test
    fun textCasesShowTheChatAskPreview() {
        for (case in fixture["textCases"]!!.jsonArray) {
            val c = case.jsonObject
            assertEquals(c.str("name"), c.str("shown"), QuestionPush.text(c.str("askText")))
        }
    }

    @Test
    fun bodyCasesOpenOrFallBack() {
        for (case in fixture["bodyCases"]!!.jsonArray) {
            val c = case.jsonObject
            val data = c["data"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }
            val keys = (c["keys"] as? JsonObject)?.entries?.associate { (epoch, secret) -> epoch.toInt() to secret.jsonPrimitive.content.hexBytes() }
            val shown = QuestionPush.body(data, { rootId -> keys.takeIf { rootId == data["rootId"] } }, SealedFfi, c.str("fallback"))
            assertEquals(c.str("name"), c.str("shown"), shown)
        }
    }

    @Test
    fun aThrowingOpenerKeepsThePushBody() {
        val throwing = object : SealedCrypto by SealedFfi {
            override fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody = throw UnsatisfiedLinkError("no lib")
        }
        val data = mapOf("rootId" to "r", "sealedText" to "exp-sealed:v1:1:AAAA")
        assertEquals("New message in a session", QuestionPush.body(data, { mapOf(1 to ByteArray(32)) }, throwing, "New message in a session"))
        assertEquals(null, QuestionPush.body(emptyMap(), { null }, SealedFfi, null))
    }
}

private fun String.hexBytes(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
