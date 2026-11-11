package at.exponential.ui.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.AutofillNode
import androidx.compose.ui.autofill.AutofillType
import androidx.compose.ui.composed
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalAutofill
import androidx.compose.ui.platform.LocalAutofillTree
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.composerKey
import at.exponential.ui.model.composerMenuKeysOn
import at.exponential.ui.model.composerPaste
import at.exponential.ui.model.composerSubmit
import at.exponential.ui.model.field
import at.exponential.ui.model.fieldCommitted
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.selectSearchClear
import at.exponential.ui.model.selectSearchClearable
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.fieldSubmitted
import at.exponential.ui.model.fire
import at.exponential.ui.model.searchKey
import at.exponential.ui.theme.ResolvedTextStyle

/**
 * The host-owned text input (the VAPP-4 verdict): a `BasicTextField` whose
 * [TextFieldValue] lives in its OWN state keyed by the node id, seeded from
 * the model's field text. The model never pushes a value per keystroke, so
 * a recomposition mid-burst cannot lose or overwrite input. Every edit →
 * `fieldEdited` (a revision + the 150 ms `change` debounce); focus in/out →
 * `fieldFocused` (blur commits); the IME action Done → `fieldCommitted`,
 * Send (a [submitsOnReturn] Composer, also a bare hardware Enter) →
 * `composerSubmit`. A MODEL write (an echo while idle, the composer clear)
 * bumps `FieldState.writeGeneration` and reloads the value, the cursor at
 * the end. The chrome is the node box NodeView paints: the field draws no
 * decoration beyond the placeholder.
 */
