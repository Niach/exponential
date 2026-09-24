import type { ReactNode } from "react"

import { getDeviceIcon } from "../device-icons"
import {
  Picker,
  type PickerItem,
  type PickerSurfaceProps,
} from "./picker"

// EXP-1029 contract — the device picker: the machines a run may start on,
// each by its device glyph (contract `deviceIcon`) + name, offline ones
// rendered disabled with the reason as the description. The composer, the
// automation editor and the workflow runner row pick one.

export interface DevicePickerDevice {
  id: string
  name: string
  /** Contract `deviceIcon`; absent = the KIND default (`ui-server` for a
   *  server, `ui-device` otherwise) — a row is never iconless. */
  icon?: string | null
  /** Contract `deviceKind`, read only for that default. */
  kind?: string | null
  /** A muted reason under the name (`Offline`, `Update to run workflows`). */
  description?: ReactNode
  disabled?: boolean
}

export interface DevicePickerProps extends PickerSurfaceProps {
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
    icon: getDeviceIcon(device),
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
