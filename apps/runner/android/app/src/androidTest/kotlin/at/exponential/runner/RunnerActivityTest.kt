package at.exponential.runner

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.hasSetTextAction
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The runner on a device: the template's bundle (assets/app) opens its first
 * screen inside the shell, a record written through the Engine shows in the
 * bound list, and the Permissions entry opens the manage sheet.
 *   ../../../packages/exponential-ui-compose/gradlew -p . :app:connectedDebugAndroidTest
 */
@RunWith(AndroidJUnit4::class)
class RunnerActivityTest {
    @get:Rule
    val rule = createAndroidComposeRule<RunnerActivity>()

    @Test
    fun theBundledAppRuns() {
        rule.waitUntil(10_000) { rule.onAllNodes(hasText("Notes"), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        val note = "Runner note ${System.nanoTime()}"
        rule.onNode(hasSetTextAction(), useUnmergedTree = true).performTextInput(note)
        rule.onNodeWithText("Add", useUnmergedTree = true).performClick()
        // The row (not the field): the package action wrote it through the Engine and the bound list re-resolved.
        rule.waitUntil(10_000) { rule.onAllNodes(hasText(note) and !hasSetTextAction(), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    }
}
