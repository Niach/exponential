package com.exponential.app.ui.issue

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.ui.components.FloatingBarRung
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.exponential.app.domain.MAX_COMMENT_ATTACHMENTS
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.ReporterReply
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.BarCapsule
import com.exponential.app.ui.components.BarCircle
import com.exponential.app.ui.components.ComposerSubmitButton
import com.exponential.app.ui.components.FloatingBottomBar
import com.exponential.app.ui.components.ComposerToolButton
import com.exponential.app.ui.components.GlassComposer
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.emoji.EmojiPickerSheet
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.EditorModel
import com.exponential.app.ui.markdown.LocalMarkdownToolbarController
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest

// What the Work screen's start circle renders (EXP-240/EXP-893, EXP-1121):
// derived from the "Ready to code?" model. Null hides the circle (steer off /
// non-member, `readiness.visible == false`) — the host owns the mapping. A LIVE
// run is not a state of this control: the run is a face of the same screen,
// one tab away (EXP-1150).
sealed interface StartButtonUi {
    /** Every step met: the white play glyph, a tap opens the composer. */
    data object Ready : StartButtonUi

    /** Inputs still loading: the plain circle, a muted glyph, inert. */
    data object Loading : StartButtonUi

    /** A step is missing: dashed stroke + amber dot, a tap opens the checklist. */
    data class NotReady(val caption: String?) : StartButtonUi

    companion object {
        fun from(readiness: CodingReadiness.Readiness): StartButtonUi? = when {
            !readiness.visible -> null
            readiness.loading -> Loading
            readiness.ready -> Ready
            else -> NotReady(readiness.caption)
        }
    }
}

/** Test tag of a READY Start coding circle (the store-screenshot flow waits on it). */
const val START_CODING_READY_TAG = "start-coding-ready"

/**
 * SLOP-4: the composer's "Reply to reporter" pill — the host builds one ONLY
 * when the issue's submission has a reporter email (null hides the pill).
 * [reporterName] = the resolved display name the ON placeholder addresses.
 */
data class ReporterToggle(
    val reporterName: String,
    val on: Boolean,
    val onToggle: (Boolean) -> Unit,
)

/** The dashed ring + amber badge a not-ready Start coding wears (EXP-1121). */
internal object StartReadinessStyle {
    val Amber: Color = DesignTokens.Semantic.Yellow
    val DashOn = 3.dp
    val DashOff = 3.dp
    val DashWidth = 1.dp
    val BadgeSize = 8.dp
    val BadgeRing = 1.5.dp
}

/** A 1dp dashed outline in the circle's hairline colour, following [shape]'s outline. */
internal fun Modifier.dashedOutline(shape: Shape, color: Color = GlassTokens.StrokeStrong): Modifier =
    drawWithContent {
        drawContent()
        val stroke = StartReadinessStyle.DashWidth.toPx()
        val inset = stroke / 2f
        val outline = shape.createOutline(
            Size(size.width - stroke, size.height - stroke),
            layoutDirection,
            this,
        )
        val path = Path().apply { addOutline(outline) }
        translate(inset, inset) {
            drawPath(
                path,
                color = color,
                style = Stroke(
                    width = stroke,
                    pathEffect = PathEffect.dashPathEffect(
                        floatArrayOf(StartReadinessStyle.DashOn.toPx(), StartReadinessStyle.DashOff.toPx()),
                    ),
                ),
            )
        }
    }

/** The small amber dot riding a not-ready control's top-right corner, ringed
 *  with the page background so it reads as cut out of the stroke. */
@Composable
internal fun ReadinessBadgeDot(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .size(StartReadinessStyle.BadgeSize + StartReadinessStyle.BadgeRing * 2)
            .clip(CircleShape)
            .background(GlassTokens.BackgroundBottom)
            .padding(StartReadinessStyle.BadgeRing)
            .clip(CircleShape)
            .background(StartReadinessStyle.Amber),
    )
}

