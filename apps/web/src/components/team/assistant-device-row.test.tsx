// @vitest-environment jsdom
// #EXP-2161 (member-space.json): Runs on's team sections, the pick's confirm,
// the waiting line, the empty device.
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { MEMBER_SPACE, MEMBER_SPACE_COPY } from "@exp/domain-contract/member-space"
import type { Device } from "@/db/schema"

const request = vi.hoisted(() => vi.fn())
const mine = vi.hoisted(() => vi.fn())
const rows = vi.hoisted(() => ({ devices: [] as unknown[], users: [] as unknown[] }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: { memberSpaces: { request: { mutate: request }, mine: { query: mine } } } }))
vi.mock(`@/lib/collections`, () => ({ deviceCollection: { name: `devices`, utils: { awaitTxId: () => Promise.resolve() } }, userCollection: { name: `users` } }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: { user: { id: `u1` } } }) }))
vi.mock(`@/components/home/use-member-teams`, () => ({ useMemberTeams: () => ({ teams: [{ id: `t2`, name: `Beta` }, { id: `t1`, name: `Alpha` }] }) }))
vi.mock(`@tanstack/react-db`, () => ({
  useLiveQuery: (build: (q: { from: (source: Record<string, { name: string }>) => unknown }) => unknown) => ({
    data: build({ from: (source) => rows[Object.values(source)[0]!.name as `devices` | `users`] }),
  }),
}))

const { AssistantDeviceRow, spaceDevice, spacePicks } = await import(`./assistant-device-row`)
const { composeDeviceList } = await import(`@/lib/steer-devices`)

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const now = new Date()
const seen = (online: boolean) => new Date(now.getTime() - (online ? 0 : 3_600_000))
const device = (patch: Partial<Device>): Device =>
  ({ id: `r`, deviceId: `d`, userId: `u1`, label: ``, kind: `desktop`, icon: null, caps: [], agents: [], unauthedAgents: [], acpAgents: null, sharedTeamIds: [], assistantTeamIds: [], spaceHostId: null, spaceTeamId: null, updateRequestedAt: null, activeSessions: 0, lastSeenAt: now, ...patch }) as Device

const mac = device({ id: `r0`, deviceId: `mac`, label: `MacBook` })
const unraid = device({ id: `r1`, deviceId: `srv-b`, userId: `u2`, label: `unraid`, kind: `server`, sharedTeamIds: [`t1`], assistantTeamIds: [`t1`], agentAccounts: { claude: { signedIn: true, profiles: [] } } as never })
const build = device({ id: `r2`, deviceId: `srv-a`, userId: `u2`, label: `build`, kind: `server`, lastSeenAt: seen(false), assistantTeamIds: [`t1`, `t2`] })
const space = device({ id: `r4`, deviceId: `space-1`, label: `unraid`, kind: `server`, spaceHostId: `r1`, spaceTeamId: `t1` })

const trigger = () => screen.getByText(`Device`).closest(`button`)!

beforeEach(() => {
  request.mockReset()
  mine.mockReset()
  mine.mockResolvedValue({ space: null, ended: false })
  rows.users = [{ id: `u2`, name: `Jonas` }]
  rows.devices = [mac, unraid, build]
})

const show = (deviceId: string | null) => {
  const onPickOwn = vi.fn()
  const onPickedSpace = vi.fn()
  const own = composeDeviceList(rows.devices as Device[], new Map(), now, `u1`)
  render(<AssistantDeviceRow deviceId={deviceId} devices={own} onPickOwn={onPickOwn} onPickedSpace={onPickedSpace} />)
  return { onPickOwn, onPickedSpace }
}

