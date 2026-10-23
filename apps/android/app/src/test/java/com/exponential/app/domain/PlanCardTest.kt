package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1280: fixtures/plan-card.json ×4 — every `answerIssueCases`,
 * `pictureForCases` and `optionRowsCases` entry runs here (TS
 * `plan-card.test.ts`, desktop `domain::plan_card`, iOS `PlanCardTests`);
 * the embedded copy / test ids / tile geometry equal the fixture's.
 */
class PlanCardTest {

    private val fixture by lazy { Json.parseToJsonElement(contractFixtureJson("plan-card.json")).jsonObject }

    private fun cases(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.str(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun card(json: JsonObject): PlanCard.Card = when (json.str("kind")) {
        "plan" -> PlanCard.Card.Plan(SessionTree.planQuestions(json))
        else -> PlanCard.Card.Question(
            options = SessionTree.questionOptions(json).map { it.label },
            multi = (json["multi"] as? JsonPrimitive)?.boolean == true,
            custom = (json["custom"] as? JsonPrimitive)?.boolean != false,
        )
    }

    private fun input(json: JsonObject): PlanCard.Input {
        val answers = json["answers"]
        return PlanCard.Input(
            decision = json.str("decision"),
            planAnswers = (answers as? JsonObject)?.mapValues { (_, v) -> v.jsonArray.map { it.jsonPrimitive.content } },
            answers = (answers as? JsonArray)?.map { it.jsonPrimitive.content },
            answer = json.str("answer"),
        )
    }

    @Test
    fun `every answer issue case matches the fixture`() {
        val cases = cases("answerIssueCases")
        assertTrue(cases.size >= 10)
        for (case in cases) {
            val actual = PlanCard.answerIssue(card(case.getValue("card").jsonObject), input(case.getValue("input").jsonObject))
            assertEquals(case.str("name"), case.str("expected"), actual?.wire)
        }
    }

    @Test
    fun `every picture-for case matches the fixture`() {
        for (case in cases("pictureForCases")) {
            val raw = case["pictures"]
            val pictures = if (raw == null || raw is JsonNull) null else PlanCard.cardPictures(raw)
            assertEquals(case.str("name"), case.str("expected"), PlanCard.pictureFor(pictures, case.str("label")))
        }
    }

    @Test
    fun `every option rows case matches the fixture`() {
        for (case in cases("optionRowsCases")) {
            val ids = case.getValue("options").jsonArray.map { it.jsonObject.str("attachmentId") }
            assertEquals(case.str("name"), case.str("expected"), PlanCard.optionRows(ids).wire)
        }
    }

    @Test
    fun `the copy, test ids and tiles equal the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("answers"), PlanCard.Copy.ANSWERS)
        assertEquals(copy.str("feedback"), PlanCard.Copy.FEEDBACK)
        assertEquals(copy.str("chooseOne"), PlanCard.Copy.CHOOSE_ONE)
        assertEquals(copy.str("chooseAny"), PlanCard.Copy.CHOOSE_ANY)
        assertEquals(copy.str("custom"), PlanCard.Copy.CUSTOM)
        assertEquals(copy.str("surface"), PlanCard.Copy.SURFACE)
        assertEquals(copy.str("answer"), PlanCard.Copy.ANSWER)
        assertEquals(copy.str("answered"), PlanCard.Copy.ANSWERED)
        assertEquals(copy.str("answering"), PlanCard.Copy.ANSWERING)
        assertEquals(copy.str("feedbackLine"), PlanCard.Copy.FEEDBACK_LINE)
        assertEquals("Feedback: Go.", PlanCard.Copy.feedbackLine("Go."))
        assertEquals(copy.str("stepLabel"), PlanCard.Copy.STEP_LABEL)
        assertEquals(copy.str("back"), PlanCard.Copy.BACK)
        assertEquals(copy.str("next"), PlanCard.Copy.NEXT)
        assertEquals(copy.str("decide"), PlanCard.Copy.DECIDE)
        assertEquals(copy.str("cancel"), PlanCard.Copy.CANCEL)
        val specimen = copy.getValue("specimen").jsonObject
        assertEquals(
            specimen.mapValues { it.value.jsonPrimitive.content },
            mapOf(
                "pending" to PlanCard.Copy.SPECIMEN_PENDING,
                "answered" to PlanCard.Copy.SPECIMEN_ANSWERED,
                "approved" to PlanCard.Copy.SPECIMEN_APPROVED,
            ),
        )
        val issues = copy.getValue("answerIssues").jsonObject
        for (issue in PlanCard.AnswerIssue.entries) assertEquals(issues.str(issue.wire), issue.text)
        assertEquals(issues.keys, PlanCard.AnswerIssue.entries.map { it.wire }.toSet())
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(
            ids.mapValues { it.value.jsonPrimitive.content },
            mapOf(
                "card" to PlanCard.TestIds.CARD,
                "question" to PlanCard.TestIds.QUESTION,
                "option" to PlanCard.TestIds.OPTION,
                "tile" to PlanCard.TestIds.TILE,
                "custom" to PlanCard.TestIds.CUSTOM,
                "surface" to PlanCard.TestIds.SURFACE,
                "feedback" to PlanCard.TestIds.FEEDBACK,
                "issue" to PlanCard.TestIds.ISSUE,
                "step" to PlanCard.TestIds.STEP,
                "stepBack" to PlanCard.TestIds.STEP_BACK,
                "stepNext" to PlanCard.TestIds.STEP_NEXT,
                "decision" to PlanCard.TestIds.DECISION,
            ),
        )
        assertNumbers(
            fixture.getValue("tiles").jsonObject,
            mapOf(
                "columns" to PlanCard.Tiles.COLUMNS,
                "phoneColumns" to PlanCard.Tiles.PHONE_COLUMNS,
                "gap" to PlanCard.Tiles.GAP,
                "pictureAspect" to PlanCard.Tiles.PICTURE_ASPECT,
                "radius" to PlanCard.Tiles.RADIUS,
                "selectedRing" to PlanCard.Tiles.SELECTED_RING,
                "padding" to PlanCard.Tiles.PADDING,
                "pictureRadius" to PlanCard.Tiles.PICTURE_RADIUS,
                "stroke" to PlanCard.Tiles.STROKE,
                "placeholderGlyph" to PlanCard.Tiles.PLACEHOLDER_GLYPH,
                "labelGap" to PlanCard.Tiles.LABEL_GAP,
                "inertOpacity" to PlanCard.Tiles.INERT_OPACITY,
            ),
        )
        assertNumbers(
            fixture.getValue("card").jsonObject,
            mapOf(
                "pictureRadius" to PlanCard.Look.PICTURE_RADIUS,
                "pictureStroke" to PlanCard.Look.PICTURE_STROKE,
                "surfaceRadius" to PlanCard.Look.SURFACE_RADIUS,
                "surfacePadding" to PlanCard.Look.SURFACE_PADDING,
                "surfaceLabelGap" to PlanCard.Look.SURFACE_LABEL_GAP,
                "blockGap" to PlanCard.Look.BLOCK_GAP,
                "blockInnerGap" to PlanCard.Look.BLOCK_INNER_GAP,
                "optionsGap" to PlanCard.Look.OPTIONS_GAP,
                "rowMinHeight" to PlanCard.Look.ROW_MIN_HEIGHT,
                "rowRadius" to PlanCard.Look.ROW_RADIUS,
                "fieldMinHeight" to PlanCard.Look.FIELD_MIN_HEIGHT,
                "inertOpacity" to PlanCard.Look.INERT_OPACITY,
                "listIndent" to PlanCard.Look.LIST_INDENT,
                "headingScale" to PlanCard.Look.HEADING_SCALE,
            ),
        )
        assertEquals(fixture.str("uploadScheme"), PlanCard.UPLOAD_SCHEME)
        assertNumbers(
            JsonObject(fixture.getValue("stepper").jsonObject.filterKeys { !it.startsWith("$") }),
            mapOf(
                "minQuestions" to PlanCard.Stepper.MIN_QUESTIONS,
                "buttonGap" to PlanCard.Stepper.BUTTON_GAP,
            ),
        )
    }

    /** NEW-6 (N-1): only a PENDING plan with `stepper.minQuestions`+
     *  questions steps; the step line fills `copy.stepLabel`. */
    @Test
    fun `the stepper applies to a pending plan with enough questions`() {
        val min = PlanCard.Stepper.MIN_QUESTIONS
        assertEquals(true, PlanCard.stepperApplies(isPlan = true, decision = null, questionCount = min))
        assertEquals(false, PlanCard.stepperApplies(isPlan = true, decision = null, questionCount = min - 1))
        assertEquals(false, PlanCard.stepperApplies(isPlan = true, decision = "approved", questionCount = min))
        assertEquals(false, PlanCard.stepperApplies(isPlan = true, decision = "rejected", questionCount = min))
        assertEquals(false, PlanCard.stepperApplies(isPlan = false, decision = null, questionCount = min))
        assertEquals("Question 1 of 2", PlanCard.stepLabel(1, 2))
    }

    /** Every key of the fixture object has a mirror, and each equals it. */
    private fun assertNumbers(json: JsonObject, mirror: Map<String, Number>) {
        assertEquals(json.keys, mirror.keys)
        for ((key, value) in json) {
            assertEquals(key, value.jsonPrimitive.double, mirror.getValue(key).toDouble(), 1e-6)
        }
    }

    @Test
    fun `a pending upload embed renders nothing`() {
        assertEquals("A\n\n\n\nB", PlanCard.withoutPendingUploads("A\n\n![after](upload:after)\n\nB"))
        assertEquals("![x](/api/attachments/a)", PlanCard.withoutPendingUploads("![x](/api/attachments/a)"))
    }
}
