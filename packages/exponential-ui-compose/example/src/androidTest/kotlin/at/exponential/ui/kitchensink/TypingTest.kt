package at.exponential.ui.kitchensink

import android.content.Intent
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The typing test: 40 characters into the host-owned Title field in ONE
 * burst with the example host's 150 ms echo; every character lands, in
 * order, and the `host:` line shows the same text (the field → model →
 * host → `setData` → echo path end to end on the device).
 */
@RunWith(AndroidJUnit4::class)
class TypingTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @OptIn(ExperimentalTestApi::class)
    @Test
    fun fortyCharactersLandInOrder() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val intent = Intent(context, MainActivity::class.java).putExtra("shot", "exponential-ui-kitchen-sink")
        ActivityScenario.launch<MainActivity>(intent).use {
            val title = hasSetTextAction() and hasContentDescription("Title")
            compose.waitUntilAtLeastOneExists(title, 10_000)
            val field = compose.onNode(title)
            field.performClick()
            val text = "The quick brown fox jumps over the lazy dog".take(40)
            field.performTextInput(text)
            compose.waitUntil(5_000) {
                compose.onNodeWithTag("host-echo").fetchSemanticsNode().config
                    .getOrNull(SemanticsProperties.Text)?.joinToString { it.text } == "host: $text"
            }
            val value = field.fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text
            assertEquals(text, value)
            val echo = compose.onNodeWithTag("host-echo").fetchSemanticsNode().config.getOrNull(SemanticsProperties.Text)?.joinToString { it.text }
            assertEquals("host: $text", echo)
        }
    }
}