@Composable
fun OwnedTextField(
    index: Int,
    model: SurfaceModel,
    multiline: Boolean,
    placeholder: String,
    textStyle: TextStyle,
    placeholderColor: Color,
    disabled: Boolean,
    submitsOnReturn: Boolean,
    accessibilityLabel: String,
    modifier: Modifier = Modifier,
    /** The owner Input's `type` (catalog enum `inputType`): masking, autofill, autocorrection. */
    inputType: String = "",
    keyboardType: KeyboardType = keyboardTypeOf(inputType),
    /** #EXP-1629: a `row` Input's value (and placeholder) sits at the end. */
    endAligned: Boolean = false,
) {
    val secure = inputType == "password"
    val state = model.field(index)
    val id = model.node(index)?.id ?: "#$index"
    // EXP-1259: a SearchList's (or a searchable submenu's) field.
    val searchList = model.node(index)?.component == "SearchList"
    val submenu = searchList && model.node(index)?.ownerComponent == "Menu"
    var value by remember(id) {
        val t = state?.text ?: ""
        mutableStateOf(TextFieldValue(t, TextRange(t.length)))
    }
    var focused by remember(id) { mutableStateOf(false) }
    // VAPP-208: focus selects a seeded value, so typing replaces it. The tap
    // that focuses the field places its caret after the focus change: while
    // the window is open, a selection-only change keeps everything selected.
    val selectsOnFocus = keyboardType == KeyboardType.Decimal
    var selectingAll by remember(id) { mutableStateOf(false) }
    if (selectsOnFocus) {
        LaunchedEffect(focused) {
            if (!focused) return@LaunchedEffect
            selectingAll = true
            value = value.copy(selection = TextRange(0, value.text.length))
            delay(SELECT_ON_FOCUS_WINDOW_MS)
            selectingAll = false
        }
    }
    val generation = state?.writeGeneration ?: 0
    LaunchedEffect(id, generation) {
        val t = state?.text ?: return@LaunchedEffect
        if (t != value.text) value = TextFieldValue(t, TextRange(t.length))
    }
    val focusManager = LocalFocusManager.current
    val requester = remember(id) { FocusRequester() }
    val request = model.focusRequest
    LaunchedEffect(request) {
        if (request == id) {
            runCatching { requester.requestFocus() }
            model.focusRequest = null
        }
    }
    val imeAction = when {
        submitsOnReturn -> ImeAction.Send
        multiline -> ImeAction.Default
        else -> ImeAction.Done
    }
    // #EDE-65 R: a Composer's paste with files (composerPaste).
    val paste = { submitsOnReturn && composerClipboard().let { (files, uris) -> model.composerPaste(index, files, uris) } }
    ComposerPasteToolbar(enabled = submitsOnReturn, onPaste = paste) {
        BasicTextField(
            value = value,
            onValueChange = { next ->
                val prev = value
                // #EDE-65: the IME composition the surface's menu keys pass by.
                state?.composing = next.composition != null
                if (submitsOnReturn && next.text.length == prev.text.length + 1 && next.text.count { it == '\n' } > prev.text.count { it == '\n' }) {
                    // Return in a composer submits instead of inserting a newline
                    // (#EDE-65: with menuKeys on it is the host's `key` Enter).
                    if (!model.composerKey(index, "Enter")) model.composerSubmit(index)
                    return@BasicTextField
                }
                value = if (selectingAll && next.text == prev.text) next.copy(selection = TextRange(0, next.text.length)) else next
                if (next.text != prev.text) selectingAll = false
                if (next.text != prev.text) model.fieldEdited(index, next.text)
            },
            modifier = modifier
                .focusRequester(requester)
                .onFocusChanged {
                    if (it.isFocused != focused) {
                        focused = it.isFocused
                        model.fieldFocused(index, it.isFocused)
                    }
                }
                .onPreviewKeyEvent { e ->
                    when {
                        // #EDE-65: the surface's keys take the menu keys first; this
                        // catches a Composer outside them (the same rule).
                        submitsOnReturn && model.composerMenuKeysOn(index) && menuKeyName(e.key) != null &&
                            !(e.isShiftPressed || e.isCtrlPressed || e.isAltPressed || e.isMetaPressed) && value.composition == null -> {
                            if (e.type == KeyEventType.KeyDown) model.composerKey(index, menuKeyName(e.key)!!)
                            true
                        }
                        // #EDE-65 R: a hardware Ctrl/⌘+V with files on the clipboard.
                        submitsOnReturn && e.type == KeyEventType.KeyDown && e.key == Key.V && (e.isCtrlPressed || e.isMetaPressed) && !e.isShiftPressed && paste() -> true
                        // An IME composition's Enter is the IME's (never a submit).
                        submitsOnReturn && e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter) && !e.isShiftPressed && value.composition == null -> {
                            model.composerSubmit(index)
                            true
                        }
                        // A hardware Enter in a single-line field submits (the IME's Done does too).
                        !multiline && !submitsOnReturn && e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter) -> {
                            model.fieldSubmitted(index)
                            true
                        }
                        // EXP-1259: a search list's field picks on Tab and leaves a
                        // searchable submenu on ← (→ in RTL) at the caret's start.
                        searchList && e.key == Key.Tab && !e.isShiftPressed && !e.isCtrlPressed -> {
                            if (e.type == KeyEventType.KeyDown) model.searchKey(index, "Tab")
                            true
                        }
                        searchList && submenu && e.type == KeyEventType.KeyDown && value.selection == TextRange(0) &&
                            e.key == (if (model.direction == "rtl") Key.DirectionRight else Key.DirectionLeft) -> {
                            model.searchKey(index, "ArrowLeft")
                            true
                        }
                        // Tab leaves a field (a multi-line one too), like the web's.
                        e.key == Key.Tab && !e.isCtrlPressed -> {
                            if (e.type == KeyEventType.KeyDown) focusManager.moveFocus(if (e.isShiftPressed) FocusDirection.Previous else FocusDirection.Next)
                            true
                        }
                        else -> false
                    }
                }
                .inputAutofill(inputType) { filled ->
                    value = TextFieldValue(filled, TextRange(filled.length))
                    model.fieldEdited(index, filled)
                }
                .semantics { contentDescription = accessibilityLabel },
            // `visibility: hidden` (an ancestor's too) takes no focus and no taps.
            enabled = !disabled && !LocalInvisible.current,
            textStyle = textStyle,
            singleLine = !multiline,
            maxLines = if (multiline) Int.MAX_VALUE else 1,
            keyboardOptions = KeyboardOptions(
                keyboardType = if (secure) KeyboardType.Password else keyboardType,
                imeAction = imeAction,
                autoCorrectEnabled = autoCorrectOf(inputType),
            ),
            keyboardActions = KeyboardActions(
                onDone = {
                    // Enter in a single-line field submits (a Form, a ChipInput's chip).
                    if (!multiline) model.fieldSubmitted(index) else model.fieldCommitted(index)
                    if (model.node(index)?.component != "ChipInput" && !searchList) focusManager.clearFocus()
                },
                onSend = { if (!model.composerKey(index, "Enter")) model.composerSubmit(index) },
            ),
            visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
            cursorBrush = SolidColor(textStyle.color.takeIf { it != Color.Unspecified } ?: Color.Black),
            decorationBox = { inner ->
                Box(Modifier.fillMaxSize(), contentAlignment = when {
                    multiline -> Alignment.TopStart
                    endAligned -> Alignment.CenterEnd
                    else -> Alignment.CenterStart
                }) {
                    if (value.text.isEmpty() && placeholder.isNotEmpty()) {
                        BasicText(
                            placeholder,
                            style = textStyle.copy(color = placeholderColor),
                            maxLines = if (multiline) Int.MAX_VALUE else 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    inner()
                }
            },
        )
    }
}

