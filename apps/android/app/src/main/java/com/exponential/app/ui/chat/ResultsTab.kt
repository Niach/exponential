package com.exponential.app.ui.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.domain.ResultsTab
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.LocalSpecimenPicture
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.work.PictureViewerItem
import com.exponential.app.ui.work.PictureViewerSet
import kotlin.math.roundToInt

// #FEED-191 RESULTS TAB ×Android (fixtures/results-tab.json; web
// `packages/ui/src/results-tab.tsx`, iOS ExpUI `ResultsTab.swift`): the session
// screen's last tab, phone form. The model is [ResultsTab.resultsTab]; this only
// paints it. The host hands in the PR block ([lead]) and a surface's renderer.

private val TILE_SHAPE = RoundedCornerShape(ResultsTab.Tab.Tile.RADIUS.dp)

/** A thumbnail's width (dp): by the picture's aspect at `tile.thumbHeight`,
 *  capped at `tile.thumbMaxWidth`; an unknown size = 4:3. */
internal fun resultThumbWidth(picture: ResultsTab.Picture): Int {
    val width = picture.width
    val height = picture.height
    val aspect = if (width != null && height != null && width > 0 && height > 0) width / height else 4.0 / 3.0
    return minOf(ResultsTab.Tab.Tile.THUMB_MAX_WIDTH.toInt(), (ResultsTab.Tab.Tile.THUMB_HEIGHT * aspect).roundToInt())
}

/** THE lightbox's set: this result's pictures, opening at [start]. */
internal fun resultPictureSet(result: ResultsTab.Result, start: Int): PictureViewerSet = PictureViewerSet(
    items = result.pictures.map { picture ->
        PictureViewerItem(
            id = picture.attachmentId,
            src = "/api/attachments/${picture.attachmentId}",
            label = picture.caption ?: picture.label,
        )
    },
    start = start,
)

/** THE Results tab body: [lead] (the PR block), then the results INLINE
 *  `tile.listGap` apart, else THE EmptyState under the lead. Not scrolling
 *  itself: the host's page scrolls it. */
@Composable
fun ResultsTabContent(
    results: List<ResultsTab.Result>,
    onOpenPicture: (ResultsTab.Result, Int) -> Unit,
    modifier: Modifier = Modifier,
    lead: (@Composable () -> Unit)? = null,
    surface: @Composable (ResultsTab.Result) -> Unit = {},
) {
    Column(
        modifier.fillMaxWidth().testTag(ResultsTab.Tab.TestIds.LIST),
        verticalArrangement = Arrangement.spacedBy(ResultsTab.Tab.Tile.LEAD_GAP.dp),
    ) {
        lead?.invoke()
        if (results.isEmpty()) {
            EmptyState(
                title = ResultsTab.Tab.Copy.EMPTY,
                icon = ExpIcons.chatResults,
                modifier = Modifier.padding(top = 48.dp).heightIn(min = 240.dp),
            )
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(ResultsTab.Tab.Tile.LIST_GAP.dp)) {
                results.forEach { result ->
                    ResultSection(result, onOpenPicture = { onOpenPicture(result, it) }, surface = { surface(result) })
                }
            }
        }
    }
}

/** [ResultsTabContent] as LAZY items (the Results page: a large chat's
 *  hundred results compose as they scroll in): the same order, the same
 *  `tile.leadGap` under [lead] and `tile.listGap` between sections, the same
 *  empty state; the list's test id rides the first item. */
