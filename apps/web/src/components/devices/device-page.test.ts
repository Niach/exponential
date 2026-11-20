import { describe, expect, it } from "vitest"
import type { Device } from "@/db/schema"
import type { ChatsListGroup, ChatsListRow } from "@/lib/session-tree"
import { findPageDevice } from "@/components/devices/use-device-page"
import { deviceRunningRows } from "@/components/devices/device-running-section"
import { accountSubtitle } from "@/components/devices/device-accounts-section"

const device = (patch: Partial<Device>) =>
  ({ id: `row-1`, deviceId: `mac-1`, userId: `me`, kind: `desktop`, sharedTeamIds: [], ...patch }) as Device

describe(`findPageDevice`, () => {
  const viewer = { userId: `me`, teamIds: [`t-acme`] }

  it(`finds the viewer's own device by row id or logical id`, () => {
    const rows = [device({})]
    expect(findPageDevice(rows, `row-1`, viewer)?.id).toBe(`row-1`)
    expect(findPageDevice(rows, `mac-1`, viewer)?.id).toBe(`row-1`)
    expect(findPageDevice(rows, `nope`, viewer)).toBeNull()
  })

  it(`a teammate's server only when shared with a team of the viewer's`, () => {
    const shared = device({ id: `row-2`, userId: `them`, kind: `server`, sharedTeamIds: [`t-acme`] })
    const elsewhere = device({ id: `row-3`, userId: `them`, kind: `server`, sharedTeamIds: [`t-other`] })
    const desktop = device({ id: `row-4`, userId: `them`, kind: `desktop`, sharedTeamIds: [`t-acme`] })
    const rows = [shared, elsewhere, desktop]
    expect(findPageDevice(rows, `row-2`, viewer)?.id).toBe(`row-2`)
    expect(findPageDevice(rows, `row-3`, viewer)).toBeNull()
    expect(findPageDevice(rows, `row-4`, viewer)).toBeNull()
  })
})

describe(`deviceRunningRows`, () => {
  const row = (patch: Partial<ChatsListRow>): ChatsListRow => ({
    sessionId: `s`,
    teamId: `t-acme`,
    depth: 0,
    parentId: null,
    rootId: `s`,
    title: `T`,
    deviceId: `mac-1`,
    busy: true,
    needsYou: false,
    unread: false,
    activityAt: `2026-11-20T00:00:00.000Z`,
    ...patch,
  })
  const groups: ChatsListGroup[] = [
    { key: `needs_you`, title: `Needs you`, rows: [row({ sessionId: `a`, needsYou: true })] },
    {
      key: `running`,
      title: `Running`,
      rows: [
        row({ sessionId: `b` }),
        row({ sessionId: `c`, deviceId: `other` }),
        row({ sessionId: `d`, depth: 1, parentId: `b` }),
      ],
    },
    { key: `recent`, title: `Recent`, rows: [row({ sessionId: `e` })] },
  ]

  it(`keeps the live roots on this device, needs-you first`, () => {
    expect(deviceRunningRows(groups, `mac-1`).map((r) => r.sessionId)).toEqual([`a`, `b`])
  })
})

describe(`accountSubtitle`, () => {
  it(`names the agent, then the plan of a named login`, () => {
    expect(accountSubtitle({ agent: `claude`, plan: `Max`, email: `a@b.dev` })).toBe(`Claude Code · Max`)
    expect(accountSubtitle({ agent: `claude`, plan: null, email: `a@b.dev` })).toBe(`Claude Code`)
  })
})
