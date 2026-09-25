package com.exponential.app.ui.components

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerActionRow
import com.exponential.app.ui.components.picker.PickerMode
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * EXP-575: THE icon picker — one slim 36dp swatch showing the current pick
 * that opens the curated grid ([IconSwatchGrid]) in a bottom sheet, so the
 * 96-glyph grid never sits inline in a form. EXP-1021 re-homed it onto the
 * shared [Picker]: the sheet is the primitive's, the grid is its panel. Every surface that picks an icon
 * (create-board form, Start-coding `icon` inputs, the device-settings sheet)
 * renders this.
 *
 * [pickable] is WHICH curated set the sheet offers — the board/action one by
 * default, `ExpIcons.devicePickable` for a device (EXP-924).
 *
 * EXP-771, the shape rule: a circle is an ACTION and a rounded square is a
 * PICKER, so this trigger and the grid's cells are rounded squares at the
 * radius ladder's MD step ([GlassTokens.RowRadius], 10dp) while every icon-only
 * action button stays a circle. Web (`rounded-md`), desktop (the theme radius)
 * and iOS (`GlassTokens.rowRadius`) draw the same corner.
 *
 * [selected] is a registry NAME; anything [pickableIconName] rejects reads as
 * "no icon" and draws a dashed placeholder. [allowsNone] hosts (optional action
 * inputs) get a "No icon" reset in the sheet, reported as `""`.
 */
@Composable
fun IconPicker(
    selected: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
    allowsNone: Boolean = false,
    /** Tint of the picked glyph (the board form passes its color). */
    accentColor: Color = MaterialTheme.colorScheme.primary,
    pickable: List<String> = ExpIcons.pickable,
) {
    var open by remember { mutableStateOf(false) }
    val picked = pickableIconName(selected, pickable)
    val glyph = picked?.let { ExpIcons.byName(it) }
    val borderColor = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Quaternary)
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    Box(
        modifier = modifier
            .size(36.dp)
            .then(
                if (glyph != null) Modifier.border(1.dp, borderColor, shape)
                else Modifier.drawBehind {
                    drawRoundRect(
                        color = borderColor,
                        cornerRadius = CornerRadius(GlassTokens.RowRadius.toPx()),
                        style = Stroke(
                            width = 1.dp.toPx(),
                            pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 6f)),
                        ),
                    )
                },
            )
            .clickable { open = true }
            .semantics { contentDescription = picked?.let { "Icon: $it" } ?: "Pick an icon" },
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            glyph ?: ExpIcons.uiIconPlaceholder,
            contentDescription = null,
            tint = if (glyph != null) accentColor
            else MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.size(20.dp),
        )
    }
    if (open) {
        // EXP-1021: the grid rides the shared picker as its PANEL — the sheet,
        // its title, its dismiss and the PICK are the primitive's; the grid
        // reports the name it was tapped on through the panel's `pick` and the
        // picker reports it on, then closes. There are no ROWS to hand it (the
        // panel replaces them), so it gets none: resolving all 96 vectors for a
        // list nothing renders was pure recomposition cost. A GlassSheet never
        // scrolls its own slot, and the board set is taller than the fitted
        // sheet's 85 % cap on every phone, so the scroller stays here.
        Picker(
            items = emptyList(),
            mode = PickerMode.Single,
            value = setOfNotNull(picked),
            onChange = { next -> next.firstOrNull()?.let(onSelect) },
            title = "Icon",
            open = true,
            onOpenChange = { next -> if (!next) open = false },
            footer = if (allowsNone && picked != null) {
                {
                    // "No icon" is a RESET, not an option: it clears the pick
                    // and closes, the way the header action always did — in the
                    // picker's own footer idiom, not another sheet's row.
                    PickerActionRow(
                        label = "No icon",
                        onClick = {
                            onSelect("")
                            open = false
                        },
                    )
                }
            } else {
                null
            },
            panel = { pick ->
                Column(
                    modifier = Modifier
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 20.dp),
                ) {
                    IconSwatchGrid(
                        selected = picked,
                        onSelect = pick,
                        accentColor = accentColor,
                        pickable = pickable,
                        modifier = Modifier.padding(bottom = 12.dp),
                    )
                }
            },
        )
    }
}
