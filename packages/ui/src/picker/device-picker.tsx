import type { ReactNode } from "react"

import { DEVICE_ICON_OPTIONS } from "../device-icons"
import { Picker, type PickerItem } from "./picker"

// EXP-1029 contract — the device picker: the machines a run may start on,
// each by its device glyph (contract `deviceIcon`) + name, offline ones
// rendered disabled with the reason as the description. The composer, the
// automation editor and the workflow runner row pick one.

export interface DevicePickerDevice {
  id: string
  name: string
  /** Contract `deviceIcon`; absent = the kind default. */
  icon?: string | null
  /** A muted reason under the name (`Offline`, `Update to run workflows`). */
  description?: ReactNode
  disabled?: boolean
}

export interface DevicePickerProps {
  devices: readonly DevicePickerDevice[]
  value: string | null
  onChange: (deviceId: string) => void
  trigger: ReactNode
  /** The sheet's title on a phone. */
  mobileTitle?: string
  search?: boolean
  disabled?: boolean
  emptyText?: string
  className?: string
}

export function devicePickerItems(devices: readonly DevicePickerDevice[]): PickerItem[] {
  return devices.map((device) => ({
    value: device.id,
    label: device.name,
    icon: DEVICE_ICON_OPTIONS.find((option) => option.name === device.icon)?.icon,
    description: device.description,
    disabled: device.disabled,
  }))
}

export function DevicePicker({
  devices,
  emptyText = `No devices`,
  mobileTitle = `Device`,
  ...props
}: DevicePickerProps) {
  return (
    <Picker
      mode="single"
      items={devicePickerItems(devices)}
      emptyText={emptyText}
      mobileTitle={mobileTitle}
      {...props}
    />
  )
}
