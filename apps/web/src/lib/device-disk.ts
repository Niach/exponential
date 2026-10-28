import { and, eq } from "drizzle-orm"
import type { DeviceDoctor } from "@/db/schema"
import { db } from "@/db/connection"
import { devices } from "@/db/schema"

// EXP-1375 (device-doctor.json, the storage rule): the device decides whether
// it is low on disk (its doctor's `disk` row is `action`, below 10 GB). A
// session start on such a device says so in the sessions.send result, before
// its runs fail on ENOSPC.

/** The start warning for a device whose doctor says it is low on disk; null
 *  when it is not (or never reported). Pure. */
export function lowDiskWarning(
  label: string,
  doctor: Pick<DeviceDoctor, `items`> | null | undefined
): string | null {
  const disk = doctor?.items.find((item) => item.key === `disk`)
  if (!disk || disk.state !== `action`) return null
  return disk.detail ? `${label} is low on disk: ${disk.detail}.` : `${label} is low on disk.`
}

/** The warning for the device a send queued work on. Never throws: a failed
 *  read reads as no warning. */
export async function startDiskWarning(queued: {
  ownerId: string
  deviceId: string
}): Promise<string | null> {
  try {
    const [row] = await db
      .select({ label: devices.label, doctor: devices.doctor })
      .from(devices)
      .where(and(eq(devices.deviceId, queued.deviceId), eq(devices.userId, queued.ownerId)))
      .limit(1)
    return row ? lowDiskWarning(row.label || queued.deviceId, row.doctor) : null
  } catch {
    return null
  }
}
