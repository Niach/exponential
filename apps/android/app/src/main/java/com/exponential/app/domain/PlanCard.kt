package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull

/** EXP-1280: the surface an agent authored: an Exponential UI core NestedNode
 *  ([root]) + its data model ([data]). */
data class CardSurface(val root: JsonObject, val data: JsonObject?)

/**
 * EXP-1280 custom plan mode: THE plan / question card's behaviour ×4 — the
 * Android mirror of `packages/domain-contract/src/plan-card.ts` (desktop
 * `domain::plan_card`, iOS ExpCore `PlanCard.swift`). The render rules live
 * in fixtures/plan-card.json's `$comment`; `PlanCardTest` replays
 * `answerIssueCases`, `pictureForCases`, `optionRowsCases` and proves the
 * copy / tiles / test ids / stepper below equal the fixture's.
 */
object PlanCard {

    object Copy {
        const val ANSWERS = "Answers"
        const val FEEDBACK = "Feedback"
        const val CHOOSE_ONE = "Choose one"
        const val CHOOSE_ANY = "Choose any"
        const val CUSTOM = "Other…"
        const val SURFACE = "Preview"
        const val ANSWER = "Answer"
        const val ANSWERED = "Answered"
        const val ANSWERING = "Answering…"
        const val FEEDBACK_LINE = "Feedback: {feedback}"
        const val SPECIMEN_PENDING = "A plan waiting on you: pictures, a surface, two questions"
        const val SPECIMEN_ANSWERED = "A question answered with several picks"
        const val SPECIMEN_APPROVED = "An approved plan"
        const val ISSUE_DECISION = "Pick a decision"
        const val ISSUE_MISSING = "Answer every question to approve"
        const val ISSUE_MULTI = "This question takes one answer"
        const val ISSUE_UNKNOWN = "Pick one of the options"
        const val STEP_LABEL = "Question {n} of {m}"
        const val BACK = "Back"
        const val NEXT = "Next"
        const val DECIDE = "Your decision"

        /** The answered card's ONE feedback line. */
        fun feedbackLine(text: String): String = FEEDBACK_LINE.replace("{feedback}", text)
    }

    object TestIds {
        const val CARD = "plan-card"
        const val QUESTION = "plan-card-question"
        const val OPTION = "plan-card-option"
        const val TILE = "plan-card-tile"
        const val CUSTOM = "plan-card-custom"
        const val SURFACE = "plan-card-surface"
        const val FEEDBACK = "plan-card-feedback"
        const val ISSUE = "plan-card-issue"
        const val STEP = "plan-card-step"
        const val STEP_BACK = "plan-card-step-back"
        const val STEP_NEXT = "plan-card-step-next"
        const val DECISION = "plan-card-decision"
    }

    /** NEW-6 (N-1): the question stepper (fixture `stepper`; dp). */
    object Stepper {
        const val MIN_QUESTIONS = 2
        const val BUTTON_GAP = 8
    }

    /** NEW-6 (N-1): whether a card shows its questions one at a time: a
     *  PENDING plan (no decision yet) with [Stepper.MIN_QUESTIONS] or more
     *  questions. A question card, a decided plan and a plan with fewer
     *  questions never do (web `stepperApplies`). */
    fun stepperApplies(isPlan: Boolean, decision: String?, questionCount: Int): Boolean {
        if (!isPlan) return false
        if (decision == "approved" || decision == "rejected") return false
        return questionCount >= Stepper.MIN_QUESTIONS
    }

    /** The step line `copy.stepLabel`: [step] 1-based of [count]. */
    fun stepLabel(step: Int, count: Int): String =
        Copy.STEP_LABEL.replace("{n}", step.toString()).replace("{m}", count.toString())

    /** The tile geometry (dp; [PICTURE_ASPECT] = width / height). */
    object Tiles {
        const val COLUMNS = 2
        const val PHONE_COLUMNS = 2
        const val GAP = 8
        const val PICTURE_ASPECT = 1.6f
        const val RADIUS = 10
        const val SELECTED_RING = 2
        const val PADDING = 8
        const val PICTURE_RADIUS = 6
        const val STROKE = 1
        const val PLACEHOLDER_GLYPH = 20
        const val LABEL_GAP = 4
        const val INERT_OPACITY = 0.5f
    }

    /** The rest of the card's ONE look (dp; fixture `card`). */
    object Look {
        const val PICTURE_RADIUS = 12
        const val PICTURE_STROKE = 1
        const val SURFACE_RADIUS = 12
        const val SURFACE_PADDING = 12
        const val SURFACE_LABEL_GAP = 6
        const val BLOCK_GAP = 12
        const val BLOCK_INNER_GAP = 2
        const val OPTIONS_GAP = 6
        const val ROW_MIN_HEIGHT = 32
        const val ROW_RADIUS = 10
        const val FIELD_MIN_HEIGHT = 36
        const val INERT_OPACITY = 0.5f
        const val LIST_INDENT = 24
        const val HEADING_SCALE = 1.2f
    }

