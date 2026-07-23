package com.exponential.app.ui.issue

import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AlternateEmail
import androidx.compose.material.icons.filled.ArrowCircleUp
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.exponential.app.domain.CodingSessionDisplayState
import com.exponential.app.ui.components.BottomBarPillFill
import com.exponential.app.ui.markdown.EditorModel
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.hasDraftImages
import com.exponential.app.ui.markdown.rememberMarkdownImagePicker
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest

// What the right-hand start circle renders (EXP-240): the play launcher
// (dimmed while no desktop is online), the in-flight spinner, or the live
// session's state dot (reusing PulsingDot/StaticDot). Null hides the circle
// (steer off / non-member / repo-less board) — the screen owns the mapping.
sealed interface StartButtonUi {
    data class Start(val enabled: Boolean) : StartButtonUi
    data object Sending : StartButtonUi
    data class Session(val state: CodingSessionDisplayState) : StartButtonUi
}

private val BarStroke = Color.White.copy(alpha = 0.12f)
private const val ExpandMs = 280

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
    startButton: StartButtonUi?,
    onStartClick: () -> Unit,
    draft: String,
    onDraftChange: (String) -> Unit,
    sending: Boolean,
    onSend: () -> Unit,
    onUploadImage: suspend (Uri) -> String?,
    mentionMembers: List<MentionMember>,
    modifier: Modifier = Modifier,
) {
    // The composer's editor model lives at bar level so the block document
    // survives collapse/expand (the VM's draft string survives even further —
    // rotation and re-navigation).
    val composerModel = remember { EditorModel() }

    // Collapse-on-blur: only once focus is gone AND the keyboard is fully down
    // (toolbar taps transiently null focusedRowId with the IME still up), only
    // after a ~200ms quiet period, only with an empty draft (never lose one),
    // and only while resumed (the photo picker backgrounds the activity).
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val imeVisibleState = rememberUpdatedState(imeVisible)
    val draftState = rememberUpdatedState(draft)
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(expanded) {
        if (!expanded) return@LaunchedEffect
        var hadFocus = false
        snapshotFlow {
            (composerModel.focusedRowId != null) to imeVisibleState.value
        }.collectLatest { (focused, ime) ->
            if (focused) {
                hadFocus = true
                return@collectLatest
            }
            if (!hadFocus || ime) return@collectLatest
            delay(200)
            val empty = draftState.value.isBlank() && composerModel.currentMarkdown().isBlank()
            if (empty && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                onExpandedChange(false)
            }
        }
    }

    AnimatedContent(
        targetState = expanded,
        transitionSpec = {
            (fadeIn(tween(ExpandMs)) togetherWith fadeOut(tween(ExpandMs)))
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
                onUploadImage = onUploadImage,
                mentionMembers = mentionMembers,
                onCollapse = { onExpandedChange(false) },
            )
        } else {
            CollapsedBar(
                showProperties = showProperties,
                onOpenProperties = onOpenProperties,
                startButton = startButton,
                onStartClick = onStartClick,
                onExpand = { onExpandedChange(true) },
            )
        }
    }
}

