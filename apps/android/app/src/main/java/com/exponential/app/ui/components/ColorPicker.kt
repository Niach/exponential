package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.parseColor
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LabelPalette
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-862: THE colour picker — the twin of [IconPicker], down to the trigger:
 * one rounded-square swatch at the same size that opens the palette in a
 * [GlassSheet], because a board form asks for an icon and a colour in the same
 * breath and the two must read as one pair of controls (web
 * `ui/color-picker.tsx`, desktop `board_form::color_picker`, iOS the same).
 *
 * EXP-771, the shape rule: a rounded square is a PICKER, a circle is an
 * ACTION. The swatches inside stay circles — they are values, not controls.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ColorPicker(
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    colors: List<String> = LabelPalette.colors,
) {
    var open by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    val borderColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
    Box(
        modifier = modifier
            .size(PICKER_TRIGGER_SIZE)
            .border(1.dp, borderColor, shape)
            .clickable { open = true }
            .semantics { contentDescription = "Color: $selected" },
        contentAlignment = Alignment.Center,
    ) {
        Box(modifier = Modifier.size(20.dp).background(parseColor(selected), CircleShape))
    }
    if (open) {
        GlassSheet(title = "Color", onDismiss = { open = false }) {
            Column(modifier = Modifier.padding(horizontal = 20.dp)) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    modifier = Modifier.padding(bottom = 12.dp),
                ) {
                    colors.forEach { swatch ->
                        val picked = swatch.equals(selected, ignoreCase = true)
                        Box(
                            modifier = Modifier
                                .size(36.dp)
                                .background(parseColor(swatch), CircleShape)
                                .then(
                                    if (picked) {
                                        Modifier.border(
                                            2.dp,
                                            MaterialTheme.colorScheme.onSurface,
                                            CircleShape,
                                        )
                                    } else {
                                        Modifier
                                    },
                                )
                                .clickable {
                                    onSelect(swatch)
                                    open = false
                                }
                                .semantics { contentDescription = swatch },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (picked) {
                                Icon(
                                    ExpIcons.uiCheck,
                                    contentDescription = null,
                                    tint = Color.White,
                                    modifier = Modifier.size(18.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** The ONE rounded-square trigger size the icon and colour pickers share, and
 *  the height a form row's field matches (web `h-9`). */
internal val PICKER_TRIGGER_SIZE = 36.dp
