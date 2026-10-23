package com.exponential.app.ui.thread

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import at.exponential.ui.primitives.LocalPrimitiveTokens
import at.exponential.ui.primitives.fieldChrome
import com.exponential.app.ui.markdown.LocalInlineCodeStyle
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.session.TranscriptBodyStyle
import com.exponential.app.ui.theme.AppPrimitiveTokens
import com.exponential.app.ui.theme.DesignTokens
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
import androidx.compose.runtime.mutableIntStateOf
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
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.session.AnsweredRow
import com.exponential.app.ui.session.AskCard
import com.exponential.app.ui.components.DockSheetBody
import com.exponential.app.ui.components.DockSheetFooter
import com.exponential.app.ui.components.DockSheetScope
import com.exponential.app.ui.components.GlassHostMenu
import com.exponential.app.ui.components.GlassMenuItem
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import com.exponential.app.ui.session.QuestionOptionButton
import com.exponential.app.ui.theme.GlassTokens
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
 * `ThreadPlanCard` / `ThreadQuestionCard`): the AskCard header (a plan's
 * "Plan ready"; a question carries only its glyph), the markdown body at the
 * chat measure (pictures inline; a still-uploading `upload:` image renders
 * nothing), the inline surface, one block per question (prompt + Choose one
 * / Choose any + THE option rows, or picture TILES once any option has a
 * picture, + THE `Other…` field), or a question's small Answer button.
 * N-12 (plan-card.json `stepper`): a PENDING card the viewer decides opens
 * as THE DockCard sheet ([sheet]): page 0 = the plan (+ its questions below
 * [PlanCard.Stepper.MIN_QUESTIONS]), else one page per question ("Question
 * n of m" as the header's meta), the footer Back / Next and on the last page
 * THE decision ([PlanSheetFooter]); [initialStep] seeds the page (local,
 * never synced). Approving waits for every question ([PlanCard.answerIssue]);
 * rejecting never does. In the transcript a pending plan reads (no
 * decision); ANSWERED = closed: the picks keep an emerald check, the rest
 * inert, the decision + ONE feedback line. Answers go out through
 * `sessions.answer` with `answers` +
 * `surfaceData`. Geometry = [PlanCard.Look] / [PlanCard.Tiles].
 */
@Composable
internal fun ColumnScope.PlanAskCard(
    row: SessionTree.ThreadRow,
    onAnswer: ThreadAnswer,
    onDecidePlan: ThreadPlanDecision,
    modifier: Modifier = Modifier,
    initialStep: Int = 0,
    /** N-12: drawn as THE DockCard sheet's body + footer (plan-card.json
     *  `stepper`: the pages, the footer decision); null = the transcript card. */
    sheet: DockSheetScope? = null,
    /** The sheet's reject state on mount (the styleguide's specimen). */
    initialRejecting: Boolean = false,
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
    val customText = remember(row.id) { mutableStateMapOf<String, String>() }
    var feedback by remember(row.id) { mutableStateOf("") }
    val surfaceHandle = remember(row.id) { CardSurfaceHandle() }
    // The sheet's 0-based page.
    var step by remember(row.id) { mutableIntStateOf(initialStep) }
    val stepped = PlanCard.stepperApplies(plan != null, plan?.decision, blocks.size)

    /** A block's answer now (web `questionAnswer`): its picks in option
     *  order; typed text joins a multi's picks and replaces a single's. */
    fun answerOf(block: ChoiceBlock): List<String> {
        val picked = block.options.map { it.label }.filter { it in picks[block.id].orEmpty() }
        val typed = customText[block.id]?.trim()?.takeIf { block.custom && it.isNotEmpty() }
            ?: return picked
        return if (block.multi) picked + typed else listOf(typed)
    }

    fun toggle(block: ChoiceBlock, label: String) {
        val current = picks[block.id].orEmpty()
        picks[block.id] = when {
            label in current -> current - label
            block.multi -> current + label
            else -> setOf(label)
        }
    }

    if (sheet != null) {
        var rejecting by remember(row.id) { mutableStateOf(initialRejecting) }
        val chat = @Composable { content: @Composable () -> Unit ->
            CompositionLocalProvider(
                LocalMarkdownBodyStyle provides TranscriptBodyStyle,
                LocalInlineCodeStyle provides MdStyle.Chat,
                content = content,
            )
        }
        // Page 0 = the plan (+ its questions below `minQuestions`), then one
        // page per question; "Question n of m" is the header's meta.
        val pages = if (stepped) 1 + blocks.size else 1
        val page = step.coerceIn(0, pages - 1)
        val pageBlock = if (stepped && page > 0) blocks[page - 1] else null
        val meta = if (pageBlock != null) PlanCard.stepLabel(page, blocks.size) else null
        androidx.compose.runtime.SideEffect { sheet.meta = meta }
        DockSheetBody {
            Box(Modifier.fillMaxWidth().testTag(PlanCard.TestIds.CARD)) {
                chat {
                    Column(Modifier.fillMaxWidth()) {
                        if (pageBlock != null) {
                            ChoiceBlockView(
                                block = pageBlock,
                                caption = if (pageBlock.multi) PlanCard.Copy.CHOOSE_ANY else PlanCard.Copy.CHOOSE_ONE,
                                picked = picks[pageBlock.id].orEmpty(),
                                typed = null,
                                answered = false,
                                enabled = live,
                                customText = customText[pageBlock.id].orEmpty(),
                                onToggle = { toggle(pageBlock, it) },
                                onCustomText = { customText[pageBlock.id] = it },
                                topPadding = 0.dp,
                            )
                        } else {
                            MarkdownView(PlanCard.withoutPendingUploads(plan?.plan ?: question!!.question))
                            val surface: CardSurface? = plan?.surface ?: question?.surface
                            if (surface != null) {
                                CardSurfaceView(
                                    surface,
                                    surfaceHandle,
                                    inert = !live,
                                    modifier = Modifier.padding(top = PlanCard.Look.BLOCK_GAP.dp),
                                )
                            }
                            if (!stepped) blocks.forEach { block ->
                                if (plan == null && block.options.isEmpty() && !block.custom) return@forEach
                                ChoiceBlockView(
                                    block = block,
                                    caption = when {
                                        plan != null -> if (block.multi) PlanCard.Copy.CHOOSE_ANY else PlanCard.Copy.CHOOSE_ONE
                                        block.multi && block.options.isNotEmpty() -> PlanCard.Copy.CHOOSE_ANY
                                        else -> null
                                    },
                                    picked = picks[block.id].orEmpty(),
                                    typed = null,
                                    answered = false,
                                    enabled = live,
                                    customText = customText[block.id].orEmpty(),
                                    onToggle = { toggle(block, it) },
                                    onCustomText = { customText[block.id] = it },
                                )
                            }
                        }
                    }
                }
            }
        }
        DockSheetFooter {
            if (plan != null) {
                val answers = blocks.associate { it.id to answerOf(it) }
                val issue = PlanCard.answerIssue(
                    PlanCard.Card.Plan(plan.questions),
                    PlanCard.Input(decision = "approved", planAnswers = answers),
                )
                PlanSheetFooter(
                    onBack = if (page > 0) ({ step = page - 1 }) else null,
                    onNext = if (page < pages - 1) ({ step = page + 1 }) else null,
                    decide = page == pages - 1 && !answered,
                    approveIssue = issue,
                    rejecting = rejecting,
                    onRejecting = { rejecting = it },
                    feedback = feedback,
                    onFeedback = { feedback = it },
                    busy = locked,
                    onDecide = { option ->
                        val approve = option.decision == "approved"
                        if (live && !(approve && issue != null)) {
                            state = AnswerState.Sending
                            onDecidePlan(
                                plan.id,
                                PlanAnswer(
                                    approved = approve,
                                    fresh = approve && option.fresh,
                                    feedback = if (approve) null else feedback.trim().takeIf { it.isNotEmpty() },
                                    answers = answers.filterValues { it.isNotEmpty() }.takeIf { it.isNotEmpty() },
                                    surfaceData = surfaceHandle.snapshot(),
                                ),
                                settle,
                            )
                        }
                    },
                )
            } else {
                val block = blocks.first()
                val labels = answerOf(block)
                val issue = PlanCard.answerIssue(
                    PlanCard.Card.Question(block.options.map { it.label }, block.multi, block.custom),
                    PlanCard.Input(answers = labels),
                )
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    if (locked) {
                        AnsweringLine()
                    } else {
                        CardButton(
                            label = PlanCard.Copy.ANSWER,
                            enabled = live && issue == null,
                            modifier = Modifier.testTag("thread-question-answer"),
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
            }
        }
        return
    }

    AskCard(
        plan = plan != null,
        // Only a plan carries a header; a question card is its glyph alone.
        heading = if (plan != null) AskDock.Copy.PLAN_READY else null,
        meta = null,
        modifier = modifier.testTag(PlanCard.TestIds.CARD),
    ) {
        val body = @Composable {
            // The chat measure (14sp, h1 = `card.headingScale` × it), not
            // the 17sp document body an issue description uses.
            CompositionLocalProvider(
                LocalMarkdownBodyStyle provides TranscriptBodyStyle,
                LocalInlineCodeStyle provides MdStyle.Chat,
            ) {
                Column(Modifier.fillMaxWidth()) {
                    MarkdownView(PlanCard.withoutPendingUploads(plan?.plan ?: question!!.question))
                    val surface: CardSurface? = plan?.surface ?: question?.surface
                    if (surface != null) {
                        CardSurfaceView(
                            surface,
                            surfaceHandle,
                            inert = !live,
                            modifier = Modifier.padding(top = PlanCard.Look.BLOCK_GAP.dp),
                        )
                    }
                    blocks.forEach { block ->
                        if (plan == null && block.options.isEmpty() && !block.custom) return@forEach
                        ChoiceBlockView(
                            block = block,
                            // A plan question always names its rule; a question
                            // card only says "Choose any" while it takes several.
                            caption = when {
                                plan != null -> if (block.multi) PlanCard.Copy.CHOOSE_ANY else PlanCard.Copy.CHOOSE_ONE
                                block.multi && !answered && block.options.isNotEmpty() -> PlanCard.Copy.CHOOSE_ANY
                                else -> null
                            },
                            picked = if (answered) recorded[block.id].orEmpty().toSet() else picks[block.id].orEmpty(),
                            typed = if (answered) {
                                recorded[block.id].orEmpty().filter { label -> block.options.none { it.label == label } }
                                    .takeIf { it.isNotEmpty() }?.let(PlanCard::joinAnswers)
                            } else {
                                null
                            },
                            answered = answered,
                            enabled = live,
                            customText = customText[block.id].orEmpty(),
                            onToggle = { toggle(block, it) },
                            onCustomText = { customText[block.id] = it },
                        )
                    }
                }
            }
        }
        body()

        if (answered) {
            if (plan != null) {
                Column(Modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    AnsweredRow(AskDock.planDecisionLabel(plan.decision!!, plan.fresh))
                    plan.feedback?.let {
                        Text(
                            PlanCard.Copy.feedbackLine(it),
                            modifier = Modifier.testTag(PlanCard.TestIds.FEEDBACK),
                            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Normal),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            } else if (recorded[QUESTION_CARD_ID].isNullOrEmpty()) {
                // Nothing recorded to highlight: the bare resolution line.
                Box(Modifier.padding(top = 4.dp)) { AnsweredRow(question!!.answer) }
            }
        } else if (plan != null) {
            // N-12: a pending plan decides in THE Dock's sheet; here it reads.
        } else {
            val block = blocks.first()
            val labels = answerOf(block)
            val issue = PlanCard.answerIssue(
                PlanCard.Card.Question(block.options.map { it.label }, block.multi, block.custom),
                PlanCard.Input(answers = labels),
            )
            if (!locked) {
                CardButton(
                    label = PlanCard.Copy.ANSWER,
                    enabled = live && issue == null,
                    modifier = Modifier.padding(top = 8.dp).testTag("thread-question-answer"),
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
            Box(Modifier.padding(top = 4.dp)) { AnsweringLine() }
        }
    }
}

/** The answer on its way: the small spinner + `copy.answering`. */
@Composable
private fun AnsweringLine() {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(12.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onSurface)
        Text(
            PlanCard.Copy.ANSWERING,
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Normal),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/**
 * N-12: THE plan sheet's pinned footer ×4 (web `PlanCardFooter`): Back
 * leading, Next trailing (plan-card.json `stepper`); the last page carries
 * THE decision instead of Next: "No, keep planning" (secondary; picked = the
 * feedback field above the row, then Cancel + the reject as the primary) and
 * "Yes" (the primary, its chevron menu holding "Yes, and start with a fresh
 * context"), disabled while a required answer is missing (the issue line
 * under the row).
 */
@Composable
internal fun PlanSheetFooter(
    onBack: (() -> Unit)?,
    onNext: (() -> Unit)?,
    decide: Boolean,
    approveIssue: PlanCard.AnswerIssue?,
    rejecting: Boolean,
    onRejecting: (Boolean) -> Unit,
    feedback: String,
    onFeedback: (String) -> Unit,
    busy: Boolean,
    onDecide: (AskDock.PlanOption) -> Unit,
) {
    val approve = AskDock.PLAN_OPTIONS.first { it.key == "approve" }
    val fresh = AskDock.PLAN_OPTIONS.firstOrNull { it.key == "approve-fresh" }
    val reject = AskDock.PLAN_OPTIONS.first { it.key == "reject" }
    if (decide && rejecting) {
        CardTextField(
            value = feedback,
            onValueChange = onFeedback,
            placeholder = AskDock.Copy.FEEDBACK_PLACEHOLDER,
            enabled = !busy,
            singleLine = false,
            minHeight = PlanCard.Look.FIELD_MIN_HEIGHT.dp,
            textStyle = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.testTag(PlanCard.TestIds.FEEDBACK),
        )
    }
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PlanCard.Stepper.BUTTON_GAP.dp),
    ) {
        if (onBack != null) {
            CardButton(
                label = PlanCard.Copy.BACK,
                enabled = true,
                primary = false,
                modifier = Modifier.testTag(PlanCard.TestIds.STEP_BACK),
                onClick = onBack,
            )
        }
        Spacer(Modifier.weight(1f))
        when {
            onNext != null -> CardButton(
                label = PlanCard.Copy.NEXT,
                enabled = true,
                primary = false,
                modifier = Modifier.testTag(PlanCard.TestIds.STEP_NEXT),
                onClick = onNext,
            )
            !decide -> Unit
            busy -> AnsweringLine()
            rejecting -> {
                CardButton(label = PlanCard.Copy.CANCEL, enabled = true, primary = false, onClick = { onRejecting(false) })
                CardButton(
                    label = reject.label,
                    enabled = true,
                    modifier = Modifier.testTag("plan-card-decide"),
                    onClick = { onDecide(reject) },
                )
            }
            else -> {
                CardButton(
                    label = reject.label,
                    enabled = true,
                    primary = false,
                    modifier = Modifier.testTag("plan-card-reject"),
                    onClick = { onRejecting(true) },
                )
                ApproveSplitButton(
                    approve = approve,
                    fresh = fresh,
                    enabled = approveIssue == null,
                    onDecide = onDecide,
                )
            }
        }
    }
    if (decide && !rejecting && !busy && approveIssue != null) {
        Text(
            approveIssue.text,
            modifier = Modifier.testTag(PlanCard.TestIds.ISSUE),
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Normal),
            // The warning paint ×4 (web `text-yellow-400`).
            color = DesignTokens.Semantic.Yellow,
        )
    }
}

/** "Yes" as the small primary button, joined to its chevron that opens the
 *  menu holding [fresh] (web `rounded-r-none` + the `-ml-px` chevron). */
@Composable
private fun ApproveSplitButton(
    approve: AskDock.PlanOption,
    fresh: AskDock.PlanOption?,
    enabled: Boolean,
    onDecide: (AskDock.PlanOption) -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val radius = PlanCard.Look.ROW_RADIUS.dp
    val shape = RoundedCornerShape(radius)
    Row(
        modifier = Modifier
            .alpha(if (enabled) 1f else PlanCard.Look.INERT_OPACITY)
            .clip(shape)
            .background(MaterialTheme.colorScheme.primary, shape)
            .heightIn(min = PlanCard.Look.ROW_MIN_HEIGHT.dp)
            .testTag(PlanCard.TestIds.DECISION),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .then(if (enabled) Modifier.clickable(role = Role.Button) { onDecide(approve) } else Modifier)
                .heightIn(min = PlanCard.Look.ROW_MIN_HEIGHT.dp)
                .padding(horizontal = 12.dp)
                .testTag("plan-card-decide"),
            contentAlignment = Alignment.Center,
        ) {
            Text(approve.label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onPrimary)
        }
        if (fresh != null) {
            Box(
                Modifier
                    .size(width = GlassTokens.Hairline, height = PlanCard.Look.ROW_MIN_HEIGHT.dp)
                    .background(MaterialTheme.colorScheme.onPrimary.copy(alpha = 0.2f)),
            )
            Box {
                Box(
                    modifier = Modifier
                        .then(if (enabled) Modifier.clickable(role = Role.Button) { menuOpen = !menuOpen } else Modifier)
                        .heightIn(min = PlanCard.Look.ROW_MIN_HEIGHT.dp)
                        .padding(horizontal = 6.dp)
                        .semantics { contentDescription = fresh.label },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        ExpIcons.uiChevronDown,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = MaterialTheme.colorScheme.onPrimary,
                    )
                }
                // N-6: in the Dock sheet the menu's row opens inline, under
                // the footer's row; in the Conversation it is the popup.
                GlassHostMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    GlassMenuItem(
                        text = { Text(fresh.label) },
                        onClick = {
                            menuOpen = false
                            onDecide(fresh)
                        },
                        modifier = Modifier.testTag("plan-card-decide-fresh"),
                    )
                }
            }
        }
    }
}

/** One question (web `PlanCardQuestionBlock`): the prompt (`xs` semibold),
 *  the choose [caption] (`2xs` muted, `blockInnerGap` under it), the options
 *  `optionsGap` under that as THE option rows or picture tiles, then THE
 *  `Other…` field; closed = the picks + a typed answer's line. */
@Composable
private fun ChoiceBlockView(
    block: ChoiceBlock,
    caption: String?,
    picked: Set<String>,
    typed: String?,
    answered: Boolean,
    enabled: Boolean,
    customText: String,
    onToggle: (String) -> Unit,
    onCustomText: (String) -> Unit,
    /** NEW-6: a stepped question sits right under its step line. */
    topPadding: Dp? = null,
) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Column(
        modifier = Modifier
            .fillMaxWidth()
            // A plan question = `blockGap` under what precedes it; a question
            // card's own options hang off its body (web `mt-2` caption).
            .padding(
                top = when {
                    topPadding != null -> topPadding
                    block.prompt != null -> PlanCard.Look.BLOCK_GAP.dp
                    caption != null -> 8.dp
                    else -> 0.dp
                },
            )
            .testTag(PlanCard.TestIds.QUESTION),
    ) {
        if (block.prompt != null) {
            Text(
                block.prompt,
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                color = MaterialTheme.colorScheme.onSurface,
            )
        }
        if (caption != null) {
            Text(
                caption,
                modifier = Modifier.padding(top = if (block.prompt != null) PlanCard.Look.BLOCK_INNER_GAP.dp else 0.dp),
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Normal),
                color = muted,
            )
        }
        if (block.options.isNotEmpty()) {
            Box(Modifier.padding(top = PlanCard.Look.OPTIONS_GAP.dp)) {
                when (PlanCard.optionRows(block.options)) {
                    PlanCard.OptionRows.List -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        block.options.forEachIndexed { index, option ->
                            val on = option.label in picked
                            Box(Modifier.testTag(PlanCard.TestIds.OPTION)) {
                                QuestionOptionButton(
                                    option = QuestionOption(option.label, "$index:${option.label}", description = option.description),
                                    // The number keys belong to the decision rows.
                                    ordinal = 0,
                                    primary = false,
                                    selected = on,
                                    // Multi = the leading selected/unselected
                                    // glyph; single = the trailing check.
                                    checked = if (block.multi) on else null,
                                    enabled = enabled,
                                    dimmed = false,
                                    onClick = { onToggle(option.label) },
                                    trailingCheck = !answered && !block.multi && on,
                                    closed = if (answered) on else null,
                                )
                            }
                        }
                    }
                    PlanCard.OptionRows.Tiles -> PlanCardTiles(block, picked, answered, enabled, onToggle)
                }
            }
        }
        if (answered) {
            if (typed != null) Box(Modifier.padding(top = 4.dp)) { AnsweredRow(typed) }
        } else if (block.custom) {
            CardTextField(
                value = customText,
                onValueChange = onCustomText,
                placeholder = PlanCard.Copy.CUSTOM,
                enabled = enabled,
                singleLine = true,
                minHeight = PlanCard.Look.ROW_MIN_HEIGHT.dp,
                textStyle = MaterialTheme.typography.bodySmall,
                modifier = Modifier.padding(top = 4.dp).testTag(PlanCard.TestIds.CUSTOM),
            )
        }
    }
}

