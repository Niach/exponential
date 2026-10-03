package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * SLOP-4: the reporter-reply copy, locked ×4 against `reporter-reply.json`
 * (web imports the json, iOS `ReporterReplyTests`, desktop `reporter_reply`).
 */
class ReporterReplyTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("reporter-reply.json")).jsonObject

    @Test
    fun `the copy is the contract's`() {
        val copy = fixture().getValue("copy").jsonObject
        val expected = mapOf(
            "toggleLabel" to ReporterReply.TOGGLE_LABEL,
            "placeholderOn" to ReporterReply.PLACEHOLDER_ON,
            "anonymousName" to ReporterReply.ANONYMOUS_NAME,
            "formerMemberName" to ReporterReply.FORMER_MEMBER_NAME,
            "reporterCaption" to ReporterReply.REPORTER_CAPTION,
            "toReporterCaption" to ReporterReply.TO_REPORTER_CAPTION,
            "sentToast" to ReporterReply.SENT_TOAST,
            "notSentToast" to ReporterReply.NOT_SENT_TOAST,
            "notificationLabel" to ReporterReply.NOTIFICATION_LABEL,
        )
        assertEquals(expected.keys, copy.keys)
        for ((key, value) in expected) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
        }
    }

    @Test
    fun `a blank reporter name reads as the anonymous visitor`() {
        assertEquals(ReporterReply.ANONYMOUS_NAME, ReporterReply.displayName(null))
        assertEquals(ReporterReply.ANONYMOUS_NAME, ReporterReply.displayName("   "))
        assertEquals("Emma Fischer", ReporterReply.displayName(" Emma Fischer "))
    }

    @Test
    fun `the ON placeholder names the reporter`() {
        assertEquals(
            "Reply to Emma Fischer… (emailed to them)",
            ReporterReply.placeholderOn("Emma Fischer"),
        )
    }
}
