package com.exponential.app

import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import com.exponential.app.domain.ImageActions
import com.exponential.app.ui.components.LocalOverlayToaster
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SpecimenSamples
import com.exponential.app.ui.components.ToastHost
import com.exponential.app.ui.components.Toaster
import com.exponential.app.ui.markdown.MarkdownView
import com.exponential.app.ui.styleguide.ResultsTabSpecimenScreen
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.ExponentialTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * #EXP-1948 on a device, no backend: a result picture and a Markdown image
 * answer a long press with Copy image + Share; Copy puts the picture's PNG
 * on the clipboard as a FileProvider `content://` and toasts `Image copied`;
 * another website's picture is a link. With `-e proofShots true` each step's
 * screen lands in the app's `files/image-actions-proof/`.
 */
class ImageActionsDeviceTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun shot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("proofShots") != "true") return
        compose.waitForIdle()
        Thread.sleep(600)
        val bitmap = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: return
        val dir = File(context.filesDir, "image-actions-proof").apply { mkdirs() }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun show(content: @androidx.compose.runtime.Composable () -> Unit) {
        compose.runOnUiThread {
            compose.activity.setContent {
                ExponentialTheme {
                    val toaster = remember { Toaster() }
                    CompositionLocalProvider(
                        LocalContentColor provides MaterialTheme.colorScheme.onBackground,
                        LocalToaster provides toaster,
                        LocalOverlayToaster provides toaster,
                    ) {
                        Box(Modifier.fillMaxSize()) {
                            content()
                            ToastHost(toaster)
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    /** A sample picture's bytes in the Tall cache, where a result's Copy reads them first. */
    private fun seedResult(id: String) {
        val bytes = SpecimenSamples.resolver(context)("/api/attachments/$id") as ByteArray
        File(File(File(context.cacheDir, "attachments"), id), "original").apply { parentFile?.mkdirs() }.writeBytes(bytes)
    }

    private fun waitFor(matcher: androidx.compose.ui.test.SemanticsMatcher) {
        compose.waitUntil(10_000) { compose.onAllNodes(matcher, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun aResultPictureCopiesItsPng() {
        seedResult("att-w")
        show { ResultsTabSpecimenScreen() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("result-picture")).fetchSemanticsNodes().isNotEmpty() }
        Thread.sleep(1500)
        compose.onAllNodes(hasTestTag("result-picture"))[0].performTouchInput { longClick() }
        waitFor(hasTestTag(ImageActions.TestIds.COPY_IMAGE))
        waitFor(hasTestTag(ImageActions.TestIds.SHARE))
        shot("result-menu")
        compose.onNodeWithTag(ImageActions.TestIds.COPY_IMAGE, useUnmergedTree = true).performClick()
        waitFor(hasText(ImageActions.Copy.COPIED))
        shot("result-copied")
        val clip = (context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip!!
        val uri = clip.getItemAt(0).uri
        assertEquals("content", uri.scheme)
        assertEquals("${context.packageName}.fileprovider", uri.authority)
        assertTrue(uri.path!!.endsWith(".png"))
        val head = context.contentResolver.openInputStream(uri)!!.use { it.readNBytes(8) }
        assertEquals(ImageActions.Encoding.Passthrough, ImageActions.copyEncoding(head))
    }

    @Test
    fun aResultPictureShares() {
        seedResult("att-w")
        show { ResultsTabSpecimenScreen() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("result-picture")).fetchSemanticsNodes().isNotEmpty() }
        Thread.sleep(1500)
        compose.onAllNodes(hasTestTag("result-picture"))[0].performTouchInput { longClick() }
        waitFor(hasTestTag(ImageActions.TestIds.SHARE))
        compose.onNodeWithTag(ImageActions.TestIds.SHARE, useUnmergedTree = true).performClick()
        Thread.sleep(2500)
        shot("result-share-sheet")
        InstrumentationRegistry.getInstrumentation().uiAutomation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK)
    }

    @Test
    fun aMarkdownImageAndAnotherWebsitesPicture() {
        val bytes = SpecimenSamples.resolver(context)("/api/attachments/att-w") as ByteArray
        val data = "data:image/png;base64," + java.util.Base64.getEncoder().encodeToString(bytes)
        val markdown = "The picker in its mixed state:\n\n![Mixed state]($data)\n\nThe reference from the design site:\n\n![](https://example.com/picker.png)\n"
        show {
            AppBackground(Modifier.testTag("md")) {
                Column(Modifier.fillMaxSize().statusBarsPadding().padding(16.dp)) { MarkdownView(markdown) }
            }
        }
        waitFor(hasText("Image: example.com"))
        Thread.sleep(1500)
        shot("markdown-link")
        compose.onNodeWithTag("md").performTouchInput { longClick(androidx.compose.ui.geometry.Offset(width * 0.4f, height * 0.18f)) }
        waitFor(hasTestTag(ImageActions.TestIds.COPY_IMAGE))
        shot("markdown-menu")
        compose.onNodeWithTag(ImageActions.TestIds.COPY_IMAGE, useUnmergedTree = true).performClick()
        waitFor(hasText(ImageActions.Copy.COPIED))
        shot("markdown-copied")
    }
}