/** How long after a NumberField's focus the focusing tap's caret still lands. */
private const val SELECT_ON_FOCUS_WINDOW_MS = 300L

/** Autocorrection: off for addresses, numbers and secrets (iOS `InputTraits` too). */
internal fun autoCorrectOf(type: String): Boolean = type !in setOf("email", "url", "tel", "password", "number")

/**
 * The autofill types an `Input` `type` offers the platform's autofill
 * service (iOS: `textContentType`). `url` has no Android autofill type.
 */
@OptIn(ExperimentalComposeUiApi::class)
internal fun autofillTypesOf(type: String): List<AutofillType> = when (type) {
    "email" -> listOf(AutofillType.EmailAddress)
    "password" -> listOf(AutofillType.Password)
    "tel" -> listOf(AutofillType.PhoneNumber)
    else -> emptyList()
}

/**
 * Registers the field with the platform autofill service while its `type`
 * names an autofill kind: the node's window bounds, a request on focus,
 * and a fill writes the text through [onFill].
 */
@OptIn(ExperimentalComposeUiApi::class)
private fun Modifier.inputAutofill(type: String, onFill: (String) -> Unit): Modifier = composed {
    val types = autofillTypesOf(type)
    if (types.isEmpty()) return@composed this
    val autofill = LocalAutofill.current
    val tree = LocalAutofillTree.current
    val node = remember(types) { AutofillNode(autofillTypes = types, onFill = onFill) }
    DisposableEffect(node) {
        tree += node
        onDispose { tree.children.remove(node.id) }
    }
    this
        .onGloballyPositioned { node.boundingBox = it.boundsInWindow() }
        .onFocusChanged { if (it.isFocused) autofill?.requestAutofillForNode(node) else autofill?.cancelAutofillForNode(node) }
}

/** The keyboard an `Input` `type` asks for. */
internal fun keyboardTypeOf(type: String): KeyboardType = when (type) {
    "email" -> KeyboardType.Email
    "number" -> KeyboardType.Number
    "password" -> KeyboardType.Password
    "url" -> KeyboardType.Uri
    "tel" -> KeyboardType.Phone
    else -> KeyboardType.Text
}

/**
 * An Input / Textarea `.field`: the owned text field inside the field box
 * (the node's own visual paints the chrome), in the leaf's text style; the
 * placeholder in the `<Component>/placeholder` colour; `type: password`
 * masks.
 */
