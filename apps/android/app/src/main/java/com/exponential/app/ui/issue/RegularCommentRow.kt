package com.exponential.app.ui.issue

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.isViaMcp
import com.exponential.app.domain.MAX_COMMENT_ATTACHMENTS
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.ui.components.CommentAttachmentsStrip
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.LargeCommentAttachments
import com.exponential.app.ui.components.PendingAttachmentStrip
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.emoji.EmojiPickerSheet
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.EditorModel
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

/** The edit/delete callbacks of ONE comment — a top-level card and each
 *  reply under it get their own set (web `CommentCardProps`). */
internal data class CommentCardActions(
    val onEdit: () -> Unit,
    val onCancelEdit: () -> Unit,
    /** (body, kept attachment ids) — the ids the edit KEEPS; the VM appends
     *  whatever it uploads and sends the union as the full desired set. */
    val onSaveEdit: (String, List<String>) -> Unit,
    val onDelete: () -> Unit,
)

// One human comment in the thread: a rounded glass card (author + relative
// time + markdown body) with the avatar sitting in the timeline gutter
// (EXP-240), rail segments above/below keeping the line continuous like the
// event rows. The edit/delete overflow is AUTHOR-ONLY (EXP-398) — matching the
// server, which no longer lets a global admin touch someone else's comment.
//
// EXP-741: the card is the THREAD — its replies sit indented under the body
// behind one hairline, each with a 20dp avatar, and the "Leave a reply…" row
// closes every top-level card (it hands the docked composer a reply target).
@Composable
internal fun RegularCommentRow(
    comment: CommentEntity,
    replies: List<CommentEntity>,
    lineAbove: Boolean,
    lineBelow: Boolean,
    usersById: Map<String, UserEntity>,
    currentUserId: String?,
    editingId: String?,
    actions: (CommentEntity) -> CommentCardActions,
    onReply: () -> Unit,
    // EXP-554: every comment's linked attachments (squared thumbs + file
    // chips below the body, never inlined), plus the edit-mode add queue of
    // whichever comment is being edited.
    attachmentsByComment: Map<String, List<AttachmentEntity>>,
    onOpenAttachment: (AttachmentEntity) -> Unit,
    editAttachments: List<PendingAttachment>,
    onAddEditAttachment: (Uri, Int) -> Unit,
    onRemoveEditAttachment: (Int) -> Unit,
    mentionMembers: List<MentionMember>,
) {
    val author = usersById[comment.authorId]
    Row(
        modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
        verticalAlignment = Alignment.Top,
    ) {
        // Avatar in the timeline gutter, aligned with the event dots' column;
        // the 6dp top segment matches the card's outer margin so the rail runs
        // continuously through comment cards.
        Column(
            modifier = Modifier.width(TimelineGutterWidth).fillMaxHeight(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Box(
                Modifier
                    .height(6.dp)
                    .width(1.dp)
                    .background(if (lineAbove) TimelineRail else Color.Transparent),
            )
            // EXP-698 r4: the author's PICTURE when they have one, else their
            // initials on their own hashed hue — the same avatar every other
            // surface draws, not a comment-only glass chip.
            UserAvatar(
                user = author,
                nameOrEmail = userDisplayName(author, comment.authorId),
                size = 26.dp,
                // The author row may not have synced (or may have left the
                // team); the comment still carries the id web/iOS hash, so the
                // hue stays the same person's everywhere.
                userId = comment.authorId,
            )
            Box(
                Modifier
                    .weight(1f)
                    .width(1.dp)
                    .background(if (lineBelow) TimelineRail else Color.Transparent),
            )
        }
        Spacer(Modifier.width(8.dp))
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(vertical = 6.dp)
                .glassCard()
                // EXP-723: 12 / 10 / 12 — the body breathes inside the card
                // (the reply row below carries the last 6 of the bottom 12).
                .padding(start = 12.dp, top = 10.dp, end = 12.dp, bottom = 6.dp),
        ) {
            CommentCardContent(
                comment = comment,
                author = author,
                isAuthor = currentUserId != null && comment.authorId == currentUserId,
                isEditing = editingId == comment.id,
                actions = actions(comment),
                attachments = attachmentsByComment[comment.id].orEmpty(),
                onOpenAttachment = onOpenAttachment,
                editAttachments = if (editingId == comment.id) editAttachments else emptyList(),
                onAddEditAttachment = onAddEditAttachment,
                onRemoveEditAttachment = onRemoveEditAttachment,
                mentionMembers = mentionMembers,
            )

            // EXP-741: the replies block + the reply row, behind one hairline.
            HorizontalDivider(
                modifier = Modifier.padding(top = 12.dp, bottom = 2.dp),
                thickness = GlassTokens.Hairline,
                color = GlassTokens.StrokeCard,
            )
            replies.forEach { reply ->
                key(reply.id) {
                    val replyAuthor = usersById[reply.authorId]
                    Row(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
                        verticalAlignment = Alignment.Top,
                    ) {
                        UserAvatar(
                            user = replyAuthor,
                            nameOrEmail = userDisplayName(replyAuthor, reply.authorId),
                            size = 20.dp,
                            userId = reply.authorId,
                            modifier = Modifier.padding(top = 2.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            CommentCardContent(
                                comment = reply,
                                author = replyAuthor,
                                isAuthor = currentUserId != null && reply.authorId == currentUserId,
                                isEditing = editingId == reply.id,
                                actions = actions(reply),
                                attachments = attachmentsByComment[reply.id].orEmpty(),
                                onOpenAttachment = onOpenAttachment,
                                editAttachments = if (editingId == reply.id) editAttachments else emptyList(),
                                onAddEditAttachment = onAddEditAttachment,
                                onRemoveEditAttachment = onRemoveEditAttachment,
                                mentionMembers = mentionMembers,
                            )
                        }
                    }
                }
            }
            Text(
                "Leave a reply…",
                style = MaterialTheme.typography.bodySmall,
                color = CommentMeta,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onReply)
                    .padding(vertical = 6.dp)
                    .testTag("comment-reply-${comment.id}"),
            )
        }
    }
}

