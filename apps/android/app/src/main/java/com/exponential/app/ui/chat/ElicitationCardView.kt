package com.exponential.app.ui.chat

import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.ElicitationCard
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.locksCard
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.LocalInlineCodeStyle
import com.exponential.app.ui.markdown.LocalMarkdownBodyStyle
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MdStyle
import com.exponential.app.ui.session.AnsweredRow
import com.exponential.app.ui.session.AskCard
import com.exponential.app.ui.session.TranscriptBodyStyle
import com.exponential.app.ui.theme.DesignTokens

/**
 * #NEW-146 (fixtures/elicitation-card.json RENDER, web `ElicitationCard`):
 * THE elicitation card: the question card's chrome with the asking SERVER as
 * its label (`icons.server`), the message, the device-built surface (the
 * form, or the URL review) WITHOUT the `Preview` caption, then ONE
 * end-aligned button row: form = Cancel (ghost) · Decline (outline) · Submit
 * (primary, disabled while a required field is empty, `copy.missing` under
 * the row); url = Decline · I'm done (outline) · Open link (primary). Open
 * link opens the url in Custom Tabs on the press only (no prefetch, never a
 * WebView) and answers nothing. Answers go through THE answer path
 * (`answers: [label]`, the form's `surfaceData`). Answered = closed: the
 * surface inert at its answered data, the pick named.
 */
@Composable
internal fun ElicitationAskCard(
    row: SessionTree.SessionChatRow.Question,
    elicitation: ElicitationCard.Sidecar,
    onAnswer: ChatAnswer,
    modifier: Modifier = Modifier,
) {
    var state by remember(row.id) { mutableStateOf<AnswerState?>(null) }
    val settle: (Boolean) -> Unit = { ok -> state = if (ok) AnswerState.Acked else null }
    val locked = state.locksCard()
    val live = !row.answered && !locked
    val handle = remember(row.id) { CardSurfaceHandle() }
    val form = elicitation.mode == ElicitationCard.Mode.Form
    // Re-read the form's data model on every pass of its surface.
    val missing by remember(row.id, handle, elicitation) {
        derivedStateOf {
            if (!form) return@derivedStateOf emptyList()
            val model = handle.model
            model?.stats
            ElicitationCard.missing(elicitation.required, handle.snapshot() ?: row.surface?.data)
        }
    }
    val dismiss = com.exponential.app.ui.session.LocalCardDismiss.current
    val onDismiss: (() -> Unit)? = if (dismiss != null && live) {
        {
            state = AnswerState.Sending
            dismiss(row.id, settle)
        }
    } else {
        null
    }
    fun send(label: String, withData: Boolean) {
        if (!live) return
        state = AnswerState.Sending
        onAnswer(row.id, AskAnswer(label, listOf(label), if (withData) handle.snapshot() else null), settle)
    }
    val context = LocalContext.current
    AskCard(
        plan = false,
        heading = elicitation.server,
        meta = null,
        headingIcon = ExpIcons.settingsMcp,
        headingTestTag = ElicitationCard.TestIds.SERVER,
        modifier = modifier.testTag(ElicitationCard.TestIds.CARD),
        onDismiss = onDismiss,
        highlighted = com.exponential.app.ui.session.LocalCardHighlight.current == row.id,
    ) {
        CompositionLocalProvider(
            LocalMarkdownBodyStyle provides TranscriptBodyStyle,
            LocalInlineCodeStyle provides MdStyle.Chat,
        ) {
            Column(Modifier.fillMaxWidth()) {
                if (row.question.isNotEmpty()) MarkdownView(row.question)
                val surface = row.surface
                if (surface != null) {
                    // Answered: the surface inert at the data the person sent.
                    val painted = remember(surface, row.answered, row.surfaceData) {
                        if (row.answered && row.surfaceData != null) surface.copy(data = row.surfaceData) else surface
                    }
                    CardSurfaceView(
                        painted,
                        handle,
                        inert = !live,
                        caption = false,
                        modifier = Modifier.padding(top = PlanCard.Look.BLOCK_GAP.dp),
                    )
                }
            }
        }
        when {
            row.answered -> Box(Modifier.padding(top = 4.dp)) { AnsweredRow(row.answers?.firstOrNull() ?: row.answer) }
            locked -> Box(Modifier.padding(top = 4.dp)) { AnsweringLine() }
            else -> Column(
                modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                horizontalAlignment = Alignment.End,
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (form) {
                        CardButton(
                            label = ElicitationCard.Copy.CANCEL,
                            enabled = true,
                            look = CardButtonLook.Ghost,
                            modifier = Modifier.testTag(ElicitationCard.TestIds.CANCEL),
                            onClick = { send(ElicitationCard.Copy.CANCEL, false) },
                        )
                        CardButton(
                            label = ElicitationCard.Copy.DECLINE,
                            enabled = true,
                            look = CardButtonLook.Outline,
                            modifier = Modifier.testTag(ElicitationCard.TestIds.DECLINE),
                            onClick = { send(ElicitationCard.Copy.DECLINE, false) },
                        )
                        CardButton(
                            label = ElicitationCard.Copy.SUBMIT,
                            enabled = missing.isEmpty(),
                            modifier = Modifier.testTag(ElicitationCard.TestIds.SUBMIT),
                            onClick = { send(ElicitationCard.Copy.SUBMIT, true) },
                        )
                    } else {
                        CardButton(
                            label = ElicitationCard.Copy.DECLINE,
                            enabled = true,
                            look = CardButtonLook.Outline,
                            modifier = Modifier.testTag(ElicitationCard.TestIds.DECLINE),
                            onClick = { send(ElicitationCard.Copy.DECLINE, false) },
                        )
                        CardButton(
                            label = ElicitationCard.Copy.DONE,
                            enabled = true,
                            look = CardButtonLook.Outline,
                            modifier = Modifier.testTag(ElicitationCard.TestIds.DONE),
                            onClick = { send(ElicitationCard.Copy.DONE, false) },
                        )
                        CardButton(
                            label = ElicitationCard.Copy.OPEN_LINK,
                            enabled = elicitation.url != null,
                            modifier = Modifier.testTag(ElicitationCard.TestIds.OPEN_LINK),
                            onClick = { elicitation.url?.let { openElicitationUrl(context, it) } },
                        )
                    }
                }
                if (form && missing.isNotEmpty()) {
                    Text(
                        ElicitationCard.Copy.MISSING,
                        style = MaterialTheme.typography.labelSmall.copy(fontSize = 11.sp),
                        color = DesignTokens.Semantic.Yellow,
                    )
                }
            }
        }
    }
}

/** No prefetch (no `mayLaunchUrl`, no warm-up), never an in-app WebView:
 *  Custom Tabs on the press only. Answers nothing. */
private fun openElicitationUrl(context: android.content.Context, url: String) {
    runCatching { CustomTabsIntent.Builder().build().launchUrl(context, Uri.parse(url)) }
}
