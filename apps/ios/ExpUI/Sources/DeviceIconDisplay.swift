import ExpCore
import SwiftUI

// EXP-924: THE device glyph resolver — the iOS twin of web's
// `getDeviceIconName` (packages/ui/src/device-icons.ts), desktop's
// `device_icon_name` and Android's `deviceIconName`. Every surface that draws
// a glyph for a CONCRETE device row goes through it, so an owner's pick and
// the kind fallback can never disagree between two lists.
public enum DeviceIconDisplay {
    /// A device's canonical icon NAME: the owner's pick when it is one of the
    /// device set's names, else the kind default — the server / desktop glyphs
    /// every machine wore before the column existed, and what a machine with
    /// no pick still gets.
    ///
    /// A name OUTSIDE `AppIcons.devicePickable` (a board-set glyph, a name a
    /// newer client knows and this build does not) falls back rather than
    /// drawing something arbitrary: the picker's set is the whole vocabulary.
    public static func iconName(icon: String?, kind: String?) -> String {
        if let icon, AppIcons.devicePickable.contains(icon) { return icon }
        return kind == "server" ? AppIcons.uiServer : AppIcons.uiDevice
    }

    /// The glyph for a machine as the pickers and lists hold it.
    public static func iconName(for device: SteerDevice) -> String {
        iconName(icon: device.icon, kind: device.kind)
    }

    /// The glyph for a synced devices row.
    public static func iconName(for entity: DeviceEntity) -> String {
        iconName(icon: entity.icon, kind: entity.kind)
    }
}
