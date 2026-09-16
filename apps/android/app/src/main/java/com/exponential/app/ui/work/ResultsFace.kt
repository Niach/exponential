package com.exponential.app.ui.work

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.SessionResultGroup
import com.exponential.app.domain.sessionResultTileHeightFitting
import com.exponential.app.domain.sessionResultTileWidth
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.FloatingBottomBar
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-879: the Work screen's RESULTS FACE — the screenshots the shown run
// published with `exponential_sessions_results`, read off the synced
// `coding_sessions.results` blob. ONE scrolling page: a filled group band per
// topic (the EXP-818 section band every list wears) over a WRAPPING row of
// equal-height tiles, each captioned with its label; a tap opens the shot
// full screen.
//
// Results is a SUB-FACE of Run, like Changes: it owns neither the Stop/Resume
// verb (the top bar's, on Run only) nor the merge capsule (the Changes bar's)
// — its floating bar carries the face switcher and nothing else.

/** The horizontal content padding the page reserves on each side. */
private val HorizontalPadding = 16.dp
private val TileShape = RoundedCornerShape(10.dp)

@Composable
fun ResultsFace(
    padding: PaddingValues,
    groups: List<SessionResultGroup>,
    trailingBarSlot: @Composable () -> Unit,
) {
    var preview by remember { mutableStateOf<SessionResultEntry?>(null) }

    BoxWithConstraints(modifier = Modifier.padding(padding).fillMaxSize()) {
        // ONE factor for the whole page: the base height unless the widest
        // tile would overflow the column, then every tile scales by the same
        // amount so the equal-height strip survives (shared rule ×4). An
        // unmeasured page (zero width) renders at the base.
        val availableDp = (maxWidth - HorizontalPadding * 2).value.toInt()
        val tileHeight = remember(groups, availableDp) {
            sessionResultTileHeightFitting(groups.flatMap { it.entries }, availableDp)
        }
        LazyColumn(
            modifier = Modifier.fillMaxSize().testTag("work-results"),
            contentPadding = PaddingValues(
                start = HorizontalPadding,
                end = HorizontalPadding,
                top = 4.dp,
                bottom = BottomBarInset,
            ),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            groups.forEach { group ->
                item(key = "topic_${group.topic}") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SectionHeader(title = group.topic)
                        // The shots wrap rather than scroll sideways: a topic
                        // with an iOS, an Android and a web shot reads as one
                        // block, not three hidden behind a swipe.
                        FlowRow(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                            verticalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            group.entries.forEach { entry ->
                                ResultTile(
                                    entry = entry,
                                    tileHeight = tileHeight,
                                    onOpen = { preview = entry },
                                )
                            }
                        }
                    }
                }
            }
        }
        Column(
            modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Results owns no bar control of its own — no Stop/Resume (the
            // top bar's, Run only), no merge capsule (the Changes bar's).
            FloatingBottomBar(right = trailingBarSlot) { Spacer(Modifier.weight(1f)) }
        }
    }

    preview?.let { entry ->
        ResultPreviewDialog(entry = entry, onDismiss = { preview = null })
    }
}

/**
 * One shot: the image at the shared height and its probed width, the label as
 * a one-line caption under it. The URL is DERIVED from the attachment id — the
 * Coil `InstanceUrlInterceptor` absolutizes the relative path against the
 * owning account and attaches its bearer token, exactly as comment
 * attachments load.
 */
@Composable
private fun ResultTile(entry: SessionResultEntry, tileHeight: Int, onOpen: () -> Unit) {
    Column(
        modifier = Modifier
            .width(sessionResultTileWidth(entry, tileHeight).dp)
            .testTag("work-result-${entry.attachmentId}"),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        AsyncImage(
            model = sessionResultImageUrl(entry),
            contentDescription = entry.label,
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .fillMaxWidth()
                .height(tileHeight.dp)
                .clip(TileShape)
                .background(GlassTokens.RowFill)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, TileShape)
                .clickable(onClick = onOpen),
        )
        Text(
            entry.label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The shot at full size, in app: dark scrim, FIT so nothing is cropped away,
 *  a tap anywhere (or Back) closes it. */
@Composable
private fun ResultPreviewDialog(entry: SessionResultEntry, onDismiss: () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.92f))
                .clickable(onClick = onDismiss)
                .testTag("work-result-preview"),
            contentAlignment = Alignment.Center,
        ) {
            AsyncImage(
                model = sessionResultImageUrl(entry),
                contentDescription = entry.label,
                contentScale = ContentScale.Fit,
                modifier = Modifier.fillMaxSize().padding(12.dp),
            )
            IconButton(
                onClick = onDismiss,
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .statusBarsPadding()
                    .padding(8.dp),
            ) {
                Icon(
                    ExpIcons.uiClose,
                    contentDescription = "Close",
                    tint = Color.White.copy(alpha = 0.9f),
                )
            }
        }
    }
}

/** The stored relative attachment URL — never absolutized here (EXP-824's
 *  rule): the image loader resolves it against the active instance. */
private fun sessionResultImageUrl(entry: SessionResultEntry): String =
    "/api/attachments/${entry.attachmentId}"