@Composable
internal fun TextFieldLeaf(cx: LeafContext, multiline: Boolean) {
    val owner = cx.ownerProps
    val placeholder = cx.part(cx.node.recipeComponent, "placeholder")
    val type = owner.str("type")
    if (!multiline && inputRowVariant(cx.props, owner)) return InputRowFieldLeaf(cx, placeholder.color)
    InnerBox(cx) {
        OwnedTextField(
            index = cx.index,
            model = cx.model,
            multiline = multiline,
            placeholder = owner.str("placeholder"),
            textStyle = cx.composeTextStyle(),
            placeholderColor = placeholder.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.5f),
            disabled = cx.model.isDisabled(cx.index),
            submitsOnReturn = false,
            accessibilityLabel = owner.str("label").ifEmpty { owner.str("placeholder") },
            modifier = Modifier.fillMaxSize(),
            inputType = type,
        )
    }
}

/**
 * #EXP-1629: a `row` Input's `field` IS a Group row (the recipe's height,
 * padding and gap; no chrome of its own): the `label` part leading and never
 * shrinking, then the owned text field filling the rest, its value
 * end-aligned at the field colour × 0.7 (the placeholder keeps its own,
 * unfaded). The label stays the field's accessibility label.
 */
@Composable
private fun InputRowFieldLeaf(cx: LeafContext, placeholderColor: Color?) {
    val owner = cx.ownerProps
    val label = inputRowLabel(cx.props, owner)
    val labelPart = cx.part("Input", "label")
    val type = owner.str("type")
    InnerBox(cx) {
        Row(
            Modifier.fillMaxSize(),
            horizontalArrangement = Arrangement.spacedBy(inputRowGap(cx.style.gap, cx.spacing("md")).dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // The field names itself with the label: the painted line stays out of the tree (no double read).
            if (label.isNotEmpty()) LeafLine(cx, label, Modifier.clearAndSetSemantics {}, color = labelPart.color ?: cx.ink, ts = inputRowLabelStyle(cx.textStyle, labelPart))
            OwnedTextField(
                index = cx.index,
                model = cx.model,
                multiline = false,
                placeholder = owner.str("placeholder"),
                textStyle = cx.composeTextStyle(color = inputRowValueInk(cx.ink)),
                placeholderColor = placeholderColor ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.5f),
                disabled = cx.model.isDisabled(cx.index),
                submitsOnReturn = false,
                accessibilityLabel = label.ifEmpty { owner.str("placeholder") },
                modifier = Modifier.weight(1f).fillMaxHeight(),
                inputType = type,
                endAligned = true,
            )
        }
    }
}

/** #EXP-1629: is this Input a `row` (the field's props, else the owner's)? */
internal fun inputRowVariant(props: Props, ownerProps: Props = emptyMap()): Boolean =
    props.str("variant").ifEmpty { ownerProps.str("variant") } == "row"

/** #EXP-1629: a `row` field's leading label (the core's on the field, else the owner's `label`). */
internal fun inputRowLabel(props: Props, ownerProps: Props = emptyMap()): String =
    props.str("label").ifEmpty { ownerProps.str("label") }

/** #EXP-1629: the row label's text style: the `label` part's size, line height and weight (regular), the field's family otherwise. */
internal fun inputRowLabelStyle(ts: ResolvedTextStyle, label: at.exponential.ui.theme.PartStyle): ResolvedTextStyle =
    ResolvedTextStyle(label.px("fontSize") ?: 12f, label.style.fontWeight ?: 400, label.px("lineHeight") ?: 16f, label.fontFamily ?: ts.fontFamily)

/** #EXP-1629: the gap between a row's label and its value (the recipe's, else `$spacing.md`; the measurer's rule). */
internal fun inputRowGap(recipeGap: Float, md: Float): Float = if (recipeGap > 0f) recipeGap else md

/** #EXP-1629: a row's typed text: the field colour at 70 % (the Group row's value tone). */
internal fun inputRowValueInk(ink: Color): Color = ink.copy(alpha = ink.alpha * INPUT_ROW_VALUE_OPACITY)