fun LazyListScope.resultsTabItems(
    results: List<ResultsTab.Result>,
    onOpenPicture: (ResultsTab.Result, Int) -> Unit,
    lead: (@Composable () -> Unit)? = null,
    surface: @Composable (ResultsTab.Result) -> Unit = {},
) {
    if (lead != null) {
        item(key = "results-lead", contentType = "lead") {
            Box(Modifier.fillMaxWidth().testTag(ResultsTab.Tab.TestIds.LIST)) { lead() }
        }
    }
    val leadGap = if (lead != null) ResultsTab.Tab.Tile.LEAD_GAP.dp else 0.dp
    if (results.isEmpty()) {
        item(key = "results-empty", contentType = "empty") {
            Box(Modifier.fillMaxWidth().padding(top = leadGap).then(if (lead == null) Modifier.testTag(ResultsTab.Tab.TestIds.LIST) else Modifier)) {
                EmptyState(
                    title = ResultsTab.Tab.Copy.EMPTY,
                    icon = ExpIcons.chatResults,
                    modifier = Modifier.padding(top = 48.dp).heightIn(min = 240.dp),
                )
            }
        }
        return
    }
    // Stable keys while the ids are distinct (a lazy list refuses a repeat).
    val distinct = results.mapTo(HashSet()) { it.id }.size == results.size
    itemsIndexed(results, key = { index, result -> if (distinct) result.id else "$index:${result.id}" }, contentType = { _, _ -> "result" }) { index, result ->
        val top = if (index == 0) leadGap else ResultsTab.Tab.Tile.LIST_GAP.dp
        val tag = if (index == 0 && lead == null) Modifier.testTag(ResultsTab.Tab.TestIds.LIST) else Modifier
        Box(Modifier.fillMaxWidth().padding(top = top).then(tag)) {
            ResultSection(result, onOpenPicture = { onOpenPicture(result, it) }, surface = { surface(result) })
        }
    }
}

/** One SECTION: line 1 the title with the age muted at the trailing end, no
 *  badge; the description (muted, wrapping) `tile.textGap` under it; then
 *  `tile.gap` under that: pictures win (ONE sideways row of thumbnails, a muted
 *  caption-or-label under each), else the surface. A press on a picture opens
 *  the lightbox; nothing else presses. */
@Composable
private fun ResultSection(
    result: ResultsTab.Result,
    onOpenPicture: (Int) -> Unit,
    surface: @Composable () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Column(
        Modifier.fillMaxWidth().testTag(ResultsTab.Tab.TestIds.SECTION),
        verticalArrangement = Arrangement.spacedBy(ResultsTab.Tab.Tile.GAP.dp),
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(ResultsTab.Tab.Tile.TEXT_GAP.dp)) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    result.title,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(result.age, style = MaterialTheme.typography.bodySmall, color = muted, maxLines = 1)
            }
            result.description?.let { description ->
                Text(description, style = MaterialTheme.typography.bodyMedium, color = muted)
            }
        }
        if (result.pictures.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(ResultsTab.Tab.Tile.GAP.dp),
            ) {
                result.pictures.forEachIndexed { index, picture ->
                    ResultThumbnail(picture, onClick = { onOpenPicture(index) })
                }
            }
        } else if (result.surface) {
            surface()
        }
    }
}

/** One thumbnail: `tile.thumbHeight` tall, width by aspect, `tile.radius`, the
 *  glass-stroke hairline on the muted fill, cropped from the top; the
 *  caption-else-label line 6 under it. */
@Composable
private fun ResultThumbnail(picture: ResultsTab.Picture, onClick: () -> Unit) {
    val width = resultThumbWidth(picture).dp
    val height = ResultsTab.Tab.Tile.THUMB_HEIGHT.dp
    val text = picture.caption ?: picture.label
    Column(
        Modifier.width(width).clickable(onClick = onClick).testTag("result-picture"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            Modifier
                .width(width)
                .height(height)
                .clip(TILE_SHAPE)
                .background(DesignTokens.Palette.Muted, TILE_SHAPE)
                .border(GlassTokens.Hairline, GlassTokens.StrokeCard, TILE_SHAPE),
        ) {
            val src = "/api/attachments/${picture.attachmentId}"
            AsyncImage(
                model = LocalSpecimenPicture.current?.invoke(src) ?: src,
                contentDescription = text,
                contentScale = ContentScale.Crop,
                alignment = Alignment.TopCenter,
                modifier = Modifier.fillMaxWidth().height(height),
            )
        }
        Text(
            text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}
