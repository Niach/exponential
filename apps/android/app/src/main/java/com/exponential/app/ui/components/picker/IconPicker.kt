package com.exponential.app.ui.components.picker

import com.exponential.app.domain.DomainContract

/**
 * EXP-1029 contract — `IconPicker(set)` under the shared picker API.
 *
 * `ui/components/IconPicker.kt` (EXP-575: the square trigger over
 * `IconSwatchGrid`; EXP-924: the SET is a parameter — the board set or the
 * device set) already IS the typed picker of this contract; EXP-1021
 * re-homes it onto [Picker] (the grid as the sheet's body, the trigger
 * unchanged). This file names the two sets as picker items so the contract
 * test can name the picker beside the other nine.
 */
enum class IconPickerSet {
    /** The 96 board / action glyphs (`icons.json` `pickable`). */
    Board,
    /** The six device glyphs (`icons.json` `devicePickable`). */
    Device,
}

/** The set's glyph names as picker items (the glyph is the icon itself). */
fun iconPickerItems(set: IconPickerSet): List<PickerItem<String>> {
    val names = when (set) {
        IconPickerSet.Board -> DomainContract.boardIconValues
        IconPickerSet.Device -> DomainContract.deviceIconValues
    }
    return names.map { PickerItem(value = it, label = it) }
}
