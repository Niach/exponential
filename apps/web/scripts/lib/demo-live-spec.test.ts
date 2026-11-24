import { describe, expect, it } from "vitest"
import contract from "@exp/domain-contract/contract"
import { sessionScreenStatus } from "@exp/domain-contract/session-screen"
import { DEMO_AWAY_SESSION_ID } from "../screenshot-demo"
import { DEMO_LIVE_SESSIONS, demoLiveRow } from "./demo-live-spec"

const NOW = Date.UTC(2026, 10, 24, 12)
const SERVER = `Acme build server`

/** The chat header's word for one seeded root (session-screen.json STATUS WORD). */
const headerWord = (id: string) => {
  const spec = DEMO_LIVE_SESSIONS.find((row) => row.id === id)!
  const row = demoLiveRow(spec, NOW)
  return sessionScreenStatus([
    {
      id,
      status: row.status,
      busy: row.busy,
      needsInput: row.needsInput,
      blocked: row.blocked?.kind ?? null,
      resumed: Boolean(row.blocked?.resumeSince),
      device: SERVER,
      root: true,
    },
  ])
}

describe(`#EXP-2105: the seeded session on the offline build server`, () => {
  const spec = DEMO_LIVE_SESSIONS.find((row) => row.id === DEMO_AWAY_SESSION_ID)!
  const row = demoLiveRow(spec, NOW)

  it(`reads its wall in the chat header, never working`, () => {
    expect(headerWord(DEMO_AWAY_SESSION_ID)).toBe(`${SERVER} offline`)
  })

  it(`is the row the watchdog leaves on a device away for hours`, () => {
    // flagOffline: a busy row whose host missed the online window = idle,
    // not busy, device_offline + resume; flagOfflineTooLong: still away
    // `offlineMinutes` after the flag = on a person.
    expect({ status: row.status, busy: row.busy, needsInput: row.needsInput }).toEqual({
      status: `idle`,
      busy: false,
      needsInput: true,
    })
    expect(row.blocked).toMatchObject({ kind: `device_offline`, resume: true, resumeSince: null })
    const flaggedMinutes = (NOW - Date.parse(row.blocked!.since ?? ``)) / 60_000
    expect(flaggedMinutes).toBeGreaterThan(contract.sessionWatchdog.offlineMinutes)
    // Its last turn is no later than the flag: nothing ran since.
    expect(row.lastTurnAt.getTime()).toBeLessThanOrEqual(Date.parse(row.blocked!.since ?? ``))
    expect(row.startedAt.getTime()).toBeLessThan(row.lastTurnAt.getTime())
  })

  it(`is the only away row: every other live row has no flag`, () => {
    for (const other of DEMO_LIVE_SESSIONS.filter((s) => s.id !== DEMO_AWAY_SESSION_ID)) {
      expect(demoLiveRow(other, NOW).blocked).toBeNull()
    }
  })
})
