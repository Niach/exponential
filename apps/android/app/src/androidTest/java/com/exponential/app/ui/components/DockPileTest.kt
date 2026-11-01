package com.exponential.app.ui.components

import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.height
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.exponential.app.domain.Dock
import com.exponential.app.ui.theme.ExponentialTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EXP-1585: a LONE Dock card has its own height. The pile starts 0 tall (no
 * card measured yet); a card measured inside it read 0 and the pile never
 * grew, so a session whose only card was THE Queue drew no Dock at all.
 */
@RunWith(AndroidJUnit4::class)
class DockPileTest {

    @get:Rule
    val composeRule = createComposeRule()

    private fun row(id: String) = Dock.Row(
        id = id,
        kind = Dock.Kind.Tasks,
        tint = Dock.Tint.Muted,
        icon = Dock.Icons.TASKS,
        label = "3 to do",
        caption = "2 completed",
        meta = null,
        title = "Measuring on the Galaxy A54",
        expandable = true,
        action = null,
    )

    private fun assertCardsDrawn(rows: List<Dock.Row>) {
        composeRule.setContent {
            ExponentialTheme {
                DockCards(rows = rows, openId = null, onOpenChange = {}) { _, _, _ -> }
            }
        }
        composeRule.waitForIdle()
        val pile = composeRule.onAllNodesWithTag(Dock.TestIds.PILE).onFirst().getUnclippedBoundsInRoot()
        val card = composeRule.onAllNodesWithTag(Dock.TestIds.CARD).onFirst().getUnclippedBoundsInRoot()
        assertTrue("card height ${card.height}", card.height > 24.dp)
        assertTrue("pile height ${pile.height}", pile.height > 24.dp)
    }

    @Test
    fun aLoneCardShows() = assertCardsDrawn(listOf(row("tasks")))

    @Test
    fun aPileShows() = assertCardsDrawn(listOf(row("queued"), row("tasks")))
}
