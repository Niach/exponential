// #EXP-2161 (member-space.json `listed`): a member space is in no device list.
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import { findPageDevice } from "@/components/devices/use-device-page"
import { composeDeviceList, listedDevices } from "./steer-devices"

const row = (patch: Partial<Device>) =>
  ({ id: `r`, deviceId: `d`, userId: `me`, label: `x`, kind: `server`, caps: [], agents: [], acpAgents: null, unauthedAgents: [], sharedTeamIds: [], updateRequestedAt: null, lastSeenAt: new Date(), spaceHostId: null, ...patch }) as Device
const rows = [row({ id: `r1`, deviceId: `mac` }), row({ id: `r2`, deviceId: `space-1`, spaceHostId: `r9` })]

describe(`listedDevices`, () => {
  it(`drops the spaces from the lists, the pickers and the device page`, () => {
    expect(listedDevices(rows).map((r) => r.id)).toEqual([`r1`])
    expect(composeDeviceList(rows, new Map(), new Date(), `me`).map((d) => d.deviceId)).toEqual([`mac`])
    expect(findPageDevice(rows, `space-1`, { userId: `me`, teamIds: [] })).toBeNull()
  })

  // Reading the devices collection is either a LIST (through the selector) or
  // a lookup by a named id, which a space must answer (it is the Assistant's).
  const LOOKUPS = [
    `src/components/agent-login-dialog.tsx`,
    `src/components/device-settings-dialog.tsx`,
    `src/components/entity-preview/use-entity-ref-row.ts`,
    `src/components/session-account-switch.tsx`,
    `src/components/session-composer.tsx`,
    `src/components/team/assistant-device-row.tsx`,
    `src/components/team/work-tabs-strip.tsx`,
    `src/components/team/work-tabs-sync.tsx`,
    `src/hooks/use-ci-run.ts`,
    `src/hooks/use-device-glyphs.ts`,
    `src/hooks/use-needs-you.ts`,
    `src/hooks/use-session-agent-usage.ts`,
    `src/hooks/use-session-device.ts`,
    `src/hooks/use-session-usage-refresh.ts`,
    `src/lib/collections.ts`,
    `src/lib/enet/send-sealed.ts`,
  ]
  it(`every reader of the devices collection lists through the selector or is a lookup`, () => {
    const files = (dir: string): string[] =>
      readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const path = join(dir, entry.name)
        return entry.isDirectory() ? files(path) : /\.tsx?$/.test(path) && !path.includes(`.test.`) ? [path] : []
      })
    const readers = files(`src`).filter((path) => /\bdeviceCollection\b/.test(readFileSync(path, `utf8`)))
    const listing = readers.filter((path) => !/\b(listedDevices|composeDeviceList)\(/.test(readFileSync(path, `utf8`)))
    expect(listing.filter((path) => !LOOKUPS.includes(path))).toEqual([])
  })
})