@Composable
private fun CollapsedBar(
    showProperties: Boolean,
    onOpenProperties: () -> Unit,
    startButton: StartButtonUi?,
    onStartClick: () -> Unit,
    onExpand: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (showProperties) {
            BarCircle(onClick = onOpenProperties) {
                Icon(
                    Icons.Filled.Tune,
                    contentDescription = "Issue properties",
                    modifier = Modifier.size(20.dp),
                    tint = Color.White,
                )
            }
        }
        // The comment pill — capsule with a `+` and tertiary placeholder text.
        val capsule = RoundedCornerShape(percent = 50)
        Row(
            modifier = Modifier
                .weight(1f)
                .height(52.dp)
                .clip(capsule)
                .background(BottomBarPillFill)
                .border(GlassTokens.Hairline, BarStroke, capsule)
                .clickable(onClick = onExpand)
                .padding(horizontal = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Filled.Add,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
                tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                "Comment",
                style = MaterialTheme.typography.bodyMedium,
                color = Color.White.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        if (startButton != null) {
            BarCircle(onClick = onStartClick) {
                when (startButton) {
                    is StartButtonUi.Start -> Icon(
                        Icons.Filled.PlayArrow,
                        contentDescription = "Start coding",
                        modifier = Modifier.size(22.dp),
                        tint = Color.White.copy(
                            alpha = if (startButton.enabled) 1f else TextEmphasis.Quaternary,
                        ),
                    )
                    is StartButtonUi.Sending -> CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = Color.White,
                    )
                    is StartButtonUi.Session -> when (startButton.state) {
                        CodingSessionDisplayState.Running -> PulsingDot(size = 10.dp)
                        CodingSessionDisplayState.NeedsInput -> StaticDot(NeedsInputAmber, size = 10.dp)
                        CodingSessionDisplayState.Review -> StaticDot(ReviewGreen, size = 10.dp)
                        CodingSessionDisplayState.Done -> StaticDot(DoneBlue, size = 10.dp)
                    }
                }
            }
        }
    }
}

@Composable
private fun BarCircle(
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(52.dp)
            .clip(CircleShape)
            .background(BottomBarPillFill)
            .border(GlassTokens.Hairline, BarStroke, CircleShape)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

// The docked composer the pill expands into: MarkdownEditor over a
// [photo][@][spacer][send] row. Ports the send gating from the old inline
// end-of-thread composer verbatim (draft:// placeholders block Send).
@Composable
private fun ExpandedCommentComposer(
    model: EditorModel,
    draft: String,
    onDraftChange: (String) -> Unit,
    sending: Boolean,
    onSend: () -> Unit,
    onUploadImage: suspend (Uri) -> String?,
    mentionMembers: List<MentionMember>,
    onCollapse: () -> Unit,
) {
    BackHandler(onBack = onCollapse)
    // The composer owns its own photo-picker launcher targeting ITS model. The
    // shared toolbar controller's onPickImage is a last-focus-wins slot that can
    // still point at the description editor (it is only overwritten on focus
    // gain), which would insert the picked image into the description instead.
    val pickImage = rememberMarkdownImagePicker(model, onUploadImage)

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 8.dp)
            .clip(RoundedCornerShape(24.dp))
            .background(BottomBarPillFill)
            .border(GlassTokens.Hairline, BarStroke, RoundedCornerShape(24.dp))
            .padding(horizontal = 14.dp, vertical = 8.dp),
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
                onUploadImage = onUploadImage,
                placeholder = "Write a comment…",
                minHeight = 40.dp,
                mentionMembers = mentionMembers,
                model = model,
            )
        }
        // Focus the first row once the editor (declared above, so its
        // markdown-load effect runs first) is in place; BlockTextField's retry
        // loop lands the OS focus and raises the keyboard.
        LaunchedEffect(Unit) {
            model.setFocused(model.rows.firstOrNull()?.id)
        }
        val hasPendingImages = remember(draft) { hasDraftImages(draft) }
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = pickImage) {
                Icon(
                    Icons.Filled.Image,
                    contentDescription = "Add image",
                    modifier = Modifier.size(20.dp),
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
            }
            IconButton(onClick = { model.insertPlainText("@") }) {
                Icon(
                    Icons.Filled.AlternateEmail,
                    contentDescription = "Mention a member",
                    modifier = Modifier.size(20.dp),
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
            }
            Spacer(Modifier.weight(1f))
            if (hasPendingImages) {
                Text(
                    "Waiting for images…",
                    style = MaterialTheme.typography.labelSmall,
                    color = CommentMeta,
                )
                Spacer(Modifier.width(8.dp))
            }
            IconButton(
                onClick = onSend,
                enabled = !sending && draft.isNotBlank() && !hasPendingImages,
            ) {
                Icon(
                    Icons.Filled.ArrowCircleUp,
                    contentDescription = "Send",
                    modifier = Modifier.size(30.dp),
                    tint = if (draft.isBlank() || hasPendingImages) {
                        Color.White.copy(alpha = 0.3f)
                    } else AccentIndigo,
                )
            }
        }
    }
}
