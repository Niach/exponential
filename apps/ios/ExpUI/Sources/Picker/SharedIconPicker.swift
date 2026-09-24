import ExpCore
import SwiftUI

// EXP-1029 contract — `IconPicker(set)` under the shared picker API.
//
// `IconPicker.swift` (EXP-575: the square trigger over `IconSwatchGrid`;
// EXP-924: the SET is a parameter — the board set or the device set) already
// IS the typed picker of this contract; EXP-1021 re-homes it onto
// `GlassPicker` (the grid as the sheet's body, the trigger unchanged). This
// file names the two sets as picker items so the contract test can name
// the picker beside the other nine. (Not `IconPicker.swift`: swiftc refuses
// two files of one name in a module.)

public enum IconPickerSet: Equatable {
    /// The 96 board / action glyphs (`icons.json` `pickable`).
    case board
    /// The six device glyphs (`icons.json` `devicePickable`).
    case device
}

public extension IconPicker {
    /// The set's glyph names as picker items (the glyph is the icon itself).
    nonisolated static func items(for set: IconPickerSet) -> [PickerItem<String>] {
        let names: [String]
        switch set {
        case .board: names = DomainContract.boardIconValues
        case .device: names = DomainContract.deviceIconValues
        }
        return names.map { PickerItem(value: $0, label: $0, icon: $0) }
    }
}
