package com.exponential.app.ui.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeLeft
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EXP-1387: the thread pager's page rules. Only the SETTLED page runs its
 * content and holds its view models; a neighbour draws its stub. A page left
 * clears its view models, and a page view model's SavedStateHandle is its
 * page's own, never the screen's (`session:<root>` lives on both).
 */
@RunWith(AndroidJUnit4::class)
class TabPagerTest {

    @get:Rule
    val composeRule = createComposeRule()

    class Probe(val handle: SavedStateHandle) : ViewModel() {
        init {
            live.incrementAndGet()
        }

        override fun onCleared() {
            live.decrementAndGet()
        }

        companion object {
            val live = AtomicInteger()
        }
    }

    @Before
    fun reset() {
        Probe.live.set(0)
    }

    @Test
    fun onlyTheSettledPageHoldsItsViewModels() {
        val pages = listOf("a", "b", "c")
        composeRule.setContent {
            var selected by remember { mutableStateOf("a") }
            TabPager(
                pages = pages,
                selected = selected,
                onSelect = { selected = it },
                key = { it },
                modifier = Modifier.fillMaxSize().testTag("pager"),
                beyondViewportPageCount = 1,
                pageViewModels = true,
                neighbour = { page -> { Text("stub $page", Modifier.testTag("stub-$page")) } },
            ) { page ->
                viewModel<Probe>(key = "probe:$page")
                Text("page $page", Modifier.testTag("page-$page"))
            }
        }
        composeRule.onNodeWithTag("page-a").assertExists()
        composeRule.onNodeWithTag("stub-b").assertExists()
        composeRule.onNodeWithTag("page-b").assertDoesNotExist()
        assertEquals(1, Probe.live.get())

        composeRule.onNodeWithTag("pager").performTouchInput { swipeLeft() }
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("page-b").assertExists()
        composeRule.onNodeWithTag("stub-a").assertExists()
        composeRule.onNodeWithTag("stub-c").assertExists()
        composeRule.onNodeWithTag("page-a").assertDoesNotExist()
        // a's view model died with its page; b's is the one alive.
        assertEquals(1, Probe.live.get())
    }

    @Test
    fun aPageViewModelOwnsItsSavedStateHandle() {
        lateinit var screen: Probe
        lateinit var page: Probe
        composeRule.setContent {
            screen = viewModel(key = "session:root")
            TabPager(
                pages = listOf("root"),
                selected = "root",
                onSelect = {},
                key = { it },
                pageViewModels = true,
            ) {
                page = viewModel(key = "session:root")
            }
        }
        composeRule.waitForIdle()
        assertNotSame(screen, page)
        assertNotSame(screen.handle, page.handle)
    }
}