/** The 52dp Start-coding circle for the bar's right slot (EXP-893 host). */
@Composable
fun StartCircle(ui: StartButtonUi, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val caption = (ui as? StartButtonUi.NotReady)?.caption
    // On the clickable node itself, so TalkBack reads "Start coding" plus
    // the first missing step as its state.
    val a11y = Modifier.semantics {
        contentDescription = CodingReadiness.Copy.START
        caption?.let { stateDescription = it }
    }
    Box(modifier = modifier.size(FloatingBarRung)) {
        val glyph: @Composable () -> Unit = {
            Icon(
                ExpIcons.actionRun,
                contentDescription = null,
                modifier = Modifier.size(22.dp),
                tint = Color.White.copy(
                    alpha = if (ui is StartButtonUi.Ready) 1f else TextEmphasis.Quaternary,
                ),
            )
        }
        when (ui) {
            is StartButtonUi.NotReady -> {
                Box(
                    modifier = Modifier
                        .matchParentSize()
                        .clip(CircleShape)
                        .background(GlassTokens.OpaqueCardFill)
                        .dashedOutline(CircleShape)
                        .then(a11y)
                        .clickable(role = Role.Button, onClick = onClick),
                    contentAlignment = Alignment.Center,
                ) { glyph() }
                ReadinessBadgeDot(
                    Modifier.align(Alignment.TopEnd).offset(x = (-2).dp, y = 2.dp),
                )
            }
            // Loading stays inert: no dashed/amber flash while the inputs land.
            else -> BarCircle(
                onClick = onClick,
                enabled = ui is StartButtonUi.Ready,
                // A test tag only once every step is met: an instrumented tap
                // on the (inert) loading circle would silently do nothing.
                modifier = Modifier
                    .matchParentSize()
                    .then(a11y)
                    .then(if (ui is StartButtonUi.Ready) Modifier.testTag(START_CODING_READY_TAG) else Modifier),
            ) { glyph() }
        }
    }
}

// The four signals collapse-on-blur watches, as one snapshotFlow value (Kotlin
// stops at Triple, and all four have to be observed together).
private data class ComposerBlurState(
    val focused: Boolean,
    val ime: Boolean,
    val emojiOpen: Boolean,
    val otherEditorFocused: Boolean,
)

/**
 * The floating three-element bottom bar of the issue detail (EXP-240), cloning
 * the main BottomNavBar treatment (near-opaque pill fill + hairline stroke):
 * a Properties circle (moderators), the expanding Comment pill, and the
 * Start-coding circle. Tapping the pill morphs the bar into the docked
 * comment composer; the host applies the single `imePadding` so the whole bar
 * rides the keyboard and stacks above the markdown toolbar.
 */
