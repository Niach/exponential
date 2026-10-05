import type { DeviceRow } from "./model"

// EXP-1183 — placeholder; the devices view lands with the device lane.
export function DevicesView({ devices }: { devices: readonly DeviceRow[] }) {
  return <div className="p-4 text-sm">{devices.length} devices</div>
}
