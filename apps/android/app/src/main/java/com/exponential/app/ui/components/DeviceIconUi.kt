package com.exponential.app.ui.components

import androidx.compose.ui.graphics.vector.ImageVector
import com.exponential.app.data.api.SteerDevice
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-924: THE device glyph resolver — the Android twin of web
 * `getDeviceIconName`/`getDeviceIcon` (`@exp/ui` device-icons.ts), iOS
 * `DeviceIcons` and desktop `device_icon`. Every surface that draws a concrete
 * device ROW goes through it, so an owner's pick shows up everywhere at once.
 *
 * The rule: the stored name when it is in the registry's `devicePickable` set
 * (the contract's `deviceIcon`), else the KIND default — the `ui-server` /
 * `ui-device` concept glyphs every machine wore before the column existed, and
 * what a NULL still means. A name from the BOARD set (`rocket`) is not a device
 * icon: it reads as unset rather than as a phantom pick, which is also what an
 * older client's row looks like after the sets diverge.
 *
 * NOT used for the generic "install the desktop app" / "run a server" cards:
 * those illustrate a device KIND, not a machine somebody named.
 */

/** The desktop kind default — the `ui-device` concept's glyph by name. */
const val DEVICE_ICON_DESKTOP_DEFAULT = "monitor"

/** The server kind default — the `ui-server` concept's glyph by name. */
const val DEVICE_ICON_SERVER_DEFAULT = "server"

/**
 * The registry NAME a device draws: [icon] when it is device-pickable, else
 * the default for its kind. Always resolves — the picker's selection is this,
 * so a machine that never picked one shows `monitor` (or `server`) selected.
 */
fun deviceIconName(icon: String?, isServer: Boolean): String =
    deviceIconPick(icon)
        ?: if (isServer) DEVICE_ICON_SERVER_DEFAULT else DEVICE_ICON_DESKTOP_DEFAULT

fun deviceIconName(device: SteerDevice): String = deviceIconName(device.icon, device.isServer)

/** The glyph [deviceIconName] names — the concept glyph on the default path. */
fun deviceIcon(icon: String?, isServer: Boolean): ImageVector =
    deviceIconPick(icon)?.let { ExpIcons.byName(it) }
        ?: if (isServer) ExpIcons.uiServer else ExpIcons.uiDevice

fun deviceIcon(device: SteerDevice): ImageVector = deviceIcon(device.icon, device.isServer)

/**
 * The device-pickable name a stored value names, or null when it names none —
 * blank, unknown to this build's registry, or pickable only as a BOARD icon.
 */
fun deviceIconPick(value: String?): String? =
    value?.takeIf { it.isNotBlank() && it in ExpIcons.devicePickable }
