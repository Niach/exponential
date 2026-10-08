package at.exponential.ui.compose

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.composerSubmit
import at.exponential.ui.model.field
import at.exponential.ui.model.fieldCommitted
import at.exponential.ui.model.fieldEdited
import at.exponential.ui.model.fieldFocused
import at.exponential.ui.model.fieldSubmitted
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
    secure: Boolean = false,
    keyboardType: KeyboardType = KeyboardType.Text,
) {
    val state = model.field(index)
    val id = model.node(index)?.id ?: "#$index"
    var value by remember(id) {
        val t = state?.text ?: ""
        mutableStateOf(TextFieldValue(t, TextRange(t.length)))
    }
    var focused by remember(id) { mutableStateOf(false) }
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
    BasicTextField(
        value = value,
        onValueChange = { next ->
            val prev = value
            if (submitsOnReturn && next.text.length == prev.text.length + 1 && next.text.count { it == '\n' } > prev.text.count { it == '\n' }) {
                // Return in a composer submits instead of inserting a newline.
                model.composerSubmit(index)
                return@BasicTextField
            }
            value = next
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
                    submitsOnReturn && e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter) && !e.isShiftPressed -> {
                        model.composerSubmit(index)
                        true
                    }
                    // A hardware Enter in a single-line field submits (the IME's Done does too).
                    !multiline && !submitsOnReturn && e.type == KeyEventType.KeyDown && (e.key == Key.Enter || e.key == Key.NumPadEnter) -> {
                        model.fieldSubmitted(index)
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
            .semantics { contentDescription = accessibilityLabel },
        // `visibility: hidden` (an ancestor's too) takes no focus and no taps.
        enabled = !disabled && !LocalInvisible.current,
        textStyle = textStyle,
        singleLine = !multiline,
        maxLines = if (multiline) Int.MAX_VALUE else 1,
        keyboardOptions = KeyboardOptions(
            keyboardType = if (secure) KeyboardType.Password else keyboardType,
            imeAction = imeAction,
            autoCorrectEnabled = !secure,
        ),
        keyboardActions = KeyboardActions(
            onDone = {
                // Enter in a single-line field submits (a Form, a ChipInput's chip).
                if (!multiline) model.fieldSubmitted(index) else model.fieldCommitted(index)
                if (model.node(index)?.component != "ChipInput") focusManager.clearFocus()
            },
            onSend = { model.composerSubmit(index) },
        ),
        visualTransformation = if (secure) PasswordVisualTransformation() else VisualTransformation.None,
        cursorBrush = SolidColor(textStyle.color.takeIf { it != Color.Unspecified } ?: Color.Black),
        decorationBox = { inner ->
            Box(Modifier.fillMaxSize(), contentAlignment = if (multiline) Alignment.TopStart else Alignment.CenterStart) {
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
            secure = type == "password",
            keyboardType = keyboardTypeOf(type),
        )
    }
}

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
    InnerBox(cx, contentAlignment = Alignment.CenterStart) {
        OwnedTextField(
            index = cx.index,
            model = cx.model,
            multiline = false,
            placeholder = cx.props.str("placeholder").ifEmpty { owner.str("placeholder") },
            textStyle = cx.composeTextStyle(),
            placeholderColor = placeholder.color ?: cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = cx.ink.alpha * 0.5f),
            disabled = cx.model.isDisabled(cx.index),
            submitsOnReturn = false,
            accessibilityLabel = owner.str("label").ifEmpty { owner.str("placeholder") },
            modifier = Modifier.fillMaxSize(),
            keyboardType = if (cx.node.component == "NumberField") KeyboardType.Decimal else KeyboardType.Text,
        )
    }
}

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
    InnerBox(cx) {
        Column(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(gap.dp)) {
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
                modifier = Modifier.fillMaxWidth().weight(1f),
            )
            Row(Modifier.fillMaxWidth().height(sendSize.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
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
                Box(
                    Modifier
                        .size((send.width ?: sendSize).dp, sendSize.dp)
                        .background(send.style.background ?: cx.themeColor("primary") ?: cx.ink, CircleShape)
                        .clickable(role = Role.Button, onClickLabel = sendLabel) { model.composerSubmit(index) }
                        .semantics { contentDescription = sendLabel },
                    contentAlignment = Alignment.Center,
                ) {
                    GlyphView(if (busy) Glyph.Stop else Glyph.Send, 18f, send.color ?: cx.themeColor("primaryForeground") ?: Color.White, weight = GlyphWeight.Semibold)
                }
            }
        }
    }
}
