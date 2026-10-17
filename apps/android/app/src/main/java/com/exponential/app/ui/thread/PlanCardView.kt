package com.exponential.app.ui.thread

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import at.exponential.ui.primitives.ChoiceTile
import at.exponential.ui.primitives.ChoiceTiles
import at.exponential.ui.primitives.ChoiceTilesStyle
import coil3.compose.AsyncImage
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.CardSurface
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.locksCard
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.session.AnsweredRow
import com.exponential.app.ui.session.AskCard
import com.exponential.app.ui.session.LocalAskDockCard
import com.exponential.app.ui.session.QuestionOptionButton
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.serialization.json.JsonObject

/** EXP-1280: a question card's answer — [answer] = the compat string (the
 *  picks joined by ", ", or the free text), [answers] the picked labels,
 *  [surfaceData] the inline surface's data model. */
data class AskAnswer(
    val answer: String,
    val answers: List<String>? = null,
    val surfaceData: JsonObject? = null,
)

/** EXP-1280: a plan card's decision (ask-dock.json `planOptions`) + the
 *  questions' picks by id, the feedback, the surface's data model. */
data class PlanAnswer(
    val approved: Boolean,
    val fresh: Boolean,
    val feedback: String?,
    val answers: Map<String, List<String>>? = null,
    val surfaceData: JsonObject? = null,
)

/** One choice question in the card's form (a plan question, or the
 *  question card's own options under the id [QUESTION_CARD_ID]). */
private data class ChoiceBlock(
    val id: String,
    val prompt: String?,
    val multi: Boolean,
    val custom: Boolean,
    val options: List<SessionTree.ThreadRow.QuestionChoice>,
)

private const val QUESTION_CARD_ID = "question"

/**
 * THE plan / question card ×4 (fixtures/plan-card.json; web
 * `ThreadPlanCard` / `ThreadQuestionCard`): the AskCard header, the markdown
 * body (pictures inline; a still-uploading `upload:` image renders nothing),
 * the inline surface, one block per question (prompt + Choose one / Choose
 * any + THE option rows, or picture TILES once any option has a picture, +
 * the `Other…` free-text row), then a plan's decision rows (ask-dock.json
 * `planOptions`) + the feedback field, or a question's Answer button.
 * Approving waits for every question ([PlanCard.answerIssue] = missing);
 * rejecting never does. ANSWERED = closed: the picks stay highlighted, the
 * rest inert, the decision + feedback shown. Answers go out through
 * `sessions.answer` with `answers` + `surfaceData`.
 */
