package com.exponential.app.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The shared curated-icon picker (EXP-273): a curated registry set as a
 * wrapping grid of bordered swatches, the Android twin of web's
 * `IconSwatchGrid` and desktop's `board_form::icon_swatch_grid`. Used by the
 * create-board form and by `icon`-typed action inputs in the Start-coding
 * sheet, so both surfaces stay byte-identical in what they offer.
 *
 * [pickable] is WHICH set is offered — the board/action one (`ExpIcons.pickable`,
 * 96 glyphs) by default, the device one (`ExpIcons.devicePickable`, 6) for the
 * device-settings sheet (EXP-924). The set is deliberately UNFILTERED: curated
 * glyphs scan faster than they search (EXP-390 dropped the query field on every
 * platform). [selected] naming no glyph of that set (null, blank, a name this
 * build's registry doesn't carry, or one pickable in the OTHER set) simply
 * highlights nothing — that is the "no icon" state for optional inputs.
 *
 * A cell is a rounded square at the radius ladder's MD step
 * ([GlassTokens.RowRadius]), the same corner the [IconPicker] trigger wears:
 * a circle is an action, a rounded square is a picker (EXP-771). Only COLOUR
 * swatches stay circles.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IconSwatchGrid(
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    /** Tint + border of the selected swatch (the board form passes its color). */
    accentColor: Color = MaterialTheme.colorScheme.primary,
    pickable: List<String> = ExpIcons.pickable,
) {
    val picked = pickableIconName(selected, pickable)
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        pickable.forEach { glyphName ->
            val glyph = ExpIcons.byName(glyphName) ?: return@forEach
            val isSelected = glyphName == picked
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .border(
                        if (isSelected) 2.dp else 1.dp,
                        if (isSelected) accentColor
                        else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                        RoundedCornerShape(GlassTokens.RowRadius),
                    )
                    .clickable { onSelect(glyphName) },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    glyph,
                    contentDescription = glyphName,
                    tint = if (isSelected) accentColor
                    else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    modifier = Modifier.size(20.dp),
                )
            }
        }
    }
}

/**
 * The pickable glyph a stored value names, or null when it names none — blank
 * (an optional `icon` action input that was never filled, or was cleared) and
 * names outside the offered [pickable] set both read as "no icon" rather than
 * as a phantom selection. Picking always writes back a NAME from that set,
 * which is what the server stores and what the other clients expect.
 */
fun pickableIconName(value: String?, pickable: List<String> = ExpIcons.pickable): String? =
    value?.takeIf { it.isNotBlank() && it in pickable }
