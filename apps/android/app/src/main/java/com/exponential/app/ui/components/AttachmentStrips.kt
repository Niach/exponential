package com.exponential.app.ui.components

import android.graphics.BitmapFactory
import android.text.format.Formatter
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.data.db.AttachmentEntity
import com.exponential.app.domain.PendingAttachment
import com.exponential.app.domain.isInlineImage
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** The square side every attachment thumbnail uses, on every composer. */
private val TileSize = 64.dp
private val TileShape = RoundedCornerShape(8.dp)

/** Reading-size comment images (EXP-723): full width, capped, softly rounded. */
private val LargeImageMaxHeight = 480.dp
private val LargeImageShape = RoundedCornerShape(DesignTokens.Radius.Lg)

/**
 * Thumbnails of the attachments queued for the next send — the steer
 * composer's strip (EXP-511) generalized to files, shared by the comment
 * composer and the comment editor (EXP-554).
 */
@Composable
fun PendingAttachmentStrip(
    items: List<PendingAttachment>,
    enabled: Boolean,
    onRemove: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (items.isEmpty()) return
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(bottom = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        items.forEachIndexed { index, item ->
            Box {
                if (item.isImage) {
                    val bitmap = remember(item.uri, item.bytes) {
                        BitmapFactory.decodeByteArray(item.bytes, 0, item.bytes.size)
                            ?.asImageBitmap()
                    }
                    Box(modifier = Modifier.size(TileSize)) {
                        if (bitmap != null) {
                            Image(
                                bitmap = bitmap,
                                contentDescription = item.filename,
                                contentScale = ContentScale.Crop,
                                modifier = Modifier.fillMaxSize().clip(TileShape),
                            )
                        } else {
                            PlaceholderTile()
                        }
                    }
                } else {
                    FileTile(filename = item.filename, subtitle = null)
                }
                RemoveBadge(
                    contentDescription = "Remove ${item.filename}",
                    enabled = enabled,
                    onClick = { onRemove(index) },
                    modifier = Modifier.align(Alignment.TopEnd),
                )
            }
        }
    }
}

/**
 * The attachments linked to a posted comment (EXP-554): squared image thumbs
 * and file chips, never inlined into the markdown body. Images load straight
 * from the stored relative `/api/attachments/{id}` URL — the Coil
 * InstanceUrlInterceptor absolutizes it against the owning account and
 * attaches its bearer token.
 *
 * [onRemove] is non-null only in edit mode, where removing a tile just drops
 * it from the set the save will send (the server hard-deletes what is absent).
 */
@Composable
fun CommentAttachmentsStrip(
    attachments: List<AttachmentEntity>,
    onOpen: (AttachmentEntity) -> Unit,
    modifier: Modifier = Modifier,
    onRemove: ((AttachmentEntity) -> Unit)? = null,
) {
    if (attachments.isEmpty()) return
    val context = LocalContext.current
    Row(
        modifier = modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(top = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        attachments.forEach { attachment ->
            Box {
                if (isInlineImage(attachment.contentType)) {
                    AsyncImage(
                        model = attachment.url,
                        contentDescription = attachment.filename,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier
                            .size(TileSize)
                            .clip(TileShape)
                            .clickable { onOpen(attachment) },
                    )
                } else {
                    FileTile(
                        filename = attachment.filename,
                        subtitle = Formatter.formatShortFileSize(context, attachment.sizeBytes),
                        modifier = Modifier.clickable { onOpen(attachment) },
                    )
                }
                if (onRemove != null) {
                    RemoveBadge(
                        contentDescription = "Remove ${attachment.filename}",
                        enabled = true,
                        onClick = { onRemove(attachment) },
                        modifier = Modifier.align(Alignment.TopEnd),
                    )
                }
            }
        }
    }
}

/**
 * A posted comment's attachments at READING size (EXP-723): images stacked
 * full-width instead of squared into 64dp thumbs — an image someone attached
 * to a comment is the point of the comment, and the thumb strip made it
 * something to go and open. Non-images stay the same file chips; the 64dp
 * strip survives in the composer and the edit form, where the tiles are a
 * queue rather than content.
 *
 * The probed `width`/`height` pre-size each image so the thread doesn't jump
 * when the bitmap lands; rows without them just cap at 480dp.
 */
@Composable
fun LargeCommentAttachments(
    attachments: List<AttachmentEntity>,
    onOpen: (AttachmentEntity) -> Unit,
    modifier: Modifier = Modifier,
    onRemove: ((AttachmentEntity) -> Unit)? = null,
) {
    if (attachments.isEmpty()) return
    val context = LocalContext.current
    Column(
        modifier = modifier.fillMaxWidth().padding(top = 8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        attachments.forEach { attachment ->
            Box {
                if (isInlineImage(attachment.contentType)) {
                    val width = attachment.width
                    val height = attachment.height
                    AsyncImage(
                        model = attachment.url,
                        contentDescription = attachment.filename,
                        contentScale = ContentScale.Fit,
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = LargeImageMaxHeight)
                            .then(
                                if (width != null && height != null && width > 0 && height > 0) {
                                    Modifier.aspectRatio(width.toFloat() / height.toFloat())
                                } else {
                                    Modifier
                                },
                            )
                            .clip(LargeImageShape)
                            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, LargeImageShape)
                            .clickable { onOpen(attachment) },
                    )
                } else {
                    FileTile(
                        filename = attachment.filename,
                        subtitle = Formatter.formatShortFileSize(context, attachment.sizeBytes),
                        modifier = Modifier.clickable { onOpen(attachment) },
                    )
                }
                if (onRemove != null) {
                    RemoveBadge(
                        contentDescription = "Remove ${attachment.filename}",
                        enabled = true,
                        onClick = { onRemove(attachment) },
                        modifier = Modifier.align(Alignment.TopEnd),
                    )
                }
            }
        }
    }
}

/** A non-image tile: type icon over a truncated name (and size, when known). */
@Composable
private fun FileTile(
    filename: String,
    subtitle: String?,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .widthIn(min = TileSize, max = 140.dp)
            .clip(TileShape)
            .background(GlassTokens.RowFill)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(
            ExpIcons.uiFile,
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = Color.White.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.size(4.dp))
        Text(
            filename,
            style = MaterialTheme.typography.labelSmall,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (subtitle != null) {
            Text(
                subtitle,
                style = MaterialTheme.typography.labelSmall,
                color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun PlaceholderTile() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .clip(TileShape)
            .background(GlassTokens.RowFill),
    )
}

/** The black circle-X badge every removable tile carries (steer parity). */
@Composable
private fun RemoveBadge(
    contentDescription: String,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier
            .padding(2.dp)
            .size(18.dp)
            .clip(CircleShape)
            .background(Color.Black.copy(alpha = 0.55f))
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            ExpIcons.uiClose,
            contentDescription = contentDescription,
            modifier = Modifier.size(12.dp),
            tint = Color.White,
        )
    }
}