    const val UPLOAD_SCHEME = "upload:"

    /** Why an answer cannot be sent (fixture `copy.answerIssues`). */
    enum class AnswerIssue(val wire: String, val text: String) {
        Decision("decision", Copy.ISSUE_DECISION),
        Missing("missing", Copy.ISSUE_MISSING),
        Multi("multi", Copy.ISSUE_MULTI),
        Unknown("unknown", Copy.ISSUE_UNKNOWN),
    }

    data class Picture(val label: String, val attachmentId: String?, val width: Int?, val height: Int?)

    /** The card an answer is checked against. */
    sealed interface Card {
        data class Plan(val questions: List<SessionTree.ThreadRow.PlanQuestion>) : Card
        data class Question(val options: List<String>, val multi: Boolean, val custom: Boolean) : Card
    }

    /** What the person sends: a plan's [decision] + [planAnswers]; a
     *  question's [answers] (picked labels) else the free-text [answer]. */
    data class Input(
        val decision: String? = null,
        val planAnswers: Map<String, List<String>>? = null,
        val answers: List<String>? = null,
        val answer: String? = null,
    )

    /** Why an answer cannot be sent (null = fine). A plan needs a decision;
     *  approving needs every question answered; a single-choice question
     *  takes one answer; without `custom` every answer must be an option's
     *  label. A question card needs one answer. */
    fun answerIssue(card: Card, input: Input): AnswerIssue? = when (card) {
        is Card.Plan -> planIssue(card, input)
        is Card.Question -> {
            val picked = input.answers?.map { it.trim() }?.filter { it.isNotEmpty() }
                ?: listOfNotNull(input.answer?.trim()?.takeIf { it.isNotEmpty() })
            if (picked.isEmpty()) AnswerIssue.Missing else optionIssue(card.options, card.multi, card.custom, picked)
        }
    }

    private fun planIssue(card: Card.Plan, input: Input): AnswerIssue? {
        if (input.decision != "approved" && input.decision != "rejected") return AnswerIssue.Decision
        val answers = input.planAnswers.orEmpty()
        if (answers.keys.any { id -> card.questions.none { it.id == id } }) return AnswerIssue.Unknown
        for (question in card.questions) {
            val picked = answers[question.id].orEmpty().map { it.trim() }.filter { it.isNotEmpty() }
            if (picked.isEmpty()) {
                if (input.decision == "approved") return AnswerIssue.Missing
                continue
            }
            optionIssue(question.options.map { it.label }, question.multi, question.custom, picked)?.let { return it }
        }
        return null
    }

    private fun optionIssue(labels: List<String>, multi: Boolean, custom: Boolean, picked: List<String>): AnswerIssue? {
        if (!multi && picked.size > 1) return AnswerIssue.Multi
        if (!custom && picked.any { label -> labels.none { it.trim() == label } }) return AnswerIssue.Unknown
        return null
    }

    /** A card's `pictures` (the upload namespace) in a tolerant read. */
    fun cardPictures(raw: JsonElement?): List<Picture> = (raw as? JsonArray).orEmpty().mapNotNull { entry ->
        val picture = entry as? JsonObject ?: return@mapNotNull null
        val label = (picture["label"] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return@mapNotNull null
        Picture(
            label = label,
            attachmentId = (picture["attachmentId"] as? JsonPrimitive)?.takeIf { it.isString }?.content,
            width = (picture["width"] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull?.toInt(),
            height = (picture["height"] as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull?.toInt(),
        )
    }

    /** The attachment a picture label resolved to (null = none / not yet). */
    fun pictureFor(pictures: List<Picture>?, label: String?): String? {
        if (label.isNullOrEmpty() || pictures == null) return null
        return pictures.firstOrNull { it.label == label }?.attachmentId
    }

    enum class OptionRows(val wire: String) { List("list"), Tiles("tiles") }

    /** Text-only options = THE option list; any option with a picture = tiles. */
    fun optionRows(attachmentIds: kotlin.collections.List<String?>): OptionRows =
        if (attachmentIds.any { !it.isNullOrEmpty() }) OptionRows.Tiles else OptionRows.List

    fun optionRows(options: Iterable<SessionTree.ThreadRow.QuestionChoice>): OptionRows =
        optionRows(options.map { it.attachmentId })

    /** The question card's compat `answer` string for a list of picks. */
    fun joinAnswers(answers: kotlin.collections.List<String>): String = answers.joinToString(", ")

    /** A markdown image whose source is still `upload:<label>` renders
     *  NOTHING until the upload lands: such embeds are dropped. */
    private val UPLOAD_IMAGE = Regex("""!\[[^\]]*\]\(\s*upload:[^)]*\)""")

    fun withoutPendingUploads(markdown: String): String =
        if (!markdown.contains(UPLOAD_SCHEME)) markdown else UPLOAD_IMAGE.replace(markdown, "")
}