/** #EXP-1629: the opacity of a `row` Input's typed text. */
internal const val INPUT_ROW_VALUE_OPACITY = 0.7f

/**
 * A one-line inline field of a control (round 1): a NumberField `.input`
 * (number keyboard; the core parses, clamps on commit and formats its
 * `text`), a ChipInput `.input` (a commit or a trailing comma adds the
 * chip) or a searchable Select's `.search`. The owned text field in the
 * part's content box; the owner's label names it.
 */
@Composable
internal fun InlineFieldLeaf(cx: LeafContext) {
    val owner = cx.ownerProps
    val placeholder = cx.part(cx.node.recipeComponent, "placeholder")
    // A `search` part leads with its glyph (the measurer's 16 + 8).
    val glyph = cx.node.part == "search" && cx.props["icon"] != null
    // #EXP-1765: a Select search's glyph is its `searchIcon` part's size, the recipe's gap from the text (else 8).
    val select = cx.node.ownerComponent == "Select"
    val glyphSize = if (select) cx.part("Select", "searchIcon").width ?: 16f else 16f
    val glyphGap = if (select && cx.style.gap > 0f) cx.style.gap else 8f
    @Suppress("UNUSED_VARIABLE") val gen = cx.model.mirrorGeneration
    val clearable = cx.model.selectSearchClearable(cx.index)
    val clearBox = glyphSize * SELECT_CLEAR_BOX
    val disabled = cx.model.isDisabled(cx.index)
    InnerBox(cx, contentAlignment = Alignment.CenterStart) {
        if (glyph) GlyphView(Glyph.Search, glyphSize, placeholder.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.6f))
        OwnedTextField(
            index = cx.index,
            model = cx.model,
            multiline = false,
            placeholder = cx.props.str("placeholder").ifEmpty { owner.str("placeholder") },
            textStyle = cx.composeTextStyle(),
            placeholderColor = placeholder.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.5f),
            disabled = disabled,
            submitsOnReturn = false,
            accessibilityLabel = owner.str("label").ifEmpty { owner.str("placeholder") },
            modifier = (if (glyph) Modifier.fillMaxSize().padding(start = (glyphSize + glyphGap).dp) else Modifier.fillMaxSize())
                .padding(end = if (clearable) (clearBox + glyphGap).dp else 0.dp),
            keyboardType = if (cx.node.component == "NumberField") KeyboardType.Decimal else KeyboardType.Text,
        )
        if (clearable) SelectSearchClearButton(cx, glyphSize, Modifier.align(Alignment.CenterEnd)) { cx.model.selectSearchClear(cx.index) }
    }
}

/**
 * #EXP-1765: a `pill` Select search's clear button (while it holds text):
 * the `Select.clear` glyph 0.75 × the `searchIcon` size in
 * `mutedForeground`, centred in a square box 1.25 × the `searchIcon` size;
 * "Clear search", a button.
 */