/** THE card's input (web `Input` / the feedback `Textarea`): a plain field
 *  in the SDK field chrome at [minHeight]; the `Other…` field is single
 *  line at `rowMinHeight`, the feedback multi-line at `fieldMinHeight`. */
@Composable
private fun CardTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    enabled: Boolean,
    singleLine: Boolean,
    minHeight: Dp,
    textStyle: TextStyle,
    modifier: Modifier = Modifier,
) {
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val color = MaterialTheme.colorScheme.onSurface
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        enabled = enabled,
        singleLine = singleLine,
        maxLines = if (singleLine) 1 else 6,
        textStyle = textStyle.copy(color = color),
        cursorBrush = SolidColor(color),
        interactionSource = interaction,
        modifier = modifier
            .fillMaxWidth()
            .fieldChrome(
                AppPrimitiveTokens,
                focused = focused,
                radius = PlanCard.Look.ROW_RADIUS.dp,
                fill = GlassTokens.CardFill,
                stroke = GlassTokens.StrokeCard,
                focusedStroke = GlassTokens.StrokeActive,
                strokeWidth = GlassTokens.Hairline,
            ),
        decorationBox = { inner ->
            Box(
                Modifier.heightIn(min = minHeight).padding(horizontal = 12.dp, vertical = 6.dp),
                contentAlignment = if (singleLine) Alignment.CenterStart else Alignment.TopStart,
            ) {
                if (value.isEmpty()) {
                    Text(placeholder, style = textStyle, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                inner()
            }
        },
    )
}

/** The card's small button (web `Button size="sm"`): [primary] = the
 *  question's Answer and the plan's decide button, else the stepper's
 *  secondary Back / Next (the row fill + glass hairline); a blocked one dims
 *  to `inertOpacity`. */
@Composable
private fun CardButton(
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    primary: Boolean = true,
) {
    val shape = RoundedCornerShape(PlanCard.Look.ROW_RADIUS.dp)
    Box(
        modifier = modifier
            .alpha(if (enabled) 1f else PlanCard.Look.INERT_OPACITY)
            .clip(shape)
            .background(if (primary) MaterialTheme.colorScheme.primary else GlassTokens.RowFill, shape)
            .then(if (primary) Modifier else Modifier.border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape))
            .then(if (enabled) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
            .heightIn(min = PlanCard.Look.ROW_MIN_HEIGHT.dp)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = if (primary) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
        )
    }
}

