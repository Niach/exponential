package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector

/**
 * EXP-1029 contract — THE picker primitive on Android (EXP-1021 implements
 * it). One primitive per platform, typed pickers on top, the same names
 * everywhere: web `packages/ui/src/picker`, IDE `ui::picker`, iOS
 * `ExpUI/Sources/Picker` (`GlassPicker`).
 *
 * Presentation belongs to the primitive, never to the caller: on the phone
 * every picker is a bottom sheet ([com.exponential.app.ui.components.GlassSheet])
 * of PLAIN rows — no cards inside the sheet; multi-select marks rows by the
 * highlight colour, no circles; swipe down closes. [search] adds the filter
 * field at the top. The trigger is whatever chip or button the caller hands
 * in; the primitive owns the sheet.
 *
 * This file is the CONTRACT: the item and mode types and a stub composable
 * that renders the trigger only. `PickerContractTest` carries the
 * presentation rules and the typed-picker gate, ignored until EXP-1021.
 */

/** One row of a picker. */
data class PickerItem<T>(
    /** The stable identity of the row; also what search matches on. */
    val value: T,
    /** What the row reads as; also the default search keyword. */
    val label: String,
    /** A leading glyph — an `AppIcons` vector, never a raw material icon. */
    val icon: ImageVector? = null,
    /** A colour for the glyph (a board's hex, a label's dot, a status tone). */
    val color: Color? = null,
    /** A muted second line or trailing note (an email, a branch age). */
    val description: String? = null,
    /** Rendered, never pickable. */
    val disabled: Boolean = false,
    /** Extra search terms (an identifier, an email). */
    val keywords: List<String> = emptyList(),
) {
    /** The keywords the row matches on: the explicit ones, else its label. */
    val searchKeywords: List<String> get() = keywords.ifEmpty { listOf(label) }
}

enum class PickerMode {
    /** Closes on a pick; `onChange` gets the one value. */
    Single,
    /** Toggles without closing; `onChange` gets the whole new set. */
    Multi,
}

/**
 * THE picker. Contract stub: renders [trigger] only; the sheet comes with
 * EXP-1021. [trigger] receives the `open` callback it wires to its click.
 */
@Composable
fun <T> Picker(
    items: List<PickerItem<T>>,
    mode: PickerMode,
    /** The current selection (at most one value in [PickerMode.Single]). */
    value: Set<T>,
    /** The whole new selection (one value in single mode). */
    onChange: (Set<T>) -> Unit,
    search: Boolean = false,
    emptyText: String? = null,
    /** The sheet's title. */
    title: String? = null,
    enabled: Boolean = true,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    trigger {}
}
