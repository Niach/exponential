package com.exponential.app.ui.issue

import android.net.Uri
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
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.getCommentBodyText
import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownEditor
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.markdown.MentionMember
import com.exponential.app.ui.markdown.hasDraftImages
import com.exponential.app.ui.theme.glassCard

// One human comment in the thread: a rounded glass card (author + relative
// time + markdown body) with the avatar sitting in the timeline gutter
// (EXP-240), rail segments above/below keeping the line continuous like the
// event rows. The edit/delete overflow is AUTHOR-ONLY (EXP-398) — matching the
// server, which no longer lets a global admin touch someone else's comment.
@Composable
internal fun RegularCommentRow(
    comment: CommentEntity,
    lineAbove: Boolean,
    lineBelow: Boolean,
    author: UserEntity?,
    isAuthor: Boolean,
    isEditing: Boolean,
    onEdit: () -> Unit,
    onCancelEdit: () -> Unit,
    onSaveEdit: (String) -> Unit,
    onDelete: () -> Unit,
    onUploadImage: suspend (Uri) -> String?,
    mentionMembers: List<MentionMember>,
    mentionEnabled: Boolean = true,
) {
    val bodyText = remember(comment.body) { getCommentBodyText(comment.body) }
    var draft by remember(comment.id, isEditing) { mutableStateOf(bodyText) }
    var menuOpen by remember { mutableStateOf(false) }

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
            Box(
                modifier = Modifier
                    .size(26.dp)
                    .clip(RoundedCornerShape(percent = 50))
                    .background(CommentAvatarBg),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    initials(userDisplayName(author, comment.authorId)),
                    style = MaterialTheme.typography.labelSmall,
                    color = CommentAvatarText,
                )
            }
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
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    userDisplayName(author, comment.authorId),
                    style = MaterialTheme.typography.labelMedium,
                    color = CommentAuthor,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    relativeTime(comment.createdAt) +
                        if (comment.editedAt != null) " · edited" else "",
                    style = MaterialTheme.typography.labelSmall,
                    color = CommentMeta,
                )
                if (isAuthor && !isEditing) {
                    Spacer(Modifier.weight(1f))
                    Box {
                        // A plain clickable icon, not an IconButton: M3's 48dp
                        // minimum touch target made the header row three times
                        // the height of its text and read as stray padding at
                        // the top of the card (EXP-398).
                        Icon(
                            ExpIcons.uiMoreVertical,
                            contentDescription = "Comment actions",
                            tint = CommentMeta,
                            modifier = Modifier
                                .clip(RoundedCornerShape(percent = 50))
                                .clickable { menuOpen = true }
                                .padding(4.dp)
                                .size(16.dp),
                        )
                        GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                            GlassMenuItem(
                                text = { Text("Edit") },
                                onClick = { menuOpen = false; onEdit() },
                            )
                            GlassMenuItem(
                                text = { Text("Delete") },
                                onClick = { menuOpen = false; onDelete() },
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
                    onUploadImage = onUploadImage,
                    placeholder = "Edit comment…",
                    minHeight = 40.dp,
                    mentionMembers = mentionMembers,
                    mentionEnabled = mentionEnabled,
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    TextButton(
                        onClick = {
                            val trimmed = draft.trim()
                            if (trimmed.isEmpty() || trimmed == bodyText) onCancelEdit()
                            else onSaveEdit(trimmed)
                        },
                        // Same gate as the composer's Send: saving while an image
                        // is still a draft:// placeholder would silently strip it.
                        enabled = !hasDraftImages(draft),
                    ) { Text("Save") }
                    TextButton(onClick = onCancelEdit) { Text("Cancel") }
                }
            } else {
                MarkdownView(bodyText)
            }
        }
    }
}

private fun initials(name: String): String =
    name.split(" ", limit = 2).mapNotNull { it.firstOrNull()?.toString() }.joinToString("").uppercase()
