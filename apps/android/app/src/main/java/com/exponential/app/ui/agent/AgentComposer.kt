package com.exponential.app.ui.agent

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.PromptDto
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.AgentComposerPrompt
import com.exponential.app.domain.ComposerMenu
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.ui.components.FeedbackMark
import com.exponential.app.ui.components.FieldError
import com.exponential.app.ui.components.rememberDelayedFlag
import kotlinx.coroutines.delay
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.imageNumberAt
import com.exponential.app.domain.insertImageMarker
import com.exponential.app.domain.renumberImageMarkers
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.IssueChip
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-825: the ONE launcher's card — the same [GlassComposer] the steer and
 * comment composers wear. Slot order is leading · strip · field · tools:
 *
 * - leading: none (NEW-2: prompts have no typed inputs). EXP-1038: the
 *   subject chips are NOT here any more — they head the page beside the
 *   contract verb
 *   ([AgentComposerHeadline], mounted by the page above this card);
 * - field: the draft with the `@` / `#` / `:` typeahead (the page mounts the
 *   candidate rows under the card and splices at this value's caret);
 * - strip: the pending images (the steer composer's tiles + `[Image #k]`
 *   markers, EXP-698);
 * - tools: EXP-1249 — ONE "+" ([ComposerMenu.PLUS_TEST_ID]) opening the
 *   composer menu ([ComposerPlusMenuSheet]: Implement issue, Run action, Add
 *   file or image, Effort, Subagents, Ultracode, MCP servers, Computer use);
 *   the submit is the round send glyph (EXP-827: icon-only ×4), NAMED by the
 *   contract's per-subject title ("Start chat" / "Start coding" / "Start
 *   batch · N" / "Run action").
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
    pendingImages: List<PendingAttachment>,
    imageError: String?,
    sending: Boolean,
    /**
     * EXP-1271: the start in flight — once the pending delay passes, the card
     * leads with the starting row (the mark, "Starting on <device>", the
     * seconds since the press) until the run's row lands.
     */
    starting: ComposerStarting? = null,
    /** EXP-1249: the "+" — opens the composer menu. */
    onOpenMenu: () -> Unit,
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
    // Wave D: only IMAGES are numbered — a file tile carries no marker.
    val imageCount = pendingImages.count { it.isImage }
    var markedImages by remember { mutableIntStateOf(imageCount) }
    LaunchedEffect(imageCount) {
        if (imageCount > markedImages) {
            var next = value
            for (k in (markedImages + 1)..imageCount) {
                val (text, caret) = insertImageMarker(next.text, next.selection.end, k)
                next = TextFieldValue(text, TextRange(caret))
            }
            onValueRewrite(next)
        }
        markedImages = imageCount
    }
    GlassComposer(
        modifier = modifier.testTag("agent-composer"),
        // EXP-1038: the subject chips are NOT in the card any more — they head
        // the page beside the contract verb ([AgentComposerHeadline], mounted
        // by the page above this card). NEW-2: prompts carry no typed pick
        // inputs, so the card has no leading rows.
        leading = null,
        strip = {
            val startingShown = rememberDelayedFlag(starting != null)
            if (starting != null && startingShown) StartingRow(starting)
            PendingAttachmentStrip(
                items = pendingImages,
                enabled = !sending,
                onRemove = { index ->
                    // Renumber BEFORE the list shrinks: `[Image #k]` goes, and
                    // every higher marker comes down one. A file has no marker.
                    val number = pendingImages.imageNumberAt(index)
                    val renumbered = if (number != null) renumberImageMarkers(value.text, number) else value.text
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
            // A refused pick (caps): invalid input, under the field (FEED-79).
            FieldError(imageError, modifier = Modifier.padding(bottom = 4.dp))
        },
        tools = {
            // EXP-1249 ×4: the "+" is the composer's ONLY tool.
            ComposerToolButton(
                ExpIcons.uiAdd,
                contentDescription = ComposerMenu.PLUS_LABEL,
                onClick = onOpenMenu,
                enabled = !sending,
                modifier = Modifier.testTag(ComposerMenu.PLUS_TEST_ID),
            )
        },
        submit = {
            // EXP-827: the round send glyph, icon-only ×4 — the chips above the
            // field already say what the send starts, so the contract's
            // per-subject label ("Start batch · 3") stays the button's NAME
            // (its content description) instead of its width.
            ComposerSubmitButton(
                ExpIcons.uiSubmit,
                contentDescription = submitLabel,
                onClick = onSubmit,
                enabled = canSubmit,
                sending = sending,
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
private fun ChipClose(
    contentDescription: String,
    testTag: String,
    onClick: () -> Unit,
) {
    // The chip's ONE control (EXP-827): a 20dp round hit area around the
    // 10dp glyph, named for TalkBack as the remove it performs.
    val describedAs = contentDescription
    Box(
        modifier = Modifier
            .size(20.dp)
            .clip(CircleShape)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag(testTag)
            .semantics { this.contentDescription = describedAs },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.uiClose,
            contentDescription = null,
            modifier = Modifier.size(10.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/**
 * EXP-1038: the composer's HEADLINE — the run's subject as the page's main
 * element rather than a chip buried in the card. The contract verb
 * ([AgentComposerPrompt.headline], ×4: "Run" · "Implement") reads straight
 * into the chips beside it: one action chip, or the checked issues. The PAGE
 * emits this only with a subject: a chat's heading would repeat the field's
 * own "Ask the agent…" placeholder, so there is no row at all (web
 * `LaunchHeadline` returns null). The chips keep their remove affordances and their test tags
 * (`agent-composer-chip-action`, `agent-composer-chip-issue-<IDENT>`), so a
 * pick still proves itself where the screenshot suites look for it; only
 * their place changed.
 */
@Composable
internal fun AgentComposerHeadline(
    headline: String,
    issueChips: List<IssueOption>,
    onRemoveIssue: (String) -> Unit,
    actionChip: PromptDto?,
    /**
     * The ✕'s name per issue chip ("Remove APP-3").
     */
    removeIssueDescription: (IssueOption) -> String = { "Remove ${it.identifier}" },
    onClearAction: () -> Unit,
    modifier: Modifier = Modifier,
    /** NEW-3: the agent subject (Message an agent ›) — ONE chip, no verb. */
    agentChip: com.exponential.app.domain.SessionTree.AgentRow? = null,
    onClearAgent: () -> Unit = {},
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 4.dp, bottom = 2.dp)
            .testTag("agent-composer-headline"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // NEW-3: an agent's chip IS the headline (no verb).
        if (headline.isNotEmpty()) {
            Text(
                headline,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                modifier = Modifier.testTag("agent-composer-headline-verb"),
            )
        }
        if (agentChip != null) {
            // The agent: its icon in its colour · name · ✕ (only the ✕
            // clears it, like the prompt chip).
            GlassPill(
                agentChip.name,
                leading = {
                    Icon(
                        ExpIcons.byName(agentChip.icon) ?: ExpIcons.settingsAgents,
                        contentDescription = null,
                        tint = remember(agentChip.color) { com.exponential.app.ui.parseColor(agentChip.color) },
                        modifier = Modifier.size(com.exponential.app.ui.components.GlassPillDefaults.glyphSize(com.exponential.app.ui.components.PillSize.Md)),
                    )
                },
                trailing = {
                    ChipClose(
                        contentDescription = "Remove ${agentChip.name}",
                        testTag = "agent-composer-chip-agent-remove",
                        onClick = onClearAgent,
                    )
                },
                modifier = Modifier.testTag("agent-composer-chip-agent"),
            )
        } else if (actionChip != null || issueChips.isNotEmpty()) {
            // Several issue chips scroll sideways rather than wrapping — a
            // batch must not push the field off the first screen. The verb
            // keeps its width (`fill = false`), the chips take what is left.
            Row(
                modifier = Modifier
                    .weight(1f, fill = false)
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (actionChip != null) {
                    // The one action: its curated glyph · name · ✕. Only the ✕
                    // clears it (EXP-827, web and iOS agree); the pill itself
                    // is readonly.
                    GlassPill(
                        actionChip.name,
                        icon = actionGlyph(actionChip),
                        trailing = {
                            ChipClose(
                                contentDescription = "Remove ${actionChip.name}",
                                testTag = "agent-composer-chip-action-remove",
                                onClick = onClearAction,
                            )
                        },
                        modifier = Modifier.testTag("agent-composer-chip-action"),
                    )
                } else {
                    // One checked issue, as the SHARED chip (EXP-885): status
                    // glyph · mono identifier · title · ✕. Same split: the ✕
                    // removes, the body is inert.
                    issueChips.forEach { option ->
                        IssueChip(
                            identifier = option.identifier,
                            title = option.title,
                            // The composer carries the ANCHOR off the wire, not
                            // the team's status row, so the glyph comes from
                            // the builtin defaults.
                            status = remember(option.status) {
                                IssueStatusResolver.resolve(null, option.status, emptyList())
                            },
                            onRemove = { onRemoveIssue(option.id) },
                            removeContentDescription = removeIssueDescription(option),
                            removeTestTag = "agent-composer-chip-issue-${option.identifier}-remove",
                            modifier = Modifier.testTag("agent-composer-chip-issue-${option.identifier}"),
                        )
                    }
                }
            }
        }
    }
}

/**
 * EXP-1271: the card's starting row — the mark, "Starting on <device>", and
 * the seconds since the press, ticking in tenths (web `StartingRow`).
 */
@Composable
internal fun StartingRow(starting: ComposerStarting, modifier: Modifier = Modifier) {
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(starting) {
        while (true) {
            now = System.currentTimeMillis()
            delay(100)
        }
    }
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(top = 4.dp, bottom = 6.dp)
            .testTag(FeedbackStates.STARTING_TEST_ID),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FeedbackMark(size = FeedbackStates.MARK_SIZE_STATUS.dp)
        Text(
            FeedbackStates.startingLabel(starting.deviceLabel),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        Text(
            startingElapsed(now - starting.sinceMs),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}

/** The starting row's clock: tenths of a second, "1.4 s" (web `startingElapsed`). */
internal fun startingElapsed(ms: Long): String =
    String.format(java.util.Locale.US, "%.1f s", ms.coerceAtLeast(0) / 1000.0)
