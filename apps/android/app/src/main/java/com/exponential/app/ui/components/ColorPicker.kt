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
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
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
                ColorSwatchGrid(
                    selected = selected,
                    onSelect = {
                        onSelect(it)
                        open = false
                    },
                    colors = colors,
                    modifier = Modifier.padding(bottom = 12.dp),
                )
            }
        }
    }
}

/**
 * #EXP-1774: the palette grid, web `ui/color-swatch-grid.tsx` cell for cell
 * (iOS `ColorSwatchGrid`): a 28dp cell with a 2dp ring (foreground when
 * picked, clear otherwise) around a 20dp dot, wrapping with a 6dp gap. The
 * ring sits INSIDE the cell, never a halo past it.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ColorSwatchGrid(
    selected: String,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    colors: List<String> = LabelPalette.colors,
) {
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
        modifier = modifier,
    ) {
        colors.forEach { swatch ->
            val picked = swatch.equals(selected, ignoreCase = true)
            Box(
                modifier = Modifier
                    .size(28.dp)
                    .clip(CircleShape)
                    .border(
                        2.dp,
                        if (picked) MaterialTheme.colorScheme.onSurface else Color.Transparent,
                        CircleShape,
                    )
                    .clickable { onSelect(swatch) }
                    .semantics {
                        contentDescription = swatch
                        this.selected = picked
                    },
                contentAlignment = Alignment.Center,
            ) {
                Box(modifier = Modifier.size(20.dp).background(parseColor(swatch), CircleShape))
            }
        }
    }
}

/** The ONE rounded-square trigger size the icon and colour pickers share, and
 *  the height a form row's field matches (web `h-9`). */
internal val PICKER_TRIGGER_SIZE = 36.dp