@Composable
internal fun PlanAskCard(
    row: SessionTree.ThreadRow,
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    modifier: Modifier = Modifier,
) {
    val plan = row as? SessionTree.ThreadRow.Plan
    val question = row as? SessionTree.ThreadRow.Question
    if (plan == null && question == null) return
    var state by remember(row.id) { mutableStateOf<AnswerState?>(null) }
    val settle: (Boolean) -> Unit = { ok -> state = if (ok) AnswerState.Acked else null }
    val answered = plan?.decision != null || question?.answered == true
    val locked = state.locksCard()
    val live = !answered && !locked

    val blocks: List<ChoiceBlock> = remember(row) {
        plan?.questions?.map { ChoiceBlock(it.id, it.prompt, it.multi, it.custom, it.options) }
            ?: listOf(ChoiceBlock(QUESTION_CARD_ID, null, question!!.multi, question.custom, question.options))
    }
    // The recorded answers (a closed card), else what this viewer picked.
    val recorded: Map<String, List<String>> = remember(row) {
        plan?.answers ?: question?.let { q ->
            val labels = q.answers ?: q.answer?.let { a ->
                val parts = if (q.multi) a.split(", ") else listOf(a)
                if (parts.all { p -> q.options.any { it.label == p } }) parts else listOf(a)
            }
            labels?.let { mapOf(QUESTION_CARD_ID to it) }
        } ?: emptyMap()
    }
    val picks = remember(row.id) { mutableStateMapOf<String, Set<String>>() }
    val customOpen = remember(row.id) { mutableStateMapOf<String, Boolean>() }
    val customText = remember(row.id) { mutableStateMapOf<String, String>() }
    var feedback by remember(row.id) { mutableStateOf("") }
    val surfaceHandle = remember(row.id) { CardSurfaceHandle() }

    /** A block's answer now: its picks in option order, + the typed text. */
    fun answerOf(block: ChoiceBlock): List<String> {
        val picked = block.options.map { it.label }.filter { it in picks[block.id].orEmpty() }
        val typed = customText[block.id]?.trim()?.takeIf { customOpen[block.id] == true && it.isNotEmpty() }
        return picked + listOfNotNull(typed)
    }

    fun toggle(block: ChoiceBlock, label: String) {
        val current = picks[block.id].orEmpty()
        picks[block.id] = when {
            label in current -> current - label
            block.multi -> current + label
            else -> setOf(label)
        }
        // One answer unless multi: a pick closes the free-text row.
        if (!block.multi) customOpen[block.id] = false
    }

    val docked = LocalAskDockCard.current
    AskCard(
        plan = plan != null,
        heading = if (plan != null) AskDock.Copy.PLAN_READY else AskDock.Copy.QUESTION,
        meta = null,
        modifier = modifier.testTag(PlanCard.TestIds.CARD),
        onBack = docked?.onBack,
    ) {
        val body = @Composable {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                MarkdownView(PlanCard.withoutPendingUploads(plan?.plan ?: question!!.question))
                val surface: CardSurface? = plan?.surface ?: question?.surface
                if (surface != null) CardSurfaceView(surface, surfaceHandle, inert = !live)
                blocks.forEach { block ->
                    if (plan == null && block.options.isEmpty() && !block.custom) return@forEach
                    ChoiceBlockView(
                        block = block,
                        picked = if (answered) recorded[block.id].orEmpty().toSet() else picks[block.id].orEmpty(),
                        typed = if (answered) {
                            recorded[block.id].orEmpty().firstOrNull { label -> block.options.none { it.label == label } }
                        } else {
                            null
                        },
                        answered = answered,
                        enabled = live,
                        customOpen = customOpen[block.id] == true,
                        customText = customText[block.id].orEmpty(),
                        onToggle = { toggle(block, it) },
                        onCustomOpen = {
                            val open = customOpen[block.id] != true
                            customOpen[block.id] = open
                            if (open && !block.multi) picks[block.id] = emptySet()
                        },
                        onCustomText = { customText[block.id] = it },
                    )
                }
            }
        }
        if (docked != null) {
            Box(Modifier.fillMaxWidth().heightIn(max = docked.bodyMaxHeight).verticalScroll(rememberScrollState())) { body() }
        } else {
            body()
        }

        if (answered) {
            Column(Modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (plan != null) {
                    AnsweredRow(AskDock.planDecisionLabel(plan.decision!!, plan.fresh))
                    plan.feedback?.let {
                        Text(
                            "${PlanCard.Copy.FEEDBACK}: $it",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        )
                    }
                } else {
                    AnsweredRow(question!!.answer)
                }
            }
        } else if (plan != null) {
            val answers = blocks.associate { it.id to answerOf(it) }
            val issue = PlanCard.answerIssue(
                PlanCard.Card.Plan(plan.questions),
                PlanCard.Input(decision = "approved", planAnswers = answers),
            )
            Column(Modifier.fillMaxWidth().padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                AskDock.PLAN_OPTIONS.forEachIndexed { index, option ->
                    val approve = option.decision == "approved"
                    QuestionOptionButton(
                        option = QuestionOption(option.label, option.key, description = option.description),
                        ordinal = index + 1,
                        primary = index == 0 && issue == null,
                        selected = false,
                        checked = null,
                        enabled = live && (!approve || issue == null),
                        dimmed = locked || (approve && issue != null),
                        onClick = {
                            state = AnswerState.Sending
                            onDecidePlan(
                                plan.id,
                                PlanAnswer(
                                    approved = approve,
                                    fresh = option.fresh,
                                    feedback = feedback.trim().takeIf { it.isNotEmpty() },
                                    answers = answers.filterValues { it.isNotEmpty() }.takeIf { it.isNotEmpty() },
                                    surfaceData = surfaceHandle.snapshot(),
                                ),
                                settle,
                            )
                        },
                    )
                }
                if (issue == PlanCard.AnswerIssue.Missing) {
                    Text(
                        issue.text,
                        modifier = Modifier.testTag(PlanCard.TestIds.ISSUE),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    )
                }
                GlassTextField(
                    value = feedback,
                    onValueChange = { feedback = it },
                    modifier = Modifier.fillMaxWidth().testTag(PlanCard.TestIds.FEEDBACK),
                    placeholder = AskDock.Copy.FEEDBACK_PLACEHOLDER,
                    enabled = live,
                    maxLines = 4,
                )
            }
        } else {
            val block = blocks.first()
            val labels = answerOf(block)
            val issue = PlanCard.answerIssue(
                PlanCard.Card.Question(block.options.map { it.label }, block.multi, block.custom),
                PlanCard.Input(answers = labels),
            )
            Box(Modifier.fillMaxWidth().padding(top = 4.dp)) {
                QuestionOptionButton(
                    option = QuestionOption(PlanCard.Copy.ANSWER, "answer"),
                    ordinal = 0,
                    primary = issue == null,
                    selected = false,
                    checked = null,
                    enabled = live && issue == null,
                    dimmed = locked,
                    onClick = {
                        state = AnswerState.Sending
                        onAnswer(
                            question!!.id,
                            AskAnswer(PlanCard.joinAnswers(labels), labels, surfaceHandle.snapshot()),
                            settle,
                        )
                    },
                )
            }
        }
        if (locked && !answered) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (state == AnswerState.Sending) {
                    CircularProgressIndicator(modifier = Modifier.size(12.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onSurface)
                }
                Text(
                    if (state == AnswerState.Sending) "Sending your answer…" else "Answer sent. Waiting for the agent.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                )
            }
        }
    }
}