/** The header line + body/edit form + attachment strip of ONE comment — the
 *  top-level card's content, and each reply's (web `CommentCardContent`). */
@Composable
private fun CommentCardContent(
    comment: CommentEntity,
    author: UserEntity?,
    isAuthor: Boolean,
    isEditing: Boolean,
    actions: CommentCardActions,
    attachments: List<AttachmentEntity>,
    onOpenAttachment: (AttachmentEntity) -> Unit,
    editAttachments: List<PendingAttachment>,
    onAddEditAttachment: (Uri, Int) -> Unit,
    onRemoveEditAttachment: (Int) -> Unit,
    mentionMembers: List<MentionMember>,
) {
    val bodyText = remember(comment.body) { getCommentBodyText(comment.body) }
    var draft by remember(comment.id, isEditing) { mutableStateOf(bodyText) }
    var menuOpen by remember { mutableStateOf(false) }
    // The already-linked attachments this edit keeps. Snapshotted when the
    // editor opens: removing a tile only drops it here — the save's id set is
    // the full desired set, so the server hard-deletes what is missing.
    var keptIds by remember(comment.id, isEditing) {
        mutableStateOf(attachments.map { it.id })
    }
    val keptAttachments = remember(attachments, keptIds) {
        attachments.filter { it.id in keptIds }
    }
    val imagePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.PickMultipleVisualMedia(MAX_COMMENT_ATTACHMENTS),
    ) { uris: List<Uri> -> uris.forEach { onAddEditAttachment(it, keptIds.size) } }
    val filePicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri: Uri? -> uri?.let { onAddEditAttachment(it, keptIds.size) } }
    // EXP-551: a hoisted model so the emoji button can insert at the caret of
    // THIS editor (the shared toolbar controller's slot is last-focus-wins and
    // may point at another editor on the screen).
    val editModel = remember(comment.id) { EditorModel() }
    var emojiPickerOpen by remember { mutableStateOf(false) }
    if (emojiPickerOpen) {
        EmojiPickerSheet(
            onPick = { unicode -> editModel.insertPlainText(unicode) },
            onDismiss = { emojiPickerOpen = false },
        )
    }

    Column(modifier = Modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                userDisplayName(author, comment.authorId),
                // EXP-723: the author's name is the card's title — a
                // notch larger and medium-weight, over a muted time.
                style = MaterialTheme.typography.titleSmall.copy(
                    fontWeight = FontWeight.Medium,
                ),
                color = CommentAuthor,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            Spacer(Modifier.width(6.dp))
            // EXP-741: "via MCP" — an agent posted it over MCP, the same
            // caption on every client.
            Text(
                relativeTime(comment.createdAt) +
                    (if (comment.editedAt != null) " · edited" else "") +
                    (if (comment.isViaMcp) " · via MCP" else ""),
                style = MaterialTheme.typography.bodySmall,
                color = CommentMeta,
            )
            if (isAuthor && !isEditing) {
                Spacer(Modifier.weight(1f))
                Box {
                    // EXP-698 r5: a BARE vertical ⋮ on every client — a
                    // glass ring around it made the comment's overflow
                    // look heavier than the comment. The 32dp box is the
                    // hit target (M3's 48dp minimum is suppressed so the
                    // header row stays the height of its text, EXP-398).
                    CompositionLocalProvider(
                        LocalMinimumInteractiveComponentSize provides Dp.Unspecified,
                    ) {
                        IconButton(
                            onClick = { menuOpen = true },
                            modifier = Modifier.size(32.dp),
                        ) {
                            Icon(
                                ExpIcons.uiMoreVertical,
                                contentDescription = "Comment actions",
                                modifier = Modifier.size(16.dp),
                                tint = CommentMeta,
                            )
                        }
                    }
                    GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                        GlassMenuItem(
                            leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                            text = { Text("Edit") },
                            onClick = { menuOpen = false; actions.onEdit() },
                        )
                        GlassMenuItem(
                            leadingIcon = { Icon(ExpIcons.uiDelete, contentDescription = null) },
                            text = { Text("Delete") },
                            destructive = true,
                            onClick = { menuOpen = false; actions.onDelete() },
                        )
                    }
                }
            }
        }
        if (isEditing) {
            MarkdownEditor(
                markdown = draft,
                editable = true,
                onChange = { draft = it },
                // A picked/pasted image in a COMMENT is an attachment, not
                // an inline markdown block (EXP-554) — no uploader here.
                onUploadImage = null,
                placeholder = "Edit comment…",
                minHeight = 40.dp,
                mentionMembers = mentionMembers,
                model = editModel,
                modifier = Modifier.fillMaxWidth(),
            )
            CommentAttachmentsStrip(
                attachments = keptAttachments,
                onOpen = onOpenAttachment,
                onRemove = { removed -> keptIds = keptIds - removed.id },
            )
            PendingAttachmentStrip(
                items = editAttachments,
                enabled = true,
                onRemove = onRemoveEditAttachment,
                modifier = Modifier.padding(top = 6.dp),
            )
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Icon(
                    ExpIcons.editorImage,
                    contentDescription = "Attach image",
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .clip(RoundedCornerShape(percent = 50))
                        .clickable {
                            imagePicker.launch(
                                PickVisualMediaRequest(
                                    ActivityResultContracts.PickVisualMedia.ImageOnly,
                                ),
                            )
                        }
                        .padding(4.dp)
                        .size(18.dp),
                )
                Icon(
                    ExpIcons.uiAttach,
                    contentDescription = "Attach file",
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .clip(RoundedCornerShape(percent = 50))
                        .clickable { filePicker.launch(arrayOf("*/*")) }
                        .padding(4.dp)
                        .size(18.dp),
                )
                Icon(
                    ExpIcons.editorEmoji,
                    contentDescription = "Insert emoji",
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier
                        .clip(RoundedCornerShape(percent = 50))
                        .clickable { emojiPickerOpen = true }
                        .padding(4.dp)
                        .size(18.dp),
                )
                Spacer(Modifier.weight(1f))
                val trimmed = draft.trim()
                TextButton(
                    onClick = {
                        val unchanged = trimmed == bodyText &&
                            keptIds == attachments.map { it.id } &&
                            editAttachments.isEmpty()
                        if (unchanged) actions.onCancelEdit() else actions.onSaveEdit(trimmed, keptIds)
                    },
                    // Attachment-only comments are allowed — an entirely
                    // empty one is not.
                    enabled = trimmed.isNotEmpty() || keptIds.isNotEmpty() ||
                        editAttachments.isNotEmpty(),
                ) { Text("Save") }
                TextButton(onClick = actions.onCancelEdit) { Text("Cancel") }
            }
        } else {
            // A comment can be attachments only (EXP-554) — the body view
            // renders nothing at all then, instead of an empty block.
            if (bodyText.isNotBlank()) {
                MarkdownView(bodyText)
            }
            // EXP-723: a posted comment's images read at full width; the
            // 64dp thumb strip stays in the edit form above, where the
            // tiles are a queue rather than content.
            LargeCommentAttachments(
                attachments = attachments,
                onOpen = onOpenAttachment,
            )
        }
    }
}
