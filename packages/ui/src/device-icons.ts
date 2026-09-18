import type { LucideIcon } from "lucide-react"
import type { DeviceIcon } from "@exp/db-schema/domain"
import { DEVICE_ICONS, SEMANTIC_ICONS, isDeviceIcon } from "@exp/icons"
import { ICON_COMPONENTS } from "./icons.generated"

// EXP-924: the device icon set — device types plus OS marks, a SECOND curated
// set next to the board one (`board-icons.ts`), from the same registry
// (icons.json `devicePickable` = the contract's `deviceIcon`).
export const DEVICE_ICON_OPTIONS = DEVICE_ICONS.map((name) => ({
  name: name as DeviceIcon,
  icon: ICON_COMPONENTS[name],
}))

// A device's canonical icon NAME: the owner's pick when set, else the kind
// default — the registry's server / desktop glyphs every machine wore before
// the column existed, and what a newly registered one still gets.
export function getDeviceIconName(device: {
  icon?: string | null
  kind?: string | null
}): DeviceIcon {
  if (device.icon && isDeviceIcon(device.icon)) return device.icon
  return SEMANTIC_ICONS[device.kind === `server` ? `ui-server` : `ui-device`]
}

// A device's display icon component.
export function getDeviceIcon(device: {
  icon?: string | null
  kind?: string | null
}): LucideIcon {
  return ICON_COMPONENTS[getDeviceIconName(device)]
}
