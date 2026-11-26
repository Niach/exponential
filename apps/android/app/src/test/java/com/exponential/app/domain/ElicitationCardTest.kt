package com.exponential.app.domain

import java.io.File as JavaFile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/** #NEW-146: `fixtures/elicitation-card.json` replayed: `missingCases`,
 *  `actionCases`, `feedCardCases` (whole values), plus the copy and test ids it pins. */
class ElicitationCardTest {
    private val fixture: JsonObject = Json.parseToJsonElement(fixtureJson()).jsonObject

    @Test
    fun missingCases() {
        val cases = fixture.getValue("missingCases").jsonArray
        assert(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val required = case.getValue("required").jsonArray.map { it.jsonPrimitive.content }
            val data = case["surfaceData"]?.takeUnless { it is JsonNull } as? JsonObject
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.getValue("name").jsonPrimitive.content, expected, ElicitationCard.missing(required, data))
        }
    }

    @Test
    fun actionCases() {
        val cases = fixture.getValue("actionCases").jsonArray
        assert(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val card = case.getValue("card").jsonObject
            val answers = card["answers"]?.takeUnless { it is JsonNull }?.jsonArray?.map { it.jsonPrimitive.content }
            val answer = (card["answer"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            val dismissed = (card["dismissed"] as? JsonPrimitive)?.content == "true"
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("expected").jsonPrimitive.content,
                ElicitationCard.action(answers, answer, dismissed).wire,
            )
        }
    }

    /** LIVE: a frame draws THE posted card (`feedCardCases`). */
    @Test
    fun feedCardCases() {
        val cases = fixture.getValue("feedCardCases").jsonArray
        assert(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val messages = case.getValue("messages").jsonArray.map { raw ->
                val m = raw.jsonObject
                SessionTree.Message(
                    id = m.getValue("id").jsonPrimitive.content,
                    sessionId = "s1",
                    authorKind = "agent",
                    authorId = null,
                    body = "",
                    card = m["card"] as? JsonObject,
                    answeredAt = (m["answeredAt"] as? JsonPrimitive)?.takeIf { it.isString }?.content,
                    consumedAt = null,
                    createdAt = "2026-11-26T16:00:00.000Z",
                )
            }
            val expected = (case["expected"] as? JsonPrimitive)?.takeIf { it.isString }?.content
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                expected,
                ElicitationCard.feedCard(
                    messages,
                    case.getValue("questionId").jsonPrimitive.content,
                    case.getValue("resolved").jsonPrimitive.content == "true",
                ),
            )
        }
    }

    @Test
    fun copyAndTestIds() {
        val copy = fixture.getValue("copy").jsonObject
        fun c(key: String) = copy.getValue(key).jsonPrimitive.content
        assertEquals(c("submit"), ElicitationCard.Copy.SUBMIT)
        assertEquals(c("decline"), ElicitationCard.Copy.DECLINE)
        assertEquals(c("cancel"), ElicitationCard.Copy.CANCEL)
        assertEquals(c("openLink"), ElicitationCard.Copy.OPEN_LINK)
        assertEquals(c("done"), ElicitationCard.Copy.DONE)
        assertEquals(c("missing"), ElicitationCard.Copy.MISSING)
        assertEquals(c("unknownServer"), ElicitationCard.Copy.UNKNOWN_SERVER)
        val specimen = copy.getValue("specimen").jsonObject
        fun s(key: String) = specimen.getValue(key).jsonPrimitive.content
        assertEquals(s("form"), ElicitationCard.Copy.Specimen.FORM)
        assertEquals(s("url"), ElicitationCard.Copy.Specimen.URL)
        assertEquals(s("warned"), ElicitationCard.Copy.Specimen.WARNED)
        assertEquals(s("answered"), ElicitationCard.Copy.Specimen.ANSWERED)
        assertEquals(fixture.getValue("formKey").jsonPrimitive.content, ElicitationCard.FORM_KEY)
        val ids = fixture.getValue("testIds").jsonObject
        fun t(key: String) = ids.getValue(key).jsonPrimitive.content
        assertEquals(t("card"), ElicitationCard.TestIds.CARD)
        assertEquals(t("server"), ElicitationCard.TestIds.SERVER)
        assertEquals(t("submit"), ElicitationCard.TestIds.SUBMIT)
        assertEquals(t("decline"), ElicitationCard.TestIds.DECLINE)
        assertEquals(t("cancel"), ElicitationCard.TestIds.CANCEL)
        assertEquals(t("openLink"), ElicitationCard.TestIds.OPEN_LINK)
        assertEquals(t("done"), ElicitationCard.TestIds.DONE)
    }

    /** The styleguide specimen's cards are the fixture's, byte for byte. */
    @Test
    fun specimenCardsAreTheFixtures() {
        fun card(prefix: String) = fixture.getValue("cardCases").jsonArray
            .map { it.jsonObject }
            .first { it.getValue("name").jsonPrimitive.content.startsWith(prefix) }
            .getValue("expected")
        assertEquals(card("a form with every field kind"), Json.parseToJsonElement(com.exponential.app.ui.styleguide.ELICITATION_SPECIMEN_FORM))
        assertEquals(card("a url: the domain"), Json.parseToJsonElement(com.exponential.app.ui.styleguide.ELICITATION_SPECIMEN_URL))
        assertEquals(card("a url on http"), Json.parseToJsonElement(com.exponential.app.ui.styleguide.ELICITATION_SPECIMEN_WARNED))
    }

    /** Every form `cardCases` card parses into its row's sidecar. */
    @Test
    fun cardCasesCarryTheSidecar() {
        fixture.getValue("cardCases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val card = case["expected"]?.takeUnless { it is JsonNull } as? JsonObject ?: return@forEach
            val raw = card.getValue("elicitation").jsonObject
            val sidecar = ElicitationCard.sidecar(raw)
            val name = case.getValue("name").jsonPrimitive.content
            assertEquals(name, raw.getValue("server").jsonPrimitive.content, sidecar?.server)
            assertEquals(name, raw.getValue("mode").jsonPrimitive.content, sidecar?.mode?.wire)
            assertEquals(name, raw["required"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty(), sidecar?.required)
        }
    }
}

private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/elicitation-card.json",
        "../../packages/domain-contract/fixtures/elicitation-card.json",
        "packages/domain-contract/fixtures/elicitation-card.json",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("elicitation-card.json not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
