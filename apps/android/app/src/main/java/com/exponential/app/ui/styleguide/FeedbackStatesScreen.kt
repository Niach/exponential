package com.exponential.app.ui.styleguide

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.shape.RoundedCornerShape
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.ToastItem
import com.exponential.app.domain.ToastKind
import com.exponential.app.ui.components.BlockingSurface
import com.exponential.app.ui.components.FeedbackMark
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.InlineStatus
import com.exponential.app.ui.components.InlineStatusState
import com.exponential.app.ui.components.PendingPill
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.ToastCard
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** The screen's tag (iOS `feedback-states-screen`). */
const val FeedbackStatesScreenTestId = "feedback-states-screen"

/**
 * EXP-1275 (EXP-1259 D8): the styleguide's "Feedback states" section on
 * Android — a capture-only screen (sg_feedback-states) drawing every kind of
 * `feedback-states.json` with its REAL primitive. EXP-1554: ONE layout ×4 =
 * web's `FeedbackStatesSpecimen`, its six numbered cards ('1 · Inline' …) in
 * the same order with the same samples, stacked in one column on the phone:
 * the error toast is drawn IN its card (at rest), never raised over the screen.
 */
@Composable
fun FeedbackStatesScreen() {
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(FeedbackStatesScreenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    .padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 72.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Kind(1, "Inline") {
                    Band("Team prompt") { InlineStatus(InlineStatusState.Saving) }
                    Band("Team prompt") { InlineStatus(InlineStatusState.Saved) }
                    SectionHeader(
                        "MCP servers",
                        loading = true,
                        trailing = {
                            Text(
                                "3",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                            )
                        },
                    )
                }
                Kind(2, "Pending action") {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        PendingPill(label = "Start", pendingLabel = START_WORD, pending = false, icon = ExpIcons.uiSubmit, primary = true, onClick = {})
                        PendingPill(label = "Start", pendingLabel = START_WORD, pending = true, icon = ExpIcons.uiSubmit, primary = true, onClick = {})
                        PendingPill(
                            label = "Connect",
                            pendingLabel = FeedbackStates.PENDING_WORDS.getValue("connect"),
                            pending = true,
                            onClick = {},
                        )
                    }
                }
                Kind(3, "Placeholder") { SkeletonRows() }
                Kind(4, "The mark") {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        // web MARK_PHASES: the mark frozen at four phases of its loop.
                        listOf(0.08f, 0.2f, 0.32f, 0.5f).forEach { phase ->
                            FeedbackMark(size = FeedbackStates.MARK_SIZE_STYLEGUIDE.dp, phase = phase)
                        }
                    }
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier
                            .fillMaxWidth()
                            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, RoundedCornerShape(12.dp))
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                    ) {
                        FeedbackMark(size = FeedbackStates.MARK_SIZE_INLINE.dp, phase = 0.3f)
                        Text(
                            FeedbackStates.startingLabel("macbook"),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            "1.4 s",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                    }
                }
                Kind(5, "Blocking") {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(160.dp)
                            .clip(RoundedCornerShape(GlassTokens.GroupRadius))
                            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, RoundedCornerShape(GlassTokens.GroupRadius)),
                    ) {
                        BlockingSurface(FeedbackStates.switchingTeam("Personal"))
                    }
                }
                Kind(6, "Errors") {
                    ToastCard(
                        item = ToastItem(
                            id = "feedback-states-error",
                            kind = ToastKind.Error,
                            title = FeedbackStates.errorTitle("save", "the team prompt"),
                            description = "Network error",
                            action = ToastAction(FeedbackStates.RETRY) {},
                        ),
                        onDismiss = {},
                        onExpandToggle = {},
                    )
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        GlassTextField(value = "dennis@", onValueChange = {}, singleLine = true)
                        // `errors.cases[4]`: invalid input stays under its field.
                        FieldError("Enter a full email address")
                    }
                }
            }
        }
    }
}

private val START_WORD = FeedbackStates.PENDING_WORDS.getValue("start")

/** One kind: web `Kind`, a bordered card titled '<n> · <title>' over its specimen. */
@Composable
private fun Kind(index: Int, title: String, content: @Composable () -> Unit) {
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, RoundedCornerShape(12.dp))
            .padding(16.dp),
    ) {
        Text(
            "$index · $title",
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
        )
        content()
    }
}

/** web `Band`: a 40dp muted strip, its label left and its status right. */
@Composable
private fun Band(label: String, trailing: @Composable () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .height(40.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(GlassTokens.CardFill)
            .padding(horizontal = 12.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        trailing()
    }
}
