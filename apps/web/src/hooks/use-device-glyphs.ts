import { useMemo } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { getDeviceIcon } from "@exp/ui"
import type { ComponentType } from "react"
import type { Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"

// EXP-1318: a thread row's trailing DEVICE glyph ×4 — the root's
// `device_id` (the LOGICAL device id) resolved against the synced devices (only the ones this viewer
// may see sync), drawn exactly as the Devices page draws it
// (`getDeviceIcon`: the owner's pick, else the kind default). Unknown or
// invisible device = no glyph.

export interface DeviceGlyph {
  icon: ComponentType<{ className?: string }>
  name: string | null
}

export function useDeviceGlyphs(): (deviceId: string | null | undefined) => DeviceGlyph | null {
  const { data } = useLiveQuery((query) => query.from({ d: deviceCollection }), [])
  const byId = useMemo(() => {
    const map = new Map<string, DeviceGlyph>()
    const rows = (data ?? []) as Device[]
    // The row's key = `sessions.device_id`, the device's LOGICAL id
    // (`devices.device_id`, as the IDE's `find_device` and the Devices page
    // match it); the row id stays a fallback.
    for (const device of rows) {
      map.set(device.id, { icon: getDeviceIcon(device), name: device.label || null })
    }
    for (const device of rows) {
      map.set(device.deviceId, { icon: getDeviceIcon(device), name: device.label || null })
    }
    return map
  }, [data])
  return (deviceId) => (deviceId ? (byId.get(deviceId) ?? null) : null)
}
