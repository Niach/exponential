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
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The shared curated-icon picker (EXP-273): the registry's full `pickable` set
 * as a wrapping grid of bordered swatches, the Android twin of web's
 * `IconSwatchGrid` and desktop's `board_form::icon_swatch_grid`. Used by the
 * create-board form and by `icon`-typed action inputs in the Start-coding
 * sheet, so both surfaces stay byte-identical in what they offer.
 *
 * The set is deliberately UNFILTERED: 60 glyphs scan faster than they search
 * (EXP-390 dropped the query field on every platform). [selected] naming no
 * pickable glyph (null, blank, or a name this build's registry doesn't carry)
 * simply highlights nothing — that is the "no icon" state for optional inputs.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun IconSwatchGrid(
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    /** Tint + border of the selected swatch (the board form passes its color). */
    accentColor: Color = MaterialTheme.colorScheme.primary,
) {
    val picked = pickableIconName(selected)
    FlowRow(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ExpIcons.pickable.forEach { glyphName ->
            val glyph = ExpIcons.byName(glyphName) ?: return@forEach
            val isSelected = glyphName == picked
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .border(
                        if (isSelected) 2.dp else 1.dp,
                        if (isSelected) accentColor
                        else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary),
                        RoundedCornerShape(10.dp),
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
 * names outside this build's `pickable` set both read as "no icon" rather than
 * as a phantom selection. Picking always writes back a `pickable` NAME, which
 * is what the server stores and what the other clients expect.
 */
fun pickableIconName(value: String?): String? =
    value?.takeIf { it.isNotBlank() && it in ExpIcons.pickable }
