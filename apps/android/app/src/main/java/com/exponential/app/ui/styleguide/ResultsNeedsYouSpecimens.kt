package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.NeedsYou
import com.exponential.app.domain.ResultsTab
import com.exponential.app.ui.chats.NeedsYouEntryRow
import com.exponential.app.ui.chats.NeedsYouRowItem
import com.exponential.app.ui.components.LocalSpecimenPicture
import com.exponential.app.ui.components.SpecimenSamples
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.thread.ResultsTabContent
import com.exponential.app.ui.thread.resultPictureSet
import com.exponential.app.ui.work.PictureViewer
import com.exponential.app.ui.work.PictureViewerSet

/*
 * #FEED-191 styleguide `needs-you` (needs-you.json `cases[1]`, hardcoded ×4) and
 * `results-tab` (results-tab.json `tab.specimen` = `cases[1]`, hardcoded ×4;
 * ResultsNeedsYouSpecimenTest compares both to the fixture). Capture only
 * (StyleguideScreenshotsTest `captureOnlyScreens`).
 */

const val NeedsYouSpecimenTestId = "needs-you-specimen"
const val ResultsTabSpecimenTestId = "results-tab-specimen"

internal val NEEDS_YOU_SPECIMEN_ROWS = listOf(
    NeedsYou.Row(
        messageId = "p1", sessionId = "r1", rootId = "r1", kind = NeedsYou.Kind.Plan, opens = NeedsYou.Opens.Plan,
        title = "Plan: Group by assignee", chat = "Assignee groups", meta = "", age = "24m",
        createdAt = "2027-01-15T07:36:00.000Z",
    ),
    NeedsYou.Row(
        messageId = "q1", sessionId = "r2", rootId = "r2", kind = NeedsYou.Kind.Question, opens = NeedsYou.Opens.Card,
        title = "Which palette?", chat = "Palette", meta = "", age = "1h",
        createdAt = "2027-01-15T07:00:00.000Z",
    ),
)

/** `needs-you`: the phone entry row, then THE list (shot `sg_needs-you`). */
@Composable
fun NeedsYouSpecimenScreen() {
    AppBackground(Modifier.testTag(NeedsYouSpecimenTestId)) {
        Column(Modifier.fillMaxSize().statusBarsPadding().padding(vertical = 16.dp)) {
            NeedsYouEntryRow(count = NEEDS_YOU_SPECIMEN_ROWS.size, onClick = {})
            Column(Modifier.padding(start = 8.dp, end = 8.dp, top = 16.dp)) {
                NEEDS_YOU_SPECIMEN_ROWS.forEach { row -> NeedsYouRowItem(row, onClick = {}) }
            }
        }
    }
}

internal val RESULTS_SPECIMEN = listOf(
    ResultsTab.Result(
        id = "res-web", sessionId = "c1", sessionName = "Web draft", createdAt = 1_799_992_800_000L, age = "2h",
        title = "Label picker", description = null,
        pictures = listOf(
            ResultsTab.Picture("web picker", "att-w", 1600.0, 1000.0, "Mixed state"),
            ResultsTab.Picture("ios picker", "att-i", 1179.0, 2556.0, null),
        ),
        surface = true,
    ),
    ResultsTab.Result(
        id = "old-result", sessionId = "r1", sessionName = "Batch-edit labels", createdAt = 1_799_222_400_000L, age = "9d",
        title = "First draft", description = null, pictures = emptyList(), surface = true,
    ),
)

/** `results-tab`: the tab body, no PR block, THE committed sample pictures (shot
 *  `sg_results-tab`); a thumbnail opens THE lightbox. */
@Composable
fun ResultsTabSpecimenScreen() {
    val samples = SpecimenSamples.resolver(LocalContext.current)
    var opened by remember { mutableStateOf<PictureViewerSet?>(null) }
    AppBackground(Modifier.testTag(ResultsTabSpecimenTestId)) {
        CompositionLocalProvider(LocalSpecimenPicture provides samples) {
            Box(Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(16.dp)) {
                ResultsTabContent(
                    results = RESULTS_SPECIMEN,
                    onOpenPicture = { result, index -> opened = resultPictureSet(result, index) },
                )
            }
            opened?.let { set -> PictureViewer(items = set.items, start = set.start, onClose = { opened = null }) }
        }
    }
}