@Composable
fun IssueDetailBottomBar(
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    showProperties: Boolean,
    onOpenProperties: () -> Unit,
    /** EXP-893: the right circle — the host's Start (EXP-1150), else empty. */
    trailing: @Composable () -> Unit,
    /** EXP-1191: the Merge circle right of the Comment capsule; null = none. */
    merge: (@Composable () -> Unit)? = null,
    draft: String,
    onDraftChange: (String) -> Unit,
    sending: Boolean,
    onSend: () -> Unit,
    // EXP-554: files queued for the next comment. They upload on send and
    // link to the comment as attachments — never inlined into its markdown.
    pendingAttachments: List<PendingAttachment>,
    onAddAttachment: (Uri) -> Unit,
    onRemoveAttachment: (Int) -> Unit,
    mentionMembers: List<MentionMember>,
    // Solo teams hide the @ button (nobody else to mention, EXP-246) — same
    // gate the assignee chip uses, threaded explicitly from the screen.
    showMentionButton: Boolean = true,
    // EXP-568: another markdown editor on the screen (the description, or a
    // comment being edited) holds the keyboard. The screen computes it from the
    // toolbar controller's activeModel; the composer opts out of that toolbar,
    // so it is never the registered model itself (re-checked below by identity).
    otherEditorFocused: Boolean = false,
    // EXP-741: the comment this composer replies to ("Replying to …" leading
    // row, `parentId` on send). The ✕ clears it; so does a collapse.
    replyTarget: CommentReplyTarget? = null,
    onClearReply: () -> Unit = {},
    // SLOP-4: the "Reply to reporter" pill, in the leading row the reply
    // target otherwise takes (a reply under a card is team-only, so the pill
    // never shows beside "Replying to …").
    reporterToggle: ReporterToggle? = null,
    modifier: Modifier = Modifier,
) {
    // The composer's editor model lives at bar level so the block document
    // survives collapse/expand (the VM's draft string survives even further —
    // rotation and re-navigation).
    val composerModel = remember { EditorModel() }

    // EXP-551: the emoji sheet is hosted at BAR level, above the expand/collapse
    // AnimatedContent — it is focusable, so opening it drops the keyboard, and
    // collapse-on-blur would otherwise fold the composer (and the sheet with
    // it) away underneath the picker.
    var emojiPickerOpen by remember { mutableStateOf(false) }
    val emojiPickerOpenState = rememberUpdatedState(emojiPickerOpen)
    if (emojiPickerOpen) {
        EmojiPickerSheet(
            onPick = { unicode -> composerModel.insertPlainText(unicode) },
            onDismiss = {
                emojiPickerOpen = false
                // Hand focus back so a dismissed picker leaves the composer
                // exactly as it was found (keyboard up, caret where it was).
                composerModel.setFocused(
                    composerModel.activeRowId ?: composerModel.rows.firstOrNull()?.id,
                )
            },
        )
    }

    // Collapse-on-blur: only once focus is gone AND the keyboard is fully down
    // (toolbar taps transiently null focusedRowId with the IME still up), only
    // after a ~200ms quiet period, only with an empty draft (never lose one),
    // and only while resumed (the photo picker backgrounds the activity).
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val imeVisibleState = rememberUpdatedState(imeVisible)
    val draftState = rememberUpdatedState(draft)
    // Queued attachments count as content: collapsing on them would hide the
    // strip while the files stay queued in the VM.
    val pendingState = rememberUpdatedState(pendingAttachments)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    // Identity re-check (EXP-568): the composer never registers with the shared
    // toolbar (showToolbar = false), so a registered model is by definition some
    // other editor — but assert it rather than trust it, since a future opt-in
    // would otherwise make the composer collapse itself.
    val toolbarController = LocalMarkdownToolbarController.current
    val otherEditorFocusedState = rememberUpdatedState(
        otherEditorFocused && toolbarController?.activeModel !== composerModel,
    )
    LaunchedEffect(expanded) {
        if (!expanded) return@LaunchedEffect
        var hadFocus = false
        snapshotFlow {
            ComposerBlurState(
                composerModel.focusedRowId != null,
                imeVisibleState.value,
                emojiPickerOpenState.value,
                otherEditorFocusedState.value,
            )
        }.collectLatest { (focused, ime, emojiOpen, otherFocused) ->
            if (focused) {
                hadFocus = true
                return@collectLatest
            }
            // The emoji picker takes focus AND the keyboard by design
            // (EXP-551) — collapsing under it would unmount the composer the
            // pick is meant to land in.
            //
            // EXP-568: a still-visible keyboard normally means a transient
            // toolbar-tap blur, so it blocks the collapse — UNLESS another
            // editor is what took it, in which case the composer must fold away
            // instead of stacking a second editor on screen. The draft is never
            // lost either way: a non-empty one keeps the composer expanded (and
            // the VM holds the text regardless).
            if (!hadFocus || (ime && !otherFocused) || emojiOpen) return@collectLatest
            delay(200)
            val empty = draftState.value.isBlank() && pendingState.value.isEmpty() &&
                composerModel.currentMarkdown().isBlank()
            if (empty && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                onExpandedChange(false)
            }
        }
    }

    // EXP-523: `transitionSpec` is a plain lambda, not a composable one, so the
    // reduce-motion flag is read here and captured.
    val reduceMotion = LocalReduceMotion.current
    AnimatedContent(
        targetState = expanded,
        transitionSpec = {
            // The shared `slow` token replaces the local ExpandMs (same
            // 280ms), and reduce-motion now collapses it to `snap()`.
            val spec = Motion.slow<Float>(reduceMotion)
            (fadeIn(spec) togetherWith fadeOut(spec))
                .using(SizeTransform(clip = false))
        },
        label = "issue-bottom-bar",
        modifier = modifier.fillMaxWidth(),
    ) { isExpanded ->
        if (isExpanded) {
            ExpandedCommentComposer(
                model = composerModel,
                draft = draft,
                onDraftChange = onDraftChange,
                sending = sending,
                onSend = onSend,
                pendingAttachments = pendingAttachments,
                onAddAttachment = onAddAttachment,
                onRemoveAttachment = onRemoveAttachment,
                mentionMembers = mentionMembers,
                showMentionButton = showMentionButton,
                onRequestEmoji = { emojiPickerOpen = true },
                onCollapse = { onExpandedChange(false) },
                replyTarget = replyTarget,
                onClearReply = onClearReply,
                reporterToggle = reporterToggle,
            )
        } else {
            CollapsedBar(
                showProperties = showProperties,
                onOpenProperties = onOpenProperties,
                trailing = trailing,
                merge = merge,
                onExpand = { onExpandedChange(true) },
            )
        }
    }
}

