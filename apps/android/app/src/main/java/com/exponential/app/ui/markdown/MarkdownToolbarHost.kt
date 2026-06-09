package com.exponential.app.ui.markdown

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity

/**
 * Hosts the markdown formatting toolbar so it can float directly above the
 * keyboard (the Compose analog of iOS's UITextView.inputAccessoryView). The
 * inline editor [MarkdownEditor] can't pin a child above the IME because it
 * lives inside a scrolling column, so instead each focused editor registers its
 * [EditorModel] with the [MarkdownToolbarController] and a single screen-level
 * overlay renders the bar, bottom-anchored with `imePadding()`, only while an
 * editor field is focused and the keyboard is up.
 */
@Stable
class MarkdownToolbarController {
    /** The editor whose field currently has focus (last-focus-wins). */
    var activeModel by mutableStateOf<EditorModel?>(null)

    /** Image-picker action + flag for the active editor (its picker launcher lives in that editor's composition). */
    var onPickImage by mutableStateOf<() -> Unit>({})
    var imageEnabled by mutableStateOf(false)
}

val LocalMarkdownToolbarController = compositionLocalOf<MarkdownToolbarController?> { null }

/**
 * Wrap a screen that hosts editable [MarkdownEditor]s in this so the floating
 * toolbar exists for them. Provides the controller and draws the overlay as a
 * sibling above the screen content (so it isn't clipped by the content's scroll).
 */
@Composable
fun ProvideMarkdownToolbar(content: @Composable () -> Unit) {
    val controller = remember { MarkdownToolbarController() }
    CompositionLocalProvider(LocalMarkdownToolbarController provides controller) {
        Box(Modifier.fillMaxSize()) {
            content()
            MarkdownToolbarOverlay(
                controller = controller,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }
    }
}

@Composable
private fun MarkdownToolbarOverlay(controller: MarkdownToolbarController, modifier: Modifier = Modifier) {
    val model = controller.activeModel ?: return
    // Show only while a field is focused AND the keyboard is up; imePadding then
    // seats the bar directly on top of the keyboard and rides its animation.
    val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    if (model.focusedRowId == null || !imeVisible) return
    Box(modifier.fillMaxWidth().imePadding()) {
        // key(model): the toolbar is a single hoisted instance reused across every
        // editor on the screen, so reset its internal state (e.g. the link dialog)
        // when the focused editor changes — otherwise a dialog opened for one
        // editor would act on the next one.
        key(model) {
            MarkdownToolbar(
                model = model,
                onPickImage = controller.onPickImage,
                imageEnabled = controller.imageEnabled,
            )
        }
    }
}