/** One question: prompt (strong) + Choose one / Choose any, the options as
 *  THE option rows or as picture tiles, + the `Other…` row. */
@Composable
private fun ChoiceBlockView(
    block: ChoiceBlock,
    picked: Set<String>,
    typed: String?,
    answered: Boolean,
    enabled: Boolean,
    customOpen: Boolean,
    customText: String,
    onToggle: (String) -> Unit,
    onCustomOpen: () -> Unit,
    onCustomText: (String) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().testTag(PlanCard.TestIds.QUESTION),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (block.prompt != null) {
            Text(
                block.prompt,
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        Text(
            if (block.multi) PlanCard.Copy.CHOOSE_ANY else PlanCard.Copy.CHOOSE_ONE,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        when (PlanCard.optionRows(block.options)) {
            PlanCard.OptionRows.List -> block.options.forEachIndexed { index, option ->
                val on = option.label in picked
                Box(Modifier.testTag(PlanCard.TestIds.OPTION)) {
                    QuestionOptionButton(
                        option = QuestionOption(option.label, "$index:${option.label}", description = option.description),
                        // The number keys belong to the decision rows.
                        ordinal = 0,
                        primary = false,
                        selected = on,
                        checked = if (block.multi) on else on.takeIf { it },
                        enabled = enabled,
                        dimmed = answered && !on,
                        onClick = { onToggle(option.label) },
                    )
                }
            }
            PlanCard.OptionRows.Tiles -> PlanCardTiles(block, picked, answered, enabled, onToggle)
        }
        if (answered) {
            if (typed != null) {
                QuestionOptionButton(
                    option = QuestionOption(typed, "custom"),
                    ordinal = 0,
                    primary = false,
                    selected = true,
                    checked = true,
                    enabled = false,
                    dimmed = false,
                    onClick = {},
                )
            }
        } else if (block.custom) {
            Box(Modifier.testTag(PlanCard.TestIds.CUSTOM)) {
                QuestionOptionButton(
                    option = QuestionOption(PlanCard.Copy.CUSTOM, "custom", freeText = true),
                    ordinal = 0,
                    primary = false,
                    selected = customOpen,
                    checked = if (block.multi) customOpen else customOpen.takeIf { it },
                    enabled = enabled,
                    dimmed = false,
                    onClick = onCustomOpen,
                )
            }
            if (customOpen) {
                GlassTextField(
                    value = customText,
                    onValueChange = onCustomText,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = com.exponential.app.domain.FREE_TEXT_ANSWER_PLACEHOLDER,
                    enabled = enabled,
                    maxLines = 4,
                )
            }
        }
    }
}

/** The options as picture tiles: THE `ChoiceTiles` primitive in the
 *  plan-card.json geometry (phones: `tiles.phoneColumns`). */
@Composable
private fun PlanCardTiles(
    block: ChoiceBlock,
    picked: Set<String>,
    answered: Boolean,
    enabled: Boolean,
    onToggle: (String) -> Unit,
) {
    val onSurface = MaterialTheme.colorScheme.onSurface
    val style = ChoiceTilesStyle(
        columns = PlanCard.Tiles.PHONE_COLUMNS,
        gap = PlanCard.Tiles.GAP.dp,
        radius = PlanCard.Tiles.RADIUS.dp,
        padding = PlanCard.Tiles.PADDING.dp,
        pictureAspect = PlanCard.Tiles.PICTURE_ASPECT,
        selectedRing = PlanCard.Tiles.SELECTED_RING.dp,
        tileFill = GlassTokens.RowFill,
        tileFillSelected = GlassTokens.RowFillActive,
        stroke = GlassTokens.StrokeCard,
        strokeWidth = GlassTokens.Hairline,
        accent = MaterialTheme.colorScheme.primary,
        pictureFill = GlassTokens.RowFill,
        label = onSurface,
        description = onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
    ChoiceTiles(
        tiles = block.options.map { option ->
            ChoiceTile(
                label = option.label,
                description = option.description,
                hasPicture = option.attachmentId != null,
                // Closed: only the picks stay live-looking, the rest inert.
                disabled = answered && option.label !in picked,
                testTag = PlanCard.TestIds.TILE,
            )
        },
        selected = block.options.indices.filter { block.options[it].label in picked }.toSet(),
        onToggle = { index -> onToggle(block.options[index].label) },
        multiple = block.multi,
        enabled = enabled,
        style = style,
        picture = { index ->
            AsyncImage(
                model = "/api/attachments/${block.options[index].attachmentId}",
                contentDescription = block.options[index].label,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        },
        placeholder = {
            Icon(
                ExpIcons.editorImage,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
                tint = onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
    )
}
