package com.exponential.app.ui.components

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
import at.exponential.ui.primitives.fieldChrome
import com.exponential.app.ui.theme.AppPrimitiveTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The ONE text input (EXP-576) — a 1:1 port of the iOS glass fields
 * (`LoginView.glassTextField` / `GlassSheetSearchField`): a plain field on a
 * faint white fill with a hairline stroke and 12dp corners, no Material
 * outline, underline or floating label. iOS describes every field with a
 * placeholder, so this takes a plain [placeholder] string instead of a `label`
 * slot. [containerColor] lets a caller tint the fill (an amber note field, say);
 * the stroke stays the glass hairline and brightens on focus.
 *
 * EXP-694: [bordered] = false drops the fill AND the hairline for a field that
 * lives INSIDE a grouped card (`OptionGroup`) — the description / prompt
 * editors, which used to float outside the card stack as their own chromed
 * boxes. The group owns the chrome then, and the field is just its text.
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
    bordered: Boolean = true,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val focused by interactionSource.collectIsFocusedAsState()
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = if (bordered) modifier.glassFieldBorder(focused) else modifier,
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
        colors = glassTextFieldColors(if (bordered) containerColor else Color.Transparent),
    )
}

/** [TextFieldValue] twin for callers that track selection (the instance URL
 *  field; the steer composer, which inserts `[Image #N]` at the caret). */
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
    bordered: Boolean = true,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val focused by interactionSource.collectIsFocusedAsState()
    TextField(
        value = value,
        onValueChange = onValueChange,
        modifier = if (bordered) modifier.glassFieldBorder(focused) else modifier,
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
        colors = glassTextFieldColors(if (bordered) containerColor else Color.Transparent),
    )
}

/** iOS field corner (GlassSheetSearchField / SearchView: 12). */
private val GlassFieldRadius = 12.dp

/** iOS field corner, as a shape. */
val GlassFieldShape = RoundedCornerShape(GlassFieldRadius)

@Composable
private fun GlassPlaceholder(text: String) {
    Text(text, color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary))
}

/**
 * The field chrome is the SDK's `fieldChrome` (SLOP-18 / VAPP-89): the glass
 * hairline brightening on focus, at the 12dp field corner. The fill stays the
 * M3 container colour (so [GlassTextField]'s `containerColor` keeps tinting
 * it), hence a transparent chrome fill.
 */
private fun Modifier.glassFieldBorder(focused: Boolean): Modifier = this.fieldChrome(
    AppPrimitiveTokens,
    focused = focused,
    radius = GlassFieldRadius,
    fill = Color.Transparent,
    stroke = GlassTokens.StrokeCard,
    focusedStroke = GlassTokens.StrokeActive,
    strokeWidth = GlassTokens.Hairline,
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