@Composable
internal fun SelectSearchClearButton(cx: LeafContext, glyphSize: Float, modifier: Modifier = Modifier, onClear: () -> Unit) {
    val label = cx.string("clearSearch")
    Box(
        modifier
            .size((glyphSize * SELECT_CLEAR_BOX).dp)
            .clickable(onClick = onClear)
            .clearAndSetSemantics {
                contentDescription = label
                role = Role.Button
                onClick {
                    onClear()
                    true
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        IconOrGlyph(builtinIcon("Select.clear", "ui-clear"), Glyph.Close, glyphSize * SELECT_CLEAR_GLYPH, cx, cx.themeColor("mutedForeground") ?: cx.ink)
    }
}

/** #EXP-1765: the clear button's box and its glyph, each over the `searchIcon` size (14 → 17.5 box, 10.5 glyph). */
internal const val SELECT_CLEAR_BOX = 1.25f
internal const val SELECT_CLEAR_GLYPH = 0.75f

/**
 * The Composer: the owned multi-line field (the `Composer/field` font)
 * over the send row: attachment chips (`Composer/attachment`, a paperclip
 * + name) and the round send button (`Composer/send`; `busy` = stop), which
 * submits through `composerSubmit`.
 */
@Composable
internal fun ComposerLeaf(cx: LeafContext) {
    val model = cx.model
    val index = cx.index
    val field = cx.part("Composer", "field")
    val send = cx.part("Composer", "send")
    // #EDE-65: the send/stop glyph's size is the `sendIcon` recipe part.
    val sendIcon = cx.part("Composer", "sendIcon").width ?: 16f
    val placeholder = cx.part("Composer", "placeholder")
    val chip = cx.part("Composer", "attachment")
    val sendSize = send.height ?: cx.control("buttonIcon", 36f)
    val gap = cx.style.gap
    val busy = cx.props.flag("busy")
    val fieldTs = ResolvedTextStyle(
        field.px("fontSize") ?: cx.textStyle.fontSize,
        cx.textStyle.fontWeight,
        field.px("lineHeight") ?: cx.textStyle.lineHeight,
        field.fontFamily ?: cx.textStyle.fontFamily,
    )
    val prompt = cx.props.str("placeholder")
    val name = cx.props["accessibility"]?.get("label")?.string ?: cx.node.accessibility?.get("label")?.string ?: cx.string("message")
    val sendLabel = if (busy) cx.string("stop") else cx.props.str("submitLabel").ifEmpty { cx.string("send") }
    val chipInk = chip.color ?: cx.ink
    val w = cx.inner.width
    val h = cx.inner.height
    // #EXP-1762: `flat` = one row: the field between the leading slot and the
    // controls, spanning the row (the disc's height while one line, growing
    // upward: the controls stay at the bottom); its text starts the field
    // recipe's paddingTop below the row's top, one line or grown
    // (composerSlots `field`; the text field has no inset of its own).
    val flat = cx.props.str("layout") == "flat"
    // #EXP-1762: the core lays an `above` slot out at the content top; the
    // field (and a flat row) start one gap below it.
    val top = model.indexOf("${cx.node.id}.above")?.let { model.frame(it).height + gap } ?: 0f
    val rh = maxOf(h - top, 0f)
    val flatBox = flatComposerField(rh, field.px("paddingTop") ?: 0f)
    val slotW = { slot: String -> model.indexOf("${cx.node.id}.$slot")?.let { model.frame(it).width + gap } ?: 0f }
    val lead = if (flat) slotW("leading") else 0f
    val fieldW = if (flat) maxOf(w - lead - slotW("trailing") - (if (cx.props.flag("attachments")) COMPOSER_ATTACH_W + gap else 0f) - sendSize - gap, 0f) else w
    val fieldH = if (!flat) maxOf(rh - sendSize - gap, 0f) else flatBox.second
    val fieldX = if (!flat) 0f else if (cx.rtl) w - lead - fieldW else lead
    val fieldY = top + if (flat) flatBox.first else 0f
    InnerBox(cx) {
            OwnedTextField(
                index = index,
                model = model,
                multiline = true,
                placeholder = prompt,
                textStyle = cx.composeTextStyle(ts = fieldTs, color = field.color ?: cx.ink),
                placeholderColor = placeholder.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.5f),
                disabled = model.isDisabled(index),
                submitsOnReturn = true,
                accessibilityLabel = name,
                modifier = Modifier.absoluteOffset(fieldX.dp, fieldY.dp).size(fieldW.dp, fieldH.dp),
            )
            Row(Modifier.align(Alignment.BottomStart).fillMaxWidth().height(sendSize.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                for (a in cx.props.list("attachments")) {
                    Row(
                        Modifier
                            .height((chip.height ?: 24f).dp)
                            .background(chip.style.background ?: cx.ink.copy(alpha = 0.08f), CircleShape)
                            .padding(horizontal = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        GlyphView(Glyph.Attach, 12f, chipInk)
                        LeafLine(cx, a["name"]?.displayText ?: a.displayText, color = chipInk, ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily))
                    }
                }
                Spacer(Modifier.weight(1f))
                // `attachments: true` = the attach control (catalog: "Shows the
                // attach control"; web's `Composer/attachment` button beside
                // send), firing `attach`: the files are the host's business.
                if (cx.props.flag("attachments")) {
                    val browse = cx.string("browse")
                    val attH = minOf(sendSize, chip.height ?: 28f)
                    Box(
                        Modifier
                            .height(attH.dp)
                            .background(chip.style.background ?: Color.Transparent, RoundedCornerShape(chip.style.radius.dp))
                            .clickable(role = Role.Button, onClickLabel = browse) { model.fire(index, "attach") }
                            .semantics { contentDescription = browse }
                            .padding(horizontal = chip.style.paddingHorizontal.takeIf { it > 0f }?.dp ?: 8.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        IconOrGlyph("ui-attach", Glyph.Attach, 16f, cx, chipInk)
                    }
                }
                // #EDE-65 R: the core lays the `trailing` slot out one `xs`
                // gap before the send disc (it paints over this leaf); the
                // attach control ends one `xs` gap before it (composerSlots).
                model.indexOf("${cx.node.id}.trailing")?.let { t ->
                    val xs = cx.spacing("xs")
                    Spacer(Modifier.width(maxOf(0f, model.frame(t).width + 2 * xs - 16f).dp))
                }
                // Web: an empty draft disables send (the `send` part's `state:
                // disabled` opacity, 0.5 without one); `busy` dims the stop
                // through the recipe's own opacity.
                val empty = !busy && (model.field(index)?.text ?: "").isBlank()
                val sendAlpha = if (empty) cx.part("Composer", "send", listOf("disabled")).style.opacity ?: 0.5f else send.style.opacity ?: 1f
                Box(
                    Modifier
                        .graphicsLayer { alpha = sendAlpha }
                        .size((send.width ?: sendSize).dp, sendSize.dp)
                        .background(send.style.background ?: cx.themeColor("primary") ?: cx.ink, if (send.style.radius > 0f) RoundedCornerShape(send.style.radius.dp) else CircleShape)
                        .clickable(role = Role.Button, onClickLabel = sendLabel) { model.composerSubmit(index) }
                        .semantics { contentDescription = sendLabel },
                    contentAlignment = Alignment.Center,
                ) {
                    val sendInk = send.color ?: cx.themeColor("primaryForeground") ?: Color.White
                    if (busy) IconOrGlyph(builtinIcon("Composer.stop", "ui-stop"), Glyph.Stop, sendIcon, cx, sendInk) else IconOrGlyph(builtinIcon("Composer.send", "ui-submit"), Glyph.Submit, sendIcon, cx, sendInk)
                }
            }
    }
}

/**
 * #EXP-1762 (interactions-round1.json composerSlots `field`): a flat
 * Composer's text field `(top, height)` in a row `row` tall: its text starts
 * `padTop` (the field recipe's `paddingTop`) below the row's top, one line or
 * grown, and it reaches the row's bottom.
 */
internal fun flatComposerField(row: Float, padTop: Float): Pair<Float, Float> = padTop to maxOf(row - padTop, 0f)

/** #EXP-1762: the attach control's width (its 16 dp glyph + 8 dp either side), what a flat Composer's field leaves for it. */
internal const val COMPOSER_ATTACH_W = 32f

/** #EDE-65: a Composer menu key's name (`composerMenuKey`: SearchList's DOM-style names), null for the rest. */
internal fun menuKeyName(key: Key): String? = when (key) {
    Key.DirectionUp -> "ArrowUp"
    Key.DirectionDown -> "ArrowDown"
    Key.Enter, Key.NumPadEnter -> "Enter"
    Key.Tab -> "Tab"
    Key.Escape -> "Escape"
    else -> null
}
