package com.exponential.app.data.sealed

import com.exponential.app.data.api.AnswerInput
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-1459: what the writer does before sealing (contract addendum 2 on #EXP-1458). */
class SealedWriteTest {
    @Test
    fun mentionsAreTheEmailTokensLowercasedOnce() {
        assertEquals(
            listOf("ann@example.com", "bo@x.io"),
            SealedWrite.mentionEmails("hi @Ann@Example.com and @bo@x.io, again @ann@example.com; mail me@x.io"),
        )
    }

    @Test
    fun issueRefsAreTheHashIdentifiersUppercasedOnce() {
        assertEquals(listOf("EXP-12", "APP-3"), SealedWrite.issueRefs("fix #EXP-12 and #app-3, see #exp-12; not foo#EXP-9 or EXP-4"))
    }

    /** #EXP-1683: a file in a sealed chat is a link line, never an image. */
    @Test
    fun attachmentLinesReplayTheFixture() {
        val fixture = kotlinx.serialization.json.Json.parseToJsonElement(
            java.io.File(System.getProperty("exponential.enet.sealedFixture")!!).readText(),
        ).jsonObject
        assertEquals(
            com.exponential.app.domain.INLINE_IMAGE_CONTENT_TYPES,
            fixture.getValue("inlineImageTypes").jsonArray.map { it.jsonPrimitive.content }.toSet(),
        )
        fixture.getValue("attachmentEmbedCases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val uploads = case.getValue("uploads").jsonObject.mapValues { (_, upload) ->
                SealedWrite.Upload(
                    upload.jsonObject.getValue("filename").jsonPrimitive.content,
                    upload.jsonObject.getValue("contentType").jsonPrimitive.content,
                )
            }
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("expected").jsonPrimitive.content,
                SealedWrite.withAttachmentEmbeds(
                    case.getValue("text").jsonPrimitive.content,
                    case.getValue("ids").jsonArray.map { it.jsonPrimitive.content },
                    uploads::get,
                ),
            )
        }
    }

    @Test
    fun attachmentsEmbedInsideTheBodyUnlessReferenced() {
        val uploads = mapOf(
            "a" to SealedWrite.Upload("shot [1].png", "image/png"),
            "b" to SealedWrite.Upload("log.txt", "text/plain"),
        )
        assertEquals(
            "Look\n\n![shot 1.png](/api/attachments/a)\n\n[log.txt](/api/attachments/b)",
            SealedWrite.withAttachmentEmbeds("Look  ", listOf("a", "b", "a"), uploads::get),
        )
        assertEquals("see ![x](/api/attachments/a)", SealedWrite.withAttachmentEmbeds("see ![x](/api/attachments/a)", listOf("a"), uploads::get))
        assertEquals("![image](/api/attachments/z)", SealedWrite.withAttachmentEmbeds("", listOf("z")) { null })
    }

    private val question = JsonObject(mapOf("kind" to JsonPrimitive("question"), "question" to JsonPrimitive("Which?")))
    private val plan = JsonObject(mapOf("kind" to JsonPrimitive("plan"), "plan" to JsonPrimitive("Do it")))

    @Test
    fun aQuestionCarriesItsAnswerAndAnswers() {
        val picked = SealedWrite.answeredCard(question, AnswerInput("m", answers = AnswerInput.questionAnswers(listOf(" Postgres ", "SQLite"))))!!
        assertEquals(JsonPrimitive("Postgres, SQLite"), picked["answer"])
        assertEquals(JsonArray(listOf(JsonPrimitive("Postgres"), JsonPrimitive("SQLite"))), picked["answers"])
        assertEquals(JsonPrimitive("Which?"), picked["question"])
        val typed = SealedWrite.answeredCard(question, AnswerInput("m", answer = " free text "))!!
        assertEquals(JsonPrimitive("free text"), typed["answer"])
        assertNull(SealedWrite.answeredCard(question, AnswerInput("m")))
    }

    @Test
    fun aPlanCarriesItsDecisionFreshFeedbackAndPicks() {
        val approved = SealedWrite.answeredCard(
            plan,
            AnswerInput("m", decision = "approved", fresh = true, feedback = "  ", answers = AnswerInput.planAnswers(mapOf("q1" to listOf("A"), "q2" to listOf(" ")))),
        )!!
        assertEquals(JsonPrimitive("approved"), approved["decision"])
        assertEquals(JsonPrimitive(true), approved["fresh"])
        assertEquals(JsonNull, approved["feedback"])
        assertEquals(JsonObject(mapOf("q1" to JsonArray(listOf(JsonPrimitive("A"))))), approved["answers"])
        val rejected = SealedWrite.answeredCard(plan, AnswerInput("m", decision = "rejected", fresh = true, feedback = "no"))!!
        assertEquals(JsonPrimitive(false), rejected["fresh"])
        assertEquals(JsonPrimitive("no"), rejected["feedback"])
        assertNull(SealedWrite.answeredCard(JsonObject(mapOf("kind" to JsonPrimitive("report"))), AnswerInput("m", decision = "approved")))
    }
}
