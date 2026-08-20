package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.emoji.EmojiPickerSheet
import com.exponential.app.ui.theme.GlassTokens

/**
 * Which set of controls the one rail shows (EXP-568) — the Android half of the
 * shared main/text/link rail the web and iOS editors grew at the same time.
 * `Main` is the block/insert rail, `Text` the inline-formatting rail, `Link`
 * the URL field.
 */
enum class RailMode { Main, Text, Link }

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

    /** Which rail the toolbar shows; reset to [RailMode.Main] on editor switch. */
    var railMode by mutableStateOf(RailMode.Main)

    /** Image-picker action + flag for the active editor (its picker launcher lives in that editor's composition). */
    var onPickImage by mutableStateOf<() -> Unit>({})
    var imageEnabled by mutableStateOf(false)

    /**
     * Document-picker action for the active editor (EXP-327). Non-null turns the
     * toolbar's image button into a "Photo library / Files" menu — the single
     * attach affordance, which is why the Files section no longer carries a
     * paperclip of its own. Null keeps the plain one-tap image button (the
     * comment composer, and any editor whose host can't take an attachment).
     */
    var onPickFile by mutableStateOf<(() -> Unit)?>(null)

    /**
     * Measured height of the visible bar, 0 while it is hidden. The `@`/`#`
     * autocomplete menu subtracts it (on top of the IME inset) so it never
     * places itself behind the toolbar (EXP-322).
     */
    var toolbarHeightPx by mutableIntStateOf(0)

    /**
     * The editor a pending emoji pick belongs to, null when no picker is up
     * (EXP-551). The sheet is FOCUSABLE — opening it hides the IME, which
     * unmounts the toolbar overlay and clears [activeModel] — so the target is
     * captured here at request time and the sheet is hosted by
     * [ProvideMarkdownToolbar], which stays mounted.
     */
    var emojiPickerTarget by mutableStateOf<EditorModel?>(null)
        private set

    fun requestEmojiPicker(model: EditorModel) {
        emojiPickerTarget = model
    }

    fun dismissEmojiPicker() {
        emojiPickerTarget = null
    }

    /**
     * The editor a pending LINK edit belongs to, with the selection captured at
     * request time — the same survival trick [emojiPickerTarget] documents.
     * Focusing the rail's URL field takes OS focus off the editor, which nulls
     * its `focusedRowId` (and hence [activeModel]) and would otherwise unmount
     * the overlay mid-edit; the host keeps rendering against this target while
     * it is non-null.
     */
    var linkEditTarget by mutableStateOf<EditorModel?>(null)
        private set

    /** (rowId, range) the link applies to — the editor's selection is gone by then. */
    var linkEditSelection by mutableStateOf<Pair<String, IntRange>?>(null)
        private set

    /** Existing href under the captured selection, prefilled into the URL field. */
    var linkEditInitialUrl by mutableStateOf("")
        private set

    fun requestLinkEditor(model: EditorModel) {
        val sel = model.activeSelection() ?: return
        linkEditSelection = sel
        linkEditInitialUrl = model.linkHrefAt(sel.first, sel.second).orEmpty()
        linkEditTarget = model
        railMode = RailMode.Link
    }

    fun dismissLinkEditor() {
        linkEditTarget = null
        linkEditSelection = null
        linkEditInitialUrl = ""
        railMode = RailMode.Main
    }
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
        // Show only while a field is focused AND the keyboard is up; imePadding
        // then seats the bar directly on top of the keyboard and rides its
        // animation.
        val model = controller.activeModel
        val imeVisible = WindowInsets.ime.getBottom(LocalDensity.current) > 0
        // EXP-568: the link rail's URL field owns the focus while it is up, so
        // `activeModel` is null then — the captured link target keeps the rail
        // mounted (and is what it renders against).
        val railModel = model ?: controller.linkEditTarget
        val toolbarVisible =
            (model?.focusedRowId != null || controller.linkEditTarget != null) && imeVisible
        // A genuinely DIFFERENT editor taking focus resets the rail to main.
        // Null transitions are ignored on purpose: every rail tap briefly drops
        // the field's OS focus (see EditorModel.toggleMark), which would
        // otherwise kick the user out of the text rail on every bold tap.
        val railOwner = remember { mutableStateOf<EditorModel?>(null) }
        LaunchedEffect(model) {
            val m = model ?: return@LaunchedEffect
            if (m !== railOwner.value) {
                railOwner.value = m
                if (controller.linkEditTarget == null) controller.railMode = RailMode.Main
            }
        }
        // While the bar is up it floats OVER the bottom of the content, so the
        // content is inset by the bar's measured height — otherwise the bar
        // covers exactly the focused line / the comment composer's send row
        // that imePadding just brought above the keyboard (EXP-135).
        var toolbarHeightPx by remember { mutableIntStateOf(0) }
        val density = LocalDensity.current
        val bottomInset = if (toolbarVisible) with(density) { toolbarHeightPx.toDp() } else 0.dp
        // Published for the `@`/`#` autocomplete menu's placement (EXP-322).
        // Via an effect, not a write during composition.
        LaunchedEffect(toolbarVisible, toolbarHeightPx) {
            controller.toolbarHeightPx = if (toolbarVisible) toolbarHeightPx else 0
        }
        Box(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize().padding(bottom = bottomInset)) {
                content()
            }
            if (toolbarVisible && railModel != null) {
                MarkdownToolbarOverlay(
                    controller = controller,
                    model = railModel,
                    modifier = Modifier.align(Alignment.BottomCenter),
                    onHeightChanged = { toolbarHeightPx = it },
                )
            }
            // EXP-551: hosted HERE, not in the toolbar — the sheet takes focus,
            // the IME goes away and the toolbar overlay leaves composition, so a
            // sheet owned by the bar would tear itself down as it opened.
            val emojiTarget = controller.emojiPickerTarget
            if (emojiTarget != null) {
                EmojiPickerSheet(
                    onPick = { unicode ->
                        // insertPlainText re-asserts focus on the row, so the
                        // keyboard comes back with the caret after the emoji.
                        emojiTarget.insertPlainText(unicode)
                    },
                    onDismiss = { controller.dismissEmojiPicker() },
                )
            }
        }
    }
}

@Composable
private fun MarkdownToolbarOverlay(
    controller: MarkdownToolbarController,
    model: EditorModel,
    modifier: Modifier = Modifier,
    onHeightChanged: (Int) -> Unit = {},
) {
    // Near-opaque zinc surface + top hairline: the bar floats over arbitrary
    // content, so a translucent fill left the pill nearly invisible (EXP-25).
    Column(
        modifier
            .fillMaxWidth()
            .imePadding()
            // Measured AFTER imePadding so the reported height is the bar
            // itself, not bar + keyboard.
            .onSizeChanged { onHeightChanged(it.height) }
            .background(GlassTokens.BackgroundBottom.copy(alpha = 0.97f)),
    ) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(GlassTokens.Hairline)
                .background(GlassTokens.StrokeSection),
        )
        // key(model): the toolbar is a single hoisted instance reused across every
        // editor on the screen, so reset any internal state when the focused
        // editor changes — otherwise state kept for one editor would act on the
        // next one.
        key(model) {
            MarkdownToolbar(
                model = model,
                onPickImage = controller.onPickImage,
                onPickFile = controller.onPickFile,
                imageEnabled = controller.imageEnabled,
            )
        }
    }
}