/** The options as picture tiles: THE `ChoiceTiles` primitive (the eUI
 *  recipe on the app's theme tokens) at the plan-card.json `tiles` numbers
 *  (phones: `tiles.phoneColumns`). */
@Composable
private fun PlanCardTiles(
    block: ChoiceBlock,
    picked: Set<String>,
    answered: Boolean,
    enabled: Boolean,
    onToggle: (String) -> Unit,
) {
    val tokens = LocalPrimitiveTokens.current
    val style = ChoiceTilesStyle.default(tokens).copy(
        columns = PlanCard.Tiles.PHONE_COLUMNS,
        gap = PlanCard.Tiles.GAP.dp,
        radius = PlanCard.Tiles.RADIUS.dp,
        padding = PlanCard.Tiles.PADDING.dp,
        pictureAspect = PlanCard.Tiles.PICTURE_ASPECT,
        pictureRadius = PlanCard.Tiles.PICTURE_RADIUS.dp,
        selectedRing = PlanCard.Tiles.SELECTED_RING.dp,
        pictureGap = PlanCard.Tiles.LABEL_GAP.dp,
        descriptionGap = PlanCard.Tiles.LABEL_GAP.dp,
        placeholderGlyph = PlanCard.Tiles.PLACEHOLDER_GLYPH.dp,
        inertOpacity = PlanCard.Tiles.INERT_OPACITY,
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
                modifier = Modifier.fillMaxSize(),
                tint = tokens.mutedForeground,
            )
        },
    )
}
