package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PagerDefaults
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager

/**
 * EXP-1152 / EXP-1190: the body under a phone's top segmented tab strip as a
 * native PAGER — the neighbour follows the finger and settles with the
 * platform fling. [beyondViewportPageCount] = 1 composes the neighbours before
 * a drag starts, so a swipe's first frame never stutters on a cold page (pass
 * more to keep every page composed). Nested horizontal scrollers and text
 * selection keep priority through nested scroll.
 *
 * Two-way sync with the screen's [selected]: a tab tap ANIMATES the pager
 * there; the reader's own drag/fling reports the settled-past-half-way page
 * back through [onSelect], so the strip follows the finger. When [pages]
 * itself changes (a tab appeared or vanished) the pager SNAPS to the selected
 * page's new index before anything reports back, so an index shift never
 * names the wrong page. A page change clears focus (the neighbours stay
 * composed, so an off-screen field would keep the keyboard). [key] must be
 * Bundle-saveable (a String). [content] is called per PAGE, filling it.
 *
 * #EXP-1387: a page for which [neighbour] returns a stub runs [content]
 * (and its [PageViewModelStore]) only while it is the SETTLED page; composed
 * as a neighbour (or passed by a tab tap's animation) it draws the stub, so a
 * drag never builds a whole screen and its view models on its first frame.
 * The settled page alone feeds the detail haze, flipping when a swipe
 * settles, never half-way through a drag.
 */
@Composable
fun <T> TabPager(
    pages: List<T>,
    selected: T,
    onSelect: (T) -> Unit,
    key: (T) -> Any,
    modifier: Modifier = Modifier,
    beyondViewportPageCount: Int = 1,
    /** EXP-1343: each page gets its own [PageViewModelStore] (its view
     *  models die with the page instead of piling up on the screen's entry). */
    pageViewModels: Boolean = false,
    /** #EXP-1387: a page's cheap stand-in while it is not the settled page;
     *  null (for the page, or no lambda) = that page always runs [content]. */
    neighbour: ((T) -> (@Composable () -> Unit)?)? = null,
    /** #EXP-1433: the pager came to REST on [selected] (no scroll in
     *  flight): a page the screen keeps only while it is shown can go now,
     *  never mid-drag. */
    onSettled: ((T) -> Unit)? = null,
    content: @Composable (T) -> Unit,
) {
    val pagerState = rememberPagerState(initialPage = pages.indexOf(selected).coerceAtLeast(0)) { pages.size }
    val latestSelected by rememberUpdatedState(selected)
    val latestPages by rememberUpdatedState(pages)
    val latestOnSelect by rememberUpdatedState(onSelect)
    // The pages the pager last laid out: a change means the indices moved.
    var laidOut by remember { mutableStateOf(pages) }
    // Scrolls WE drive (a tab tap's animation, a pages-change snap) in flight.
    // A counter, not a flag: a relaunched effect's cancelled `finally` may run
    // after its successor already started one.
    var driving by remember { mutableIntStateOf(0) }
    LaunchedEffect(selected, pages) {
        val target = pages.indexOf(selected)
        if (target < 0) return@LaunchedEffect
        val snap = pages != laidOut
        laidOut = pages
        if (pagerState.currentPage == target) return@LaunchedEffect
        driving++
        try {
            if (snap) pagerState.scrollToPage(target) else pagerState.animateScrollToPage(target)
        } finally {
            driving--
        }
    }
    LaunchedEffect(pagerState) {
        // Only the READER's drag or fling reports back — read in ONE snapshot
        // with the page, so a tab tap's animation passing through the pages
        // between (or a snap after a pages change) never re-picks them.
        snapshotFlow {
            if (pagerState.isScrollInProgress && driving == 0) pagerState.currentPage else null
        }.collect { page ->
            val picked = page?.let { latestPages.getOrNull(it) } ?: return@collect
            if (picked != latestSelected) latestOnSelect(picked)
        }
    }
    val latestOnSettled by rememberUpdatedState(onSettled)
    LaunchedEffect(pagerState) {
        // Read in ONE snapshot: the settled page counts only once it IS the
        // selected page (a pages-change snap still pending never reports
        // the old index's page).
        snapshotFlow {
            if (pagerState.isScrollInProgress) {
                null
            } else {
                latestPages.getOrNull(pagerState.settledPage)?.takeIf { it == latestSelected }
            }
        }.collect { page -> if (page != null) latestOnSettled?.invoke(page) }
    }
    // Never on first composition — a page's own initial focus is its to take.
    val focusManager = LocalFocusManager.current
    var focusPage by remember { mutableStateOf(selected) }
    LaunchedEffect(selected) {
        if (selected == focusPage) return@LaunchedEffect
        focusPage = selected
        focusManager.clearFocus()
    }
    val outerHazeActive = LocalDetailHazeSourceActive.current
    HorizontalPager(
        state = pagerState,
        modifier = modifier,
        beyondViewportPageCount = beyondViewportPageCount,
        userScrollEnabled = pages.size > 1,
        key = { index -> pages.getOrNull(index)?.let(key) ?: index },
        flingBehavior = PagerDefaults.flingBehavior(pagerState),
    ) { index ->
        val page = pages.getOrNull(index) ?: return@HorizontalPager
        // #EXP-1387: read through derivedStateOf, so a page recomposes only
        // when ITS settled-ness flips, never on every page change mid-drag.
        val settled by remember(index) { derivedStateOf { pagerState.settledPage == index } }
        // EXP-1343: only the settled page feeds the ONE detail haze; a
        // neighbour composed for a drag never registers a second source.
        CompositionLocalProvider(LocalDetailHazeSourceActive provides (outerHazeActive && settled)) {
            Box(modifier = Modifier.fillMaxSize()) {
                val stub = if (settled) null else neighbour?.invoke(page)
                when {
                    stub != null -> stub()
                    pageViewModels -> PageViewModelStore { content(page) }
                    else -> content(page)
                }
            }
        }
    }
}