@Composable
private fun CollapsedBar(
    showProperties: Boolean,
    onOpenProperties: () -> Unit,
    trailing: @Composable () -> Unit,
    merge: (@Composable () -> Unit)?,
    onExpand: () -> Unit,
) {
    // EXP-893: the shared Work-screen bar chrome — properties circle, the
    // `+ Comment` capsule, and whatever the host puts on the right.
    FloatingBottomBar(
        left = if (showProperties) {
            {
                BarCircle(onClick = onOpenProperties) {
                    Icon(
                        ExpIcons.uiProperties,
                        contentDescription = "Issue properties",
                        modifier = Modifier.size(20.dp),
                        tint = Color.White,
                    )
                }
            }
        } else {
            null
        },
        right = trailing,
        afterCentre = merge,
    ) {
        BarCapsule(
            label = "Comment",
            icon = ExpIcons.uiAdd,
            onClick = onExpand,
        )
    }
}

// The docked composer the pill expands into: the pending-attachment strip over
// a MarkdownEditor over a [photo][file][@][#][spacer][send] row. EXP-554:
// picked images/files become real comment ATTACHMENTS (uploaded on send), so
// nothing here ever mints a `draft://` block — the editor gets no uploader.
@Composable
private fun ExpandedCommentComposer(
    model: EditorModel,
    draft: String,
    onDraftChange: (String) -> Unit,
    sending: Boolean,
    onSend: () -> Unit,
    pendingAttachments: List<PendingAttachment>,
    onAddAttachment: (Uri) -> Unit,
    onRemoveAttachment: (Int) -> Unit,
    mentionMembers: List<MentionMember>,
    showMentionButton: Boolean,
    // EXP-551: the composer opts out of the floating toolbar, so the bar hosts
    // the emoji sheet for it (hosting it HERE would let collapse-on-blur
    // unmount the sheet the moment it takes focus off the editor).
    onRequestEmoji: () -> Unit,
    onCollapse: () -> Unit,
    replyTarget: CommentReplyTarget?,
    onClearReply: () -> Unit,
    reporterToggle: ReporterToggle?,
) {
    BackHandler(onBack = onCollapse)
    // The composer owns its own pickers (the shared toolbar controller's
    // onPickImage is a last-focus-wins slot that can still point at the
    // description editor). Both feed the VM's pending list, never the model.
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(MAX_COMMENT_ATTACHMENTS),
    ) { uris: List<Uri> -> uris.forEach(onAddAttachment) }
    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri: Uri? -> uri?.let(onAddAttachment) }

    val canSend = draft.isNotBlank() || pendingAttachments.isNotEmpty()
    // EXP-741: the reply target rides the composer's leading row; SLOP-4: so
    // does the "Reply to reporter" pill, on a top-level comment (never beside
    // "Replying to …" — a reply under a card is team-only).
    val leading: (@Composable ColumnScope.() -> Unit)? = when {
        replyTarget != null -> {
            {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        "Replying to ${replyTarget.authorName}",
                        style = MaterialTheme.typography.bodySmall,
                        color = Color.White.copy(alpha = TextEmphasis.Secondary),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    CompositionLocalProvider(
                        LocalMinimumInteractiveComponentSize provides Dp.Unspecified,
                    ) {
                        IconButton(onClick = onClearReply, modifier = Modifier.size(28.dp)) {
                            Icon(
                                ExpIcons.uiClose,
                                contentDescription = "Stop replying",
                                modifier = Modifier.size(14.dp),
                                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                            )
                        }
                    }
                }
            }
        }
        reporterToggle != null -> {
            {
                // The ONE select pill (the slot the old ticket composer's
                // Reply / Internal-note pair used), OFF by default.
                Row(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.padding(bottom = 8.dp),
                ) {
                    GlassPill(
                        ReporterReply.TOGGLE_LABEL,
                        size = PillSize.Sm,
                        mode = PillMode.Select,
                        selected = reporterToggle.on,
                        onClick = { reporterToggle.onToggle(!reporterToggle.on) },
                        modifier = Modifier.testTag("reply-to-reporter"),
                    )
                }
            }
        }
        else -> null
    }
    GlassComposer(
        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
        // The composer floats over the issue's own scrolling content.
        opaque = true,
        leading = leading,
        strip = {
            PendingAttachmentStrip(
                items = pendingAttachments,
                enabled = !sending,
                onRemove = onRemoveAttachment,
            )
        },
        tools = {
            ComposerToolButton(
                ExpIcons.editorImage,
                contentDescription = "Attach image",
                onClick = {
                    // EXP-824: videos attach too (transcoded + postered by the VM).
                    imagePicker.launch(
                        PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo),
                    )
                },
            )
            ComposerToolButton(
                ExpIcons.uiAttach,
                contentDescription = "Attach file",
                onClick = { filePicker.launch(arrayOf("*/*")) },
            )
            if (showMentionButton) {
                ComposerToolButton(
                    ExpIcons.editorMention,
                    contentDescription = "Mention a member",
                    onClick = { model.insertPlainText("@") },
                )
            }
            ComposerToolButton(
                ExpIcons.editorIssueRef,
                contentDescription = "Reference an issue",
                onClick = { model.insertPlainText("#") },
            )
            ComposerToolButton(
                ExpIcons.editorEmoji,
                contentDescription = "Insert emoji",
                onClick = onRequestEmoji,
            )
        },
        submit = {
            ComposerSubmitButton(
                ExpIcons.uiSubmit,
                contentDescription = "Send",
                onClick = onSend,
                enabled = canSend,
                sending = sending,
            )
        },
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 44.dp, max = 160.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            MarkdownEditor(
                markdown = draft,
                editable = true,
                onChange = onDraftChange,
                // No uploader: an image pasted or picked into a COMMENT is an
                // attachment, not an inline markdown block (EXP-554).
                onUploadImage = null,
                placeholder = when {
                    replyTarget != null -> "Leave a reply…"
                    // SLOP-4: the ON placeholder names the reporter (and says
                    // the words leave the team).
                    reporterToggle?.on == true -> ReporterReply.placeholderOn(reporterToggle.reporterName)
                    else -> "Write a comment…"
                },
                minHeight = 40.dp,
                mentionMembers = mentionMembers,
                // The composer carries its own image/@/# row below — the
                // floating formatting strip never shows for it (EXP-246).
                showToolbar = false,
                model = model,
            )
        }
        // Focus the first row once the editor (declared above, so its
        // markdown-load effect runs first) is in place; BlockTextField's retry
        // loop lands the OS focus and raises the keyboard.
        LaunchedEffect(Unit) {
            model.setFocused(model.rows.firstOrNull()?.id)
        }
    }
}
