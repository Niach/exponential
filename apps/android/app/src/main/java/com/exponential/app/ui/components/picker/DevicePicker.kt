package com.exponential.app.ui.components.picker

import androidx.compose.runtime.Composable

/**
 * EXP-1029 contract — the device picker: the machines a run may start on,
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
)

fun devicePickerItems(devices: List<DevicePickerDevice>): List<PickerItem<String>> =
    devices.map { device ->
        PickerItem(
            value = device.id,
            label = device.name,
            description = device.description,
            disabled = device.disabled,
        )
    }

@Composable
fun DevicePicker(
    devices: List<DevicePickerDevice>,
    value: String?,
    onChange: (String) -> Unit,
    trigger: @Composable (open: () -> Unit) -> Unit,
) {
    Picker(
        items = devicePickerItems(devices),
        mode = PickerMode.Single,
        value = setOfNotNull(value),
        onChange = { picked -> picked.firstOrNull()?.let(onChange) },
        emptyText = "No devices",
        title = "Device",
        trigger = trigger,
    )
}
