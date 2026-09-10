package com.exponential.app.ui.agent

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.insertImageMarker
import com.exponential.app.domain.renumberImageMarkers
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the ONE launcher's card — the same [GlassComposer] the steer and
 * comment composers wear. Slot order is leading · strip · field · tools:
 *
 * - leading: the subject chips (issue chips OR one action chip — tapping a
 *   chip removes it) and, under an action that declares inputs, its typed
 *   pick rows ([ActionInputFields]);
 * - field: the draft with the `@` / `#` / `:` typeahead (the page mounts the
 *   candidate rows under the card and splices at this value's caret);
 * - strip: the pending images (the steer composer's tiles + `[Image #k]`
 *   markers, EXP-698);
 * - tools: `#` opens the issue picker, ▶ the action picker, the image glyph
 *   the photo picker; the submit is a LABELLED primary pill whose title
 *   follows the subject ("Start chat" / "Start coding" / "Start batch · N" /
 *   "Run action").
 *
 * Test tags are byte-identical with the iOS identifiers (the styleguide and
 * store captures address both platforms by the same names).
 */
@Composable
internal fun AgentComposer(
    value: TextFieldValue,
    // A user edit (may arm the autocomplete) vs the composer's own rewrites
    // (markers, renumbering), which never arm — the EXP-802 split.
    onValueChange: (TextFieldValue) -> Unit,
    onValueRewrite: (TextFieldValue) -> Unit,
    fieldModifier: Modifier,
    placeholder: String,
    issueChips: List<IssueOption>,
    onRemoveIssue: (String) -> Unit,
    actionChip: ActionDto?,
    onClearAction: () -> Unit,
    inputValues: Map<String, String>,
    repos: List<TeamRepo>,
    boards: List<StartBoardOption>,
    pullRequests: List<StartPullRequestOption>,
    onInputChange: (key: String, value: String) -> Unit,
    pendingImages: List<PendingAttachment>,
    imageError: String?,
    canAttach: Boolean,
    sending: Boolean,
    onPickIssues: () -> Unit,
    onPickActions: () -> Unit,
    onPickImages: () -> Unit,
    onRemoveImage: (Int) -> Unit,
    submitLabel: String,
    canSubmit: Boolean,
    onSubmit: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Each newly picked image inserts its own `[Image #k]` marker at the
    // caret, so the writer can say "crop [Image #2]" without typing the
    // token. Removing one renumbers the draft (below), so the markers always
    // name images the composer still has (the steer composer's rule).
    var markedImages by remember { mutableIntStateOf(pendingImages.size) }
    LaunchedEffect(pendingImages.size) {
        if (pendingImages.size > markedImages) {
            var next = value
            for (k in (markedImages + 1)..pendingImages.size) {
                val (text, caret) = insertImageMarker(next.text, next.selection.end, k)
                next = TextFieldValue(text, TextRange(caret))
            }
            onValueRewrite(next)
        }
        markedImages = pendingImages.size
    }
    val hasSubject = issueChips.isNotEmpty() || actionChip != null
    val inputDefs = actionChip?.inputs.orEmpty()
    GlassComposer(
        modifier = modifier.testTag("agent-composer"),
        leading = if (hasSubject) {
            {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .horizontalScroll(rememberScrollState())
                        .padding(bottom = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    if (actionChip != null) {
                        // The one action: its curated glyph · name · ✕.
                        GlassPill(
                            actionChip.name,
                            onClick = onClearAction,
                            icon = actionGlyph(actionChip),
                            trailing = { ChipClose() },
                            contentDescription = "Remove ${actionChip.name}",
                            modifier = Modifier.testTag("agent-composer-chip-action"),
                        )
                    } else {
                        // One checked issue: status glyph · mono identifier ·
                        // ✕. The chip IS the remove control.
                        issueChips.forEach { option ->
                            GlassPill(
                                option.identifier,
                                onClick = { onRemoveIssue(option.id) },
                                leading = { StatusIcon(IssueStatus.fromWire(option.status), size = 12.dp) },
                                trailing = { ChipClose() },
                                fontFamily = FontFamily.Monospace,
                                contentDescription = "Remove ${option.identifier}",
                                modifier = Modifier.testTag("agent-composer-chip-issue-${option.identifier}"),
                            )
                        }
                    }
                }
                if (actionChip != null && inputDefs.isNotEmpty()) {
                    ActionInputFields(
                        defs = inputDefs,
                        values = inputValues,
                        repos = repos,
                        boards = boards,
                        pullRequests = pullRequests,
                        onValueChange = onInputChange,
                    )
                    Spacer(Modifier.height(4.dp))
                }
            }
        } else {
            null
        },
        strip = {
            PendingAttachmentStrip(
                items = pendingImages,
                enabled = !sending,
                onRemove = { index ->
                    // Renumber BEFORE the list shrinks: `[Image #k]` goes, and
                    // every higher marker comes down one.
                    val renumbered = renumberImageMarkers(value.text, index + 1)
                    if (renumbered != value.text) {
                        onValueRewrite(
                            TextFieldValue(
                                renumbered,
                                TextRange(value.selection.end.coerceAtMost(renumbered.length)),
                            ),
                        )
                    }
                    onRemoveImage(index)
                },
            )
            if (imageError != null) {
                Text(
                    imageError,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(bottom = 4.dp),
                )
            }
        },
        tools = {
            // EXP-825 ×4: `#` for issues, ▶ for actions, the image glyph every
            // other composer wears.
            ComposerToolButton(
                ExpIcons.editorIssueRef,
                contentDescription = "Pick issues",
                onClick = onPickIssues,
                enabled = !sending,
                modifier = Modifier.testTag("agent-composer-issues-button"),
            )
            ComposerToolButton(
                ExpIcons.actionRun,
                contentDescription = "Pick an action",
                onClick = onPickActions,
                enabled = !sending,
                modifier = Modifier.testTag("agent-composer-actions-button"),
            )
            ComposerToolButton(
                ExpIcons.editorImage,
                contentDescription = "Attach image",
                onClick = onPickImages,
                enabled = canAttach && !sending,
                modifier = Modifier.testTag("agent-composer-image-button"),
            )
        },
        submit = {
            // The contract's label IS the affordance (iOS `GlassPill` primary
            // parity) — a bare send arrow could not say "Start batch · 3".
            GlassPill(
                submitLabel,
                onClick = onSubmit,
                primary = true,
                enabled = canSubmit && !sending,
                loading = sending,
                modifier = Modifier.testTag("agent-composer-submit"),
            )
        },
    ) {
        GlassTextField(
            value = value,
            onValueChange = onValueChange,
            modifier = Modifier
                .fillMaxWidth()
                .testTag("agent-composer-field")
                .then(fieldModifier),
            placeholder = placeholder,
            minLines = 2,
            maxLines = 8,
            // The composer card owns the chrome; the field is just its text.
            bordered = false,
        )
    }
}

@Composable
private fun ChipClose() {
    Icon(
        ExpIcons.uiClose,
        contentDescription = null,
        modifier = Modifier.size(10.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
    )
}