describe(`member-space.json sectionCases`, () => {
  for (const c of MEMBER_SPACE.sectionCases) {
    it(c.name, () => {
      const { picks, value } = spacePicks(
        c.devices.map((row) => ({ ...row, id: row.rowId, lastSeenAt: seen(row.online), icon: null, kind: `server` })),
        c.teams,
        c.boundId,
        c.me,
        now
      )
      const sections: unknown[] = []
      for (const team of c.teams) {
        const own = picks.filter((pick) => pick.teamId === team.id)
        if (own.length === 0) continue
        sections.push({
          teamId: team.id,
          title: own[0]!.section,
          rows: own.map((pick) => ({ id: pick.hostDeviceId, label: pick.name, disabled: pick.disabled, selected: pick.checked })),
        })
      }
      expect(sections).toEqual(c.expected)
      expect(picks.find((pick) => pick.id === value)?.selected ?? false).toBe(picks.some((pick) => pick.selected))
    })
  }
})

describe(`AssistantDeviceRow`, () => {
  it(`own machines first, then the teams' sections; a pick confirms, then requests`, async () => {
    request.mockResolvedValue({ spaceId: `s`, deviceId: `space-s`, txid: 1 })
    const { onPickOwn, onPickedSpace } = show(`mac`)
    fireEvent.click(trigger())
    const panel = await screen.findByTestId(`agent-device`)
    expect([...panel.querySelectorAll(`[cmdk-group-heading]`)].map((el) => el.textContent)).toEqual([`Beta`, `Alpha`])
    expect([...panel.querySelectorAll(`[cmdk-item]`)].map((el) => el.textContent)).toEqual([`MacBook`, `buildOffline`, `buildOffline`, `unraid`])
    fireEvent.click(screen.getByText(`unraid`))
    expect(request).not.toHaveBeenCalled()
    expect(await screen.findByText(`Run your Assistant on unraid?`)).toBeTruthy()
    expect(screen.getByText(/^Jonas and admins of this machine can read/)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`assistant-space-confirm`))
    await waitFor(() => expect(onPickedSpace).toHaveBeenCalledWith(`space-s`))
    expect(request).toHaveBeenCalledWith({ deviceId: `srv-b`, teamId: `t1` }, expect.anything())
    expect(onPickOwn).not.toHaveBeenCalled()
  })

  it(`an own machine is picked without a confirm`, async () => {
    const { onPickOwn } = show(null)
    expect(trigger().textContent).toContain(MEMBER_SPACE_COPY.noDevice)
    fireEvent.click(trigger())
    fireEvent.click(await screen.findByText(`MacBook`))
    expect(onPickOwn).toHaveBeenCalledWith(`mac`)
  })

  it(`the bound space shows as its host's row, waiting while its install waits`, async () => {
    rows.devices = [mac, unraid, build, space]
    mine.mockResolvedValue({ space: { spaceId: `r4`, status: `waiting` }, ended: false })
    show(`space-1`)
    expect(trigger().textContent).toContain(`unraid`)
    expect((await screen.findByTestId(`assistant-space-waiting`)).textContent).toBe(MEMBER_SPACE_COPY.waiting)
    fireEvent.click(trigger())
    const panel = await screen.findByTestId(`agent-device`)
    const picked = [...panel.querySelectorAll(`[data-picked]`)].map((el) => el.textContent)
    expect(picked).toEqual([`unraid`])
    // Never the space itself beside it.
    expect([...panel.querySelectorAll(`[cmdk-item]`)].filter((el) => el.textContent === `unraid`)).toHaveLength(1)
  })

  it(`a ready space shows no waiting line`, async () => {
    rows.devices = [mac, unraid, space]
    mine.mockResolvedValue({ space: { spaceId: `r4`, status: `ready` }, ended: false })
    show(`space-1`)
    await waitFor(() => expect(mine).toHaveBeenCalled())
    expect(screen.queryByTestId(`assistant-space-waiting`)).toBeNull()
  })

  it(`Account and Model read the space's logins, the host's until it reports`, () => {
    expect(spaceDevice(space, unraid, `u1`, now).agentAccounts).toEqual(unraid.agentAccounts)
    const own = { claude: { signedIn: true, profiles: [{ id: `p` }] } } as never
    expect(spaceDevice({ ...space, agentAccounts: own }, unraid, `u1`, now).agentAccounts).toEqual(own)
  })
})
