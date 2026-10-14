package com.exponential.app.ui.work

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import coil3.compose.AsyncImage
import com.exponential.app.domain.GuideViewer
import com.exponential.app.domain.SessionResultEntry
import com.exponential.app.domain.SessionResultGroup
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** The Guide as the viewer's input (`guideViewer`'s entry shape): each
 *  topic's text, then its pictures (the folded inline ones first: publish
 *  order). */
internal fun guideViewerEntries(groups: List<SessionResultGroup>): List<GuideViewer.Entry> =
    groups.flatMap { group ->
        listOfNotNull(group.text?.let { GuideViewer.Entry(topic = group.topic, text = it) }) +
            (group.earlier + group.entries).map {
                GuideViewer.Entry(topic = group.topic, label = it.label, attachmentId = it.attachmentId)
            }
    }

/** Every Guide picture in the viewer's order, walked off [GuideViewer.state]. */
private fun viewerOrder(entries: List<GuideViewer.Entry>, startId: String): List<String> {
    val start = GuideViewer.state(entries, startId) ?: return emptyList()
    val before = generateSequence(start.prevId) { GuideViewer.state(entries, it)?.prevId }.toList().asReversed()
    val after = generateSequence(start.nextId) { GuideViewer.state(entries, it)?.nextId }.toList()
    return before + startId + after
}

/**
 * EXP-1283 (1i): THE Guide picture viewer, phone form
 * (`session-results.json` `viewer`): a full-screen page — the picture on top
 * fit to the width, the panel below it (the topic, the picture's label pill,
 * the topic's GFM text, `index / count`), a swipe between EVERY Guide
 * picture in the Guide's order (no wrap), × top-right. The picture is the
 * pager; the panel scrolls with it.
 */
@Composable
internal fun GuidePictureViewer(
    groups: List<SessionResultGroup>,
    start: SessionResultEntry,
    onDismiss: () -> Unit,
) {
    val entries = remember(groups) { guideViewerEntries(groups) }
    val order = remember(entries, start.attachmentId) { viewerOrder(entries, start.attachmentId) }
    val pictures = remember(groups) {
        groups.flatMap { it.earlier + it.entries }.associateBy { it.attachmentId }
    }
    val pagerState = rememberPagerState(initialPage = order.indexOf(start.attachmentId).coerceAtLeast(0)) {
        order.size
    }
    val maxPicture = LocalConfiguration.current.screenHeightDp.dp * 0.55f
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(GlassTokens.BackgroundTop)
                .testTag("guide-picture-viewer"),
        ) {
            HorizontalPager(state = pagerState, modifier = Modifier.fillMaxSize()) { page ->
                val id = order[page]
                val entry = pictures[id] ?: start
                val state = GuideViewer.state(entries, id)
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .statusBarsPadding()
                        .navigationBarsPadding()
                        .padding(top = 64.dp, bottom = 24.dp),
                ) {
                    val aspect = entry.width?.let { w -> entry.height?.takeIf { it > 0 && w > 0 }?.let { w.toFloat() / it } }
                    AsyncImage(
                        model = sessionResultImageUrl(entry),
                        contentDescription = entry.label,
                        contentScale = ContentScale.Fit,
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = maxPicture)
                            .then(if (aspect != null) Modifier.aspectRatio(aspect, matchHeightConstraintsFirst = true) else Modifier)
                            .background(Color.Black)
                            .testTag("guide-picture-viewer-image"),
                    )
                    if (state != null) {
                        Column(
                            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 20.dp),
                            verticalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            Text(
                                state.topic,
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.SemiBold,
                                color = MaterialTheme.colorScheme.onSurface,
                                modifier = Modifier.testTag("guide-picture-viewer-topic"),
                            )
                            LabelPill(state.label)
                            state.text?.let { MarkdownView(markdown = it, softBreaksAsNewlines = true) }
                        }
                    }
                }
            }
            CircleIconButton(
                ExpIcons.uiClose,
                "Close",
                onClick = onDismiss,
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .statusBarsPadding()
                    .padding(12.dp)
                    .testTag("guide-picture-viewer-close"),
            )
            if (order.size > 1) {
                PageIndicator(
                    index = pagerState.currentPage,
                    count = order.size,
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .navigationBarsPadding()
                        .padding(bottom = 16.dp),
                )
            }
        }
    }
}

@Composable
private fun LabelPill(label: String) {
    val shape = RoundedCornerShape(10.dp)
    Text(
        label,
        style = MaterialTheme.typography.labelLarge,
        fontFamily = FontFamily.Monospace,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier
            .clip(shape)
            .background(GlassTokens.OpaqueCardFill, shape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .testTag("guide-picture-viewer-label"),
    )
}

/** `index / count` over one dot per picture (the shown one lit). */
@Composable
private fun PageIndicator(index: Int, count: Int, modifier: Modifier = Modifier) {
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            "${index + 1} / $count",
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.testTag("guide-picture-viewer-index"),
        )
        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            repeat(count) { dot ->
                Box(
                    Modifier
                        .size(8.dp)
                        .clip(CircleShape)
                        .background(
                            MaterialTheme.colorScheme.onSurface.copy(
                                alpha = if (dot == index) 1f else TextEmphasis.Quaternary,
                            ),
                        ),
                )
            }
        }
    }
}
