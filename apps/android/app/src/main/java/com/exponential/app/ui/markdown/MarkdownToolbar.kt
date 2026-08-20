package com.exponential.app.ui.markdown

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.SizeTransform
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.PopupProperties
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.Motion

private val PillBg = Color.White.copy(alpha = 0.10f)
private val IconInactive = Color.White.copy(alpha = 0.65f)
private val SepColor = Color.White.copy(alpha = 0.12f)

/**
 * Formatting rail driving the [EditorModel] — ONE pill with three modes
 * (EXP-568, mirroring web/iOS): [RailMode.Main] holds the insert + block
 * affordances plus a pinned keyboard-down button, [RailMode.Text] the inline
 * formatting (heading levels, bold/italic/strikethrough, clear), and
 * [RailMode.Link] a URL field. The mode lives on the [MarkdownToolbarController]
 * rather than here, so it survives the overlay's remounts (a focus handoff
 * unmounts this composable, see [MarkdownToolbarHost]).
 *
 * There is deliberately no `@` button: typed `@` autocomplete covers mentions,
 * and the comment composer keeps its own row of buttons.
 */
@Composable
fun MarkdownToolbar(
    model: EditorModel,
    onPickImage: () -> Unit,
    imageEnabled: Boolean,
    // EXP-327: non-null turns the image button into a "Photo library / Files"
    // menu — the ONE attach affordance on the screen.
    onPickFile: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    val controller = LocalMarkdownToolbarController.current
    val railMode = controller?.railMode ?: RailMode.Main
    // EXP-523: `transitionSpec` is a plain lambda, not a composable one, so the
    // reduce-motion flag is read here and captured.
    val reduceMotion = LocalReduceMotion.current

    Column(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 6.dp, vertical = 4.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(PillBg),
        ) {
            AnimatedContent(
                targetState = railMode,
                transitionSpec = {
                    val spec = Motion.standard<Float>(reduceMotion)
                    (fadeIn(spec) togetherWith fadeOut(spec))
                        .using(SizeTransform(clip = false))
                },
                label = "markdown-rail",
            ) { mode ->
                if (mode == RailMode.Link && controller != null) {
                    LinkRail(controller = controller, model = model)
                } else {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(Modifier.weight(1f)) {
                            Row(
                                modifier = Modifier
                                    .horizontalScroll(rememberScrollState())
                                    .padding(horizontal = 4.dp, vertical = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(1.dp),
                            ) {
                                if (mode == RailMode.Text) {
                                    TextRailItems(model = model, controller = controller)
                                } else {
                                    MainRailItems(
                                        model = model,
                                        controller = controller,
                                        onPickImage = onPickImage,
                                        onPickFile = onPickFile,
                                        imageEnabled = imageEnabled,
                                    )
                                }
                            }
                        }
                        // Pinned OUTSIDE the scroll so dismissing the keyboard is
                        // always one tap away, however long the rail gets.
                        if (mode == RailMode.Main) {
                            val focusManager = LocalFocusManager.current
                            val keyboard = LocalSoftwareKeyboardController.current
                            ToolbarButton(ExpIcons.uiChevronDown, "Hide keyboard", active = false) {
                                focusManager.clearFocus()
                                keyboard?.hide()
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MainRailItems(
    model: EditorModel,
    controller: MarkdownToolbarController?,
    onPickImage: () -> Unit,
    onPickFile: (() -> Unit)?,
    imageEnabled: Boolean,
) {
    val activeRowId = model.activeRowId
    val attrs = model.attrsAtCaret()
    var attachMenuOpen by remember { mutableStateOf(false) }
    var listMenuOpen by remember { mutableStateOf(false) }

    // EXP-551: the picker sheet is FOCUSABLE, which drops the IME and unmounts
    // this whole overlay — so the sheet's open state lives on the controller
    // (hosted by ProvideMarkdownToolbar, which stays mounted) and this button
    // only requests it.
    if (controller != null) {
        ToolbarButton(ExpIcons.editorEmoji, "Insert emoji", active = false) {
            controller.requestEmojiPicker(model)
        }
    }
    if (onPickFile == null) {
        ToolbarButton(ExpIcons.editorImage, "Image", active = false, enabled = imageEnabled) { onPickImage() }
    } else {
        Box {
            ToolbarButton(ExpIcons.editorImage, "Attach", active = false, enabled = imageEnabled) {
                attachMenuOpen = !attachMenuOpen
            }
            // This whole toolbar only exists while the IME is up
            // (MarkdownToolbarHost gates the overlay on imeVisible), so a
            // FOCUSABLE popup would take focus, drop the keyboard, and tear its
            // own menu out of composition — the same trap BlockTextField's
            // `@`/`#` menu documents. Non-focusable keeps the keyboard, so
            // dismissal rides the item taps, a re-tap of the button, and
            // BackHandler.
            BackHandler(enabled = attachMenuOpen) { attachMenuOpen = false }
            GlassDropdownMenu(
                expanded = attachMenuOpen,
                onDismissRequest = { attachMenuOpen = false },
                properties = PopupProperties(focusable = false),
            ) {
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.uiAttach, contentDescription = null) },
                    text = { Text("Files") },
                    onClick = {
                        attachMenuOpen = false
                        onPickFile()
                    },
                )
                GlassMenuItem(
                    leadingIcon = { Icon(ExpIcons.editorImage, contentDescription = null) },
                    text = { Text("Photo library") },
                    onClick = {
                        attachMenuOpen = false
                        onPickImage()
                    },
                )
            }
        }
    }
    ToolbarButton(ExpIcons.editorIssueRef, "Reference an issue", active = false) {
        model.insertPlainText("#")
    }
    if (controller != null) {
        ToolbarButton(ExpIcons.editorLink, "Link", active = false) {
            controller.requestLinkEditor(model)
        }
    }
    Separator()
    if (controller != null) {
        ToolbarButton(ExpIcons.editorTextFormat, "Text formatting", active = false) {
            controller.railMode = RailMode.Text
            // Nothing ran a model op, so the field's focus must be handed back
            // by hand (see the rationale in EditorModel.toggleMark).
            model.setFocused(model.activeRowId)
        }
    }
    // ONE list button; the three kinds live in its menu (same non-focusable
    // popup treatment as the attach menu above).
    val activeListType = attrs?.takeIf { it.kind == BlockKind.ListItem }?.listType
    Box {
        ToolbarButton(
            icon = when (activeListType) {
                ListType.Ordered -> ExpIcons.editorListOrdered
                ListType.Checklist -> ExpIcons.editorListTodo
                else -> ExpIcons.editorList
            },
            label = "Lists",
            active = activeListType != null,
        ) {
            listMenuOpen = !listMenuOpen
        }
        BackHandler(enabled = listMenuOpen) { listMenuOpen = false }
        GlassDropdownMenu(
            expanded = listMenuOpen,
            onDismissRequest = { listMenuOpen = false },
            properties = PopupProperties(focusable = false),
        ) {
            ListMenuItem(ExpIcons.editorList, "List", ListType.Bullet, activeListType) { type ->
                listMenuOpen = false
                activeRowId?.let { model.toggleList(it, type) }
            }
            ListMenuItem(ExpIcons.editorListOrdered, "Numbered list", ListType.Ordered, activeListType) { type ->
                listMenuOpen = false
                activeRowId?.let { model.toggleList(it, type) }
            }
            ListMenuItem(ExpIcons.editorListTodo, "Checklist", ListType.Checklist, activeListType) { type ->
                listMenuOpen = false
                activeRowId?.let { model.toggleList(it, type) }
            }
        }
    }
    ToolbarButton(ExpIcons.editorQuote, "Quote", active = attrs?.kind == BlockKind.Blockquote) {
        activeRowId?.let { model.toggleQuote(it) }
    }
    ToolbarButton(ExpIcons.editorCode, "Code block", active = attrs?.kind == BlockKind.CodeBlock) {
        activeRowId?.let { model.toggleCodeBlock(it) }
    }
}

@Composable
private fun ListMenuItem(
    icon: ImageVector,
    label: String,
    type: ListType,
    activeType: ListType?,
    onClick: (ListType) -> Unit,
) {
    GlassMenuItem(
        leadingIcon = { Icon(icon, contentDescription = null) },
        text = { Text(label) },
        trailingIcon = if (activeType == type) {
            { Icon(ExpIcons.uiCheck, contentDescription = null) }
        } else {
            null
        },
        onClick = { onClick(type) },
    )
}

@Composable
private fun TextRailItems(
    model: EditorModel,
    controller: MarkdownToolbarController?,
) {
    val attrs = model.attrsAtCaret()
    val selection = model.activeSelection()
    val headingLevel = if (attrs?.kind == BlockKind.Heading) attrs.headingLevel else 0

    // With a real selection the mark state IS the selection's coverage; at a
    // collapsed caret it is whatever the next typed characters will inherit
    // (EditorModel's pending-mark queue).
    fun markActive(kind: InlineKind): Boolean {
        val (rid, range) = selection ?: return false
        return if (range.last > range.first) {
            MarkOps.hasMarkOver(model.marksFor(rid), range.first, range.last, kind)
        } else {
            model.pendingMarkActive(rid, range.first, kind)
        }
    }

    fun toggle(kind: InlineKind) {
        val (rid, range) = selection ?: return
        if (range.last > range.first) model.toggleMark(rid, range, kind)
        else model.togglePendingMark(rid, range.first, kind)
    }

    ToolbarButton(ExpIcons.uiBack, "Back", active = false) {
        controller?.railMode = RailMode.Main
        model.setFocused(model.activeRowId)
    }
    ToolbarButton(ExpIcons.editorText, "Text", active = headingLevel == 0) {
        model.activeRowId?.let { model.setHeading(it, 0) }
    }
    ToolbarButton(ExpIcons.editorHeading1, "Heading 1", active = headingLevel == 1) {
        model.activeRowId?.let { model.setHeading(it, 1) }
    }
    ToolbarButton(ExpIcons.editorHeading2, "Heading 2", active = headingLevel == 2) {
        model.activeRowId?.let { model.setHeading(it, 2) }
    }
    ToolbarButton(ExpIcons.editorHeading3, "Heading 3", active = headingLevel == 3) {
        model.activeRowId?.let { model.setHeading(it, 3) }
    }
    Separator()
    ToolbarButton(ExpIcons.editorBold, "Bold", active = markActive(InlineKind.Bold)) {
        toggle(InlineKind.Bold)
    }
    ToolbarButton(ExpIcons.editorItalic, "Italic", active = markActive(InlineKind.Italic)) {
        toggle(InlineKind.Italic)
    }
    ToolbarButton(
        ExpIcons.editorStrikethrough,
        "Strikethrough",
        active = markActive(InlineKind.Strikethrough),
    ) {
        toggle(InlineKind.Strikethrough)
    }
    ToolbarButton(ExpIcons.editorClearFormatting, "Clear formatting", active = false) {
        val (rid, range) = selection ?: return@ToolbarButton
        model.clearFormatting(rid, range)
    }
}

/**
 * The URL rail. Its field holds the OS focus (and therefore the keyboard) while
 * it is up, which is exactly why the target + selection were captured on the
 * controller before it opened — see [MarkdownToolbarController.linkEditTarget].
 */
@Composable
private fun LinkRail(
    controller: MarkdownToolbarController,
    model: EditorModel,
) {
    var url by remember { mutableStateOf(controller.linkEditInitialUrl) }
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focusRequester.requestFocus() } }

    fun cancel() {
        val rowId = controller.linkEditSelection?.first
        controller.dismissLinkEditor()
        rowId?.let { model.setFocused(it) }
    }

    fun apply() {
        val target = controller.linkEditTarget ?: model
        val sel = controller.linkEditSelection
        val href = url.trim()
        if (sel != null) {
            val (rowId, range) = sel
            if (range.last > range.first) {
                // EXP-572: an emptied field over a range REMOVES the link
                // (toggleMark unwraps on a blank href) — the only way to
                // un-link text, matching web/iOS.
                target.toggleMark(rowId, range, InlineKind.Link, href)
            } else if (href.isNotEmpty()) {
                // No selection to wrap: the URL becomes its own link text.
                target.insertLinkText(rowId, range.first, text = href, url = href)
            }
        }
        val rowId = sel?.first
        controller.dismissLinkEditor()
        rowId?.let { target.setFocused(it) }
    }

    BackHandler { cancel() }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 4.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToolbarButton(ExpIcons.uiClose, "Cancel link", active = false) { cancel() }
        Box(
            modifier = Modifier
                .weight(1f)
                .padding(horizontal = 4.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            if (url.isEmpty()) {
                Text(
                    "https://",
                    style = MaterialTheme.typography.bodyMedium,
                    color = Color.White.copy(alpha = 0.4f),
                )
            }
            BasicTextField(
                value = url,
                onValueChange = { url = it },
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyMedium.copy(color = Color.White),
                cursorBrush = SolidColor(Color.White),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Uri,
                    imeAction = ImeAction.Done,
                ),
                keyboardActions = KeyboardActions(onDone = { apply() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focusRequester),
            )
        }
        ToolbarButton(ExpIcons.uiCheck, "Apply link", active = false) { apply() }
    }
}

@Composable
private fun ToolbarButton(
    icon: ImageVector,
    label: String,
    active: Boolean,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    IconButton(onClick = onClick, enabled = enabled, modifier = Modifier.size(36.dp)) {
        Icon(
            icon,
            contentDescription = label,
            tint = if (!enabled) Color.White.copy(alpha = 0.3f) else if (active) MdStyle.Link else IconInactive,
            modifier = Modifier.size(18.dp),
        )
    }
}

@Composable
private fun Separator() {
    Spacer(
        modifier = Modifier
            .padding(horizontal = 3.dp)
            .width(1.dp)
            .height(18.dp)
            .background(SepColor),
    )
}
