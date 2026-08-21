package com.exponential.app.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldColors
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The ONE text input (EXP-576) — a 1:1 port of the iOS glass fields
 * (`LoginView.glassTextField` / `GlassSheetSearchField`): a plain field on a
 * faint white fill with a hairline stroke and 12dp corners, no Material
 * outline, underline or floating label. iOS describes every field with a
 * placeholder, so this takes a plain [placeholder] string instead of a `label`
 * slot. [containerColor] lets a caller tint the fill (the helpdesk internal-note
 * amber); the stroke stays the glass hairline and brightens on focus.
 */
@Composable
fun GlassTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    trailingIcon: @Composable (() -> Unit)? = null,
    enabled: Boolean = true,
    singleLine: Boolean = false,
    minLines: Int = 1,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    textStyle: TextStyle = LocalTextStyle.current,
    containerColor: Color = GlassTokens.CardFill,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val focused by interactionSource.collectIsFocusedAsState()
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.glassFieldBorder(focused),
        enabled = enabled,
        textStyle = textStyle,
        placeholder = placeholder?.let { { GlassPlaceholder(it) } },
        leadingIcon = leadingIcon,
        trailingIcon = trailingIcon,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        singleLine = singleLine,
        minLines = minLines,
        maxLines = maxLines,
        interactionSource = interactionSource,
        shape = GlassFieldShape,
        colors = glassTextFieldColors(containerColor),
    )
}

/** [TextFieldValue] twin for callers that track selection (the instance URL field). */
@Composable
fun GlassTextField(
    value: TextFieldValue,
    onValueChange: (TextFieldValue) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    leadingIcon: @Composable (() -> Unit)? = null,
    trailingIcon: @Composable (() -> Unit)? = null,
    enabled: Boolean = true,
    singleLine: Boolean = false,
    minLines: Int = 1,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    textStyle: TextStyle = LocalTextStyle.current,
    containerColor: Color = GlassTokens.CardFill,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val focused by interactionSource.collectIsFocusedAsState()
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.glassFieldBorder(focused),
        enabled = enabled,
        textStyle = textStyle,
        placeholder = placeholder?.let { { GlassPlaceholder(it) } },
        leadingIcon = leadingIcon,
        trailingIcon = trailingIcon,
        visualTransformation = visualTransformation,
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        singleLine = singleLine,
        minLines = minLines,
        maxLines = maxLines,
        interactionSource = interactionSource,
        shape = GlassFieldShape,
        colors = glassTextFieldColors(containerColor),
    )
}

/** iOS field corner (GlassSheetSearchField / SearchView: 12). */
val GlassFieldShape = RoundedCornerShape(12.dp)

@Composable
private fun GlassPlaceholder(text: String) {
    Text(text, color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary))
}

private fun Modifier.glassFieldBorder(focused: Boolean): Modifier = border(
    GlassTokens.Hairline,
    if (focused) GlassTokens.StrokeActive else GlassTokens.StrokeCard,
    GlassFieldShape,
)

/**
 * Glass fill with every Material indicator line switched off — also used by
 * the fields that live INSIDE a glass group (search rows of grouped option
 * cards, the composer pill), which pass [Color.Transparent] and take the
 * container's chrome instead of their own.
 */
@Composable
fun glassTextFieldColors(containerColor: Color = GlassTokens.CardFill): TextFieldColors =
    TextFieldDefaults.colors(
        focusedContainerColor = containerColor,
        unfocusedContainerColor = containerColor,
        disabledContainerColor = containerColor,
        focusedIndicatorColor = Color.Transparent,
        unfocusedIndicatorColor = Color.Transparent,
        disabledIndicatorColor = Color.Transparent,
        errorIndicatorColor = Color.Transparent,
        cursorColor = MaterialTheme.colorScheme.onSurface,
    )
