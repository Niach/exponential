package com.exponential.app.ui.styleguide

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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.RoundedCornerShape
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.ToastAction
import com.exponential.app.domain.ToastKind
import com.exponential.app.domain.ToastItem
import com.exponential.app.ui.components.BlockingSurface
import com.exponential.app.ui.components.FeedbackMark
import com.exponential.app.ui.components.InlineStatus
import com.exponential.app.ui.components.InlineStatusState
import com.exponential.app.ui.components.PendingPill
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.ToastCard
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** The screen's tag (iOS `feedback-states-screen`). */
const val FeedbackStatesScreenTestId = "feedback-states-screen"

/**
 * EXP-1275 (EXP-1259 D8): the styleguide's "Feedback states" section on
 * Android — a capture-only screen (sg_feedback-states) drawing every kind of
 * `feedback-states.json` with its REAL primitive: inline, pending,
 * placeholder, the mark (live + frozen frames), blocking (contained) and
 * errors (the toast raised on appear, the inline field line).
 */
@Composable
fun FeedbackStatesScreen() {
    val toaster = remember { Toaster() }
    LaunchedEffect(Unit) {
        toaster.error(
            FeedbackStates.errorTitle("save", "the team prompt"),
            "The server did not answer.",
            ToastAction(FeedbackStates.RETRY) {},
        )
    }
    AppBackground {
        Box(modifier = Modifier.fillMaxSize().testTag(FeedbackStatesScreenTestId)) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .statusBarsPadding()
                    .verticalScroll(rememberScrollState())
                    // The top clears the error toast raised over the screen.
                    .padding(start = 16.dp, end = 16.dp, top = 112.dp, bottom = 72.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                Kind("Inline") {
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        InlineStatus(InlineStatusState.Saving)
                        InlineStatus(InlineStatusState.Saved)
                        InlineStatus(InlineStatusState.Idle, idleLabel = "Edited 2m ago")
                    }
                    SectionHeader("Running", loading = true)
                }
                Kind("Pending") {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        PendingPill(label = "Start", pendingLabel = "Starting", pending = false, icon = ExpIcons.actionRun, onClick = {})
                        PendingPill(
                            label = "Start",
                            pendingLabel = FeedbackStates.PENDING_WORDS.getValue("start"),
                            pending = true,
                            icon = ExpIcons.actionRun,
                            onClick = {},
                        )
                    }
                }
                Kind("Placeholder") { SkeletonRows() }
                Kind("The mark") {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        FeedbackMark(size = FeedbackStates.MARK_SIZE_STYLEGUIDE.dp)
                        listOf(0.15f, 0.3f, 0.6f, 0.85f).forEach { phase ->
                            FeedbackMark(size = FeedbackStates.MARK_SIZE_STYLEGUIDE.dp, phase = phase)
                        }
                    }
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        FeedbackMark(size = FeedbackStates.MARK_SIZE_STATUS.dp)
                        Text(
                            FeedbackStates.startingLabel("Alex's MacBook Pro"),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
                Kind("Blocking") {
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(160.dp)
                            .clip(RoundedCornerShape(GlassTokens.GroupRadius)),
                    ) {
                        BlockingSurface(FeedbackStates.switchingTeam("Acme"))
                    }
                }
                Kind("Errors") {
                    Text(
                        "team@acme",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "Enter a full email address",
                        // `errors.inline.size`: the rose line under its field.
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.error,
                    )
                    // EXP-1391 + EXP-1378: the check gate's refusal offers
                    // Merge anyway instead of Retry (`errors.mergeRefusal`).
                    ToastCard(
                        item = ToastItem(
                            id = "merge-refused",
                            kind = ToastKind.Error,
                            title = FeedbackStates.errorTitle("merge", "the PR"),
                            description = "Checks are still running on PR #7: \"gate\". ${FeedbackStates.MERGE_HANDOFF}",
                            action = ToastAction(FeedbackStates.MERGE_ANYWAY) {},
                        ),
                        onDismiss = {},
                        onExpandToggle = {},
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
            ToastHost(toaster, Modifier.align(Alignment.TopCenter))
        }
    }
}

/** One kind: its muted label over its specimen. */
@Composable
private fun Kind(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(
            title,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
        content()
    }
}
