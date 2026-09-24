package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable
import com.exponential.app.ui.components.deviceIcon

/**
 * EXP-1029 contract, EXP-1021 implementation — the device picker: the machines a run may start on,
 * each by its device glyph (`DeviceIconUi`) + name, offline ones disabled
 * with the reason as the description. The composer, the automation editor
 * and the workflow runner row pick one.
 */
data class DevicePickerDevice(
    val id: String,
    val name: String,
    /** Contract `deviceIcon`; null = the kind default. */
    val icon: String? = null,
    /** A muted reason under the name (`Offline`, `Update to run workflows`). */
    val description: String? = null,
    val disabled: Boolean = false,
    /** A server daemon's kind default differs from a desktop's (`deviceIcon`). */
    val isServer: Boolean = false,
)

fun devicePickerItems(devices: List<DevicePickerDevice>): List<PickerItem<String>> =
    devices.map { device ->
        PickerItem(
            value = device.id,
            label = device.name,
            description = device.description,
            disabled = device.disabled,
            // `deviceIcon` always resolves — a machine that never picked one
            // still draws its kind's default.
            icon = deviceIcon(device.icon, isServer = device.isServer),
        )
    }

@Composable
fun DevicePicker(
    devices: List<DevicePickerDevice>,
    value: String?,
    onChange: (String) -> Unit,
    /** The sheet headline; the default names the picker. */
    title: String = "Device",
    /**
     * EXP-1021: the sheet CONTROLLED by the caller, for a picker that is a
     * state machine rather than a chip (the issue screens open theirs from a
     * properties sheet that has already closed).
     */
    open: Boolean? = null,
    onOpenChange: ((Boolean) -> Unit)? = null,
    trigger: @Composable (open: () -> Unit) -> Unit = {},
) {
    Picker(
        items = devicePickerItems(devices),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        emptyText = "No devices",
        title = title,
        open = open,
        onOpenChange = onOpenChange,
        trigger = trigger,
    )
}
