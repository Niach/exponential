package com.exponential.app.ui.components.picker

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier

/**
 * N-14 — the ROW host's inline slot (`picker-presentation.json`): a row of
 * picker triggers (THE composer options row) offers ONE slot UNDER the whole
 * row, as wide as the row. An inline picker inside it publishes its rows here
 * instead of next to its trigger, so the trigger and its siblings never move.
 * One slot shows ONE picker's rows: a second [claim] replaces the first, and
 * the first picker, seeing it no longer owns the slot, folds itself.
 */
class InlinePickerSlot {
    /** The picker whose rows the slot shows; null = folded. */
    var owner: Any? by mutableStateOf(null)
        private set

    /** The rows of [owner]; rendered by [InlinePickerRow] under the row. */
    var content: (@Composable () -> Unit)? by mutableStateOf(null)
        private set

    /** Shows [key]'s rows, replacing whichever picker had the slot. */
    fun claim(key: Any, rows: @Composable () -> Unit) {
        owner = key
        content = rows
    }

    /** Folds the slot, but only for the picker that still owns it. */
    fun release(key: Any) {
        if (owner === key) {
            owner = null
            content = null
        }
    }

    fun isShowing(key: Any): Boolean = owner === key
}

/** Null outside a row host: an inline picker then discloses under its trigger. */
val LocalInlinePickerSlot = staticCompositionLocalOf<InlinePickerSlot?> { null }

/**
 * N-14 — a horizontally scrolling row of picker triggers with the row slot
 * under it. Collapsed it is exactly the row it replaces (the slot takes no
 * height); open, the one picker's rows sit under the WHOLE row at its width.
 * Outside a sheet the pickers keep their own sheets (`placementCases`), so the
 * slot simply stays empty.
 */
@Composable
fun InlinePickerRow(
    modifier: Modifier = Modifier,
    rowModifier: Modifier = Modifier,
    horizontalArrangement: Arrangement.Horizontal = Arrangement.Start,
    verticalAlignment: Alignment.Vertical = Alignment.Top,
    /** VAPP-110: false = the row fits its width instead of scrolling (a form
     *  row whose field takes the rest with `weight`, like the identity row). */
    scrollable: Boolean = true,
    content: @Composable RowScope.() -> Unit,
) {
    val slot = remember { InlinePickerSlot() }
    Column(modifier = modifier.fillMaxWidth()) {
        CompositionLocalProvider(LocalInlinePickerSlot provides slot) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .then(if (scrollable) Modifier.horizontalScroll(rememberScrollState()) else Modifier)
                    .then(rowModifier),
                horizontalArrangement = horizontalArrangement,
                verticalAlignment = verticalAlignment,
                content = content,
            )
        }
        val rows = slot.content
        if (rows != null) {
            Box(Modifier.fillMaxWidth()) { rows() }
        }
    }
}
