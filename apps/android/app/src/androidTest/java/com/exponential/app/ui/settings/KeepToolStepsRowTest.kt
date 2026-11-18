package com.exponential.app.ui.settings

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.data.chatstore.KeepToolSteps
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.ExponentialTheme
import com.exponential.app.ui.theme.glassGroup
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EDE-114 (chat-store.json THE STORAGE SETTING): the settings root's General
 * group with `Keep tool steps` (THE picker row, its value the picked window),
 * and a pick through the shared Picker sheet. The group is written to the
 * app's files (`keep-tool-steps.png`) for the PR picture.
 */
@RunWith(AndroidJUnit4::class)
class KeepToolStepsRowTest {
    @get:Rule val composeRule = createComposeRule()

    @Test
    fun theRowShowsTheWindowAndAPickChangesIt() {
        var picked: Int? = KeepToolSteps.DEFAULT_DAYS
        composeRule.setContent {
            ExponentialTheme {
                AppBackground {
                    var days by remember { mutableStateOf<Int?>(KeepToolSteps.DEFAULT_DAYS) }
                    Column(
                        Modifier.fillMaxSize().padding(horizontal = 16.dp, vertical = 24.dp).testTag("general"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        SectionHeader("General")
                        Column(Modifier.fillMaxWidth().glassGroup().padding(vertical = 4.dp)) {
                            SettingsRow(icon = ExpIcons.settingsRate, title = "Rate our app", trailingIcon = ExpIcons.uiExternalLink, onClick = {})
                            KeepToolStepsRow(days) {
                                days = it
                                picked = it
                            }
                        }
                    }
                }
            }
        }
        composeRule.onNodeWithText("Keep tool steps").assertExists()
        composeRule.onNodeWithText("30 days").assertExists()
        composeRule.waitForIdle()
        val image = composeRule.onRoot().captureToImage().asAndroidBitmap()
        val out = File(InstrumentationRegistry.getInstrumentation().targetContext.filesDir, "keep-tool-steps.png")
        out.outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }

        composeRule.onNodeWithTag(KeepToolSteps.TEST_ID).performClick()
        composeRule.onAllNodesWithText("90 days").onFirst().performClick()
        composeRule.waitForIdle()
        assertEquals(90, picked)
    }
}
