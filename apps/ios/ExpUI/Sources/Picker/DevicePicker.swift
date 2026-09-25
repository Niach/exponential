import SwiftUI

// EXP-1029 contract — the device picker: the machines a run may start on,
// each by its device glyph (`DeviceIconDisplay`) + name, offline ones
// disabled with the reason as the description. The composer, the automation
// editor and the workflow runner row pick one.

public struct DevicePickerDevice: Identifiable, Hashable {
    public let id: String
    public let name: String
    /// Contract `deviceIcon`; nil = the kind default.
    public let icon: String?
    /// A muted reason under the name (`Offline`, `Update to run workflows`).
    public let description: String?
    public let disabled: Bool

    public init(id: String, name: String, icon: String? = nil, description: String? = nil, disabled: Bool = false) {
        self.id = id
        self.name = name
        self.icon = icon
        self.description = description
        self.disabled = disabled
    }
}

public struct DevicePicker<Trigger: View>: View {
    public let devices: [DevicePickerDevice]
    public let value: String?
    public let onChange: (String) -> Void
    /// The sheet headline; the default names the picker. A row that asks the
    /// question in its own words — the automation editor's "Runs on" — says so
    /// here, exactly as on Android (`DevicePicker.kt`), so the sheet cannot
    /// contradict the row that opened it.
    public let title: String
    /// EXP-1021 — the surface controls every typed picker forwards verbatim
    /// (web's `PickerSurfaceProps`): a host that opens the picker from its own
    /// property row or `…` menu drives `open` and hides the trigger, and
    /// `onDismiss` fires once the sheet finished animating away (what a
    /// hand-off to a SECOND picker is promoted on).
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        devices: [DevicePickerDevice],
        value: String?,
        onChange: @escaping (String) -> Void,
        title: String = "Device",
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.devices = devices
        self.value = value
        self.onChange = onChange
        self.title = title
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    nonisolated public static func items(_ devices: [DevicePickerDevice]) -> [PickerItem<String>] {
        devices.map { device in
            PickerItem(
                value: device.id,
                label: device.name,
                icon: device.icon,
                description: device.description,
                disabled: device.disabled
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(devices),
            mode: .single,
            value: value.map { [$0] } ?? [],
            onChange: { picked in picked.first.map(onChange) },
            emptyText: "No devices",
            title: title,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            trigger: trigger
        )
    }
}
