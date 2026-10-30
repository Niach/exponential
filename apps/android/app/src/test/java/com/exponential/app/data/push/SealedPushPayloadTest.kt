package com.exponential.app.data.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SealedPushPayloadTest {
    private fun parse(json: String) = parseSealedPushPayload(json.toByteArray())

    @Test
    fun parsesTitleBodyAndTheStringDataMap() {
        assertEquals(
            SealedPushPayload("T", "B", mapOf("userId" to "u1", "type" to "comment", "issueId" to "i1")),
            parse("""{"title":"T","body":"B","data":{"userId":"u1","type":"comment","issueId":"i1"}}"""),
        )
    }

    @Test
    fun missingFieldsAreNullOrEmpty() {
        assertEquals(SealedPushPayload(null, null, emptyMap()), parse("{}"))
        assertEquals(SealedPushPayload("T", null, emptyMap()), parse("""{"title":"T","data":null}"""))
    }

    @Test
    fun nonStringDataValuesAreSkipped() {
        assertEquals(mapOf("a" to "1"), parse("""{"data":{"a":"1","n":2,"o":{}}}""")?.data)
    }

    @Test
    fun malformedPayloadsAreRefused() {
        assertNull(parse("not json"))
        assertNull(parse("[1,2]"))
        assertNull(parse("\"str\""))
        assertNull(parse("""{"data":"x"}"""))
    }

    @Test
    fun openSealedPushDropsBadBase64AndUnopenablePayloads() {
        assertNull(openSealedPush("!!!") { it })
        assertNull(openSealedPush("") { it })
        assertNull(openSealedPush(base64UrlEncode("x".toByteArray())) { null })
        val plain = """{"title":"T"}""".toByteArray()
        assertEquals("T", openSealedPush(base64UrlEncode(plain)) { it }?.title)
        // Padded base64url is accepted too.
        assertEquals("T", openSealedPush(base64UrlEncode(plain) + "==".take((4 - base64UrlEncode(plain).length % 4) % 4)) { it }?.title)
    }
}
