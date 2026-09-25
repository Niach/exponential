package com.exponential.app.ui.components

import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.exponential.app.ui.theme.ExponentialTheme
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * EXP-1043: the sub-shell's `BackHandler` lives INSIDE a [GlassSheet], which
 * material3 hosts in its own `ComponentDialog` window that registers its own
 * dismiss-on-back callback. The ordering ("one level back, and only then the
 * sheet") is not something the JVM contract test can see, so it is pinned
 * here, on a device.
 */
@RunWith(AndroidJUnit4::class)
class SubShellBackTest {

    @get:Rule
    val rule = createComposeRule()

    @Test
    fun systemBackReturnsOneLevelBeforeItDismissesTheSheet() {
        var dismissed = false
        rule.setContent {
            ExponentialTheme {
                var open by remember { mutableStateOf(true) }
                if (open) {
                    GlassSheet(
                        title = "Device settings",
                        onDismiss = {
                            open = false
                            dismissed = true
                        },
                    ) {
                        SubShellHost {
                            OptionGroup {
                                SubShell(label = "Workflow settings") {
                                    Text("Strong model")
                                }
                            }
                        }
                    }
                }
            }
        }

        rule.onNodeWithText("Workflow settings").performClick()
        rule.waitForIdle()
        rule.onNodeWithText("Strong model").assertIsDisplayed()

        // A page is open: back returns ONE level, and the sheet stays put.
        Espresso.pressBack()
        rule.waitForIdle()
        assertFalse("system back dismissed the whole sheet", dismissed)
        rule.onNodeWithText("Strong model").assertDoesNotExist()
        rule.onNodeWithText("Workflow settings").assertIsDisplayed()

        // Back at the card, back is the sheet's own again.
        Espresso.pressBack()
        rule.waitForIdle()
        assertTrue(dismissed)
    }
}
