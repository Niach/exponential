// @vitest-environment jsdom
// #EXP-2161 (member-space.json): the opt-in row, its confirm, the Members
// rows and a team owner's off switch.
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { MEMBER_SPACE, MEMBER_SPACE_COPY } from "@exp/domain-contract/member-space"
import type { SteerDevice } from "@/lib/steer-devices"

const calls = vi.hoisted(() => ({ set: vi.fn(), drop: vi.fn(), forDevice: vi.fn(), remove: vi.fn() }))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    devices: { setAssistantTeam: { mutate: calls.set }, dropAssistantTeam: { mutate: calls.drop } },
    memberSpaces: { forDevice: { query: calls.forDevice }, remove: { mutate: calls.remove } },
  },
}))
const { MemberSpaceMembers, MemberSpaceOptInRow, optInRow, TeamOwnerSpaces } = await import(`./member-spaces`)

const doctor = (state: string | null, detail?: string) =>
  ({ checkedAt: ``, items: state ? [{ key: MEMBER_SPACE.doctorKey, group: `required`, state, ...(detail ? { detail } : {}) }] : [] }) as never

beforeEach(() => {
  for (const call of Object.values(calls)) call.mockReset().mockResolvedValue({ ok: true, txid: 1 })
  calls.forDevice.mockResolvedValue({ members: [] })
})

describe(`member-space.json optInCases (the owner's row)`, () => {
  for (const c of MEMBER_SPACE.optInCases.filter((entry) => entry.caller === `deviceOwner` && entry.shared)) {
    it(c.name, () => {
      // Turning it to `allowed` is possible = the row is enabled while it reads the opposite.
      expect(optInRow({ checked: !c.allowed, kind: c.kind, doctor: doctor(c.doctor) }).disabled).toBe(!c.expected)
    })
  }
  it(`a device that cannot reads the doctor's detail`, () => {
    expect(optInRow({ checked: false, kind: `server`, doctor: doctor(`missing`, `No user namespaces`) })).toEqual({ disabled: true, detail: `No user namespaces` })
    expect(optInRow({ checked: false, kind: `server`, doctor: null }).detail).toBe(`Not available here`)
  })
})

describe(`MemberSpaceOptInRow`, () => {
  const row = (checked: boolean) =>
    render(<MemberSpaceOptInRow deviceId="srv" teamId="t1" kind="server" doctor={doctor(`ok`)} checked={checked} />)
  it(`turning on asks, then sends; turning off is one press`, async () => {
    const confirm = vi.spyOn(window, `confirm`)
    const { unmount } = row(false)
    fireEvent.click(screen.getByRole(`switch`))
    expect(calls.set).not.toHaveBeenCalled()
    expect(await screen.findByText(MEMBER_SPACE_COPY.optInTitle)).toBeTruthy()
    expect(screen.getByText(MEMBER_SPACE_COPY.optInBody)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`member-space-allow`))
    await waitFor(() => expect(calls.set).toHaveBeenCalledWith({ deviceId: `srv`, teamId: `t1`, allowed: true }, expect.anything()))
    unmount()
    row(true)
    fireEvent.click(screen.getByRole(`switch`))
    await waitFor(() => expect(calls.set).toHaveBeenLastCalledWith({ deviceId: `srv`, teamId: `t1`, allowed: false }, expect.anything()))
    expect(confirm).not.toHaveBeenCalled()
  })
})

describe(`Members`, () => {
  it(`one row per member, Remove ends that one`, async () => {
    calls.forDevice.mockResolvedValue({ members: [{ spaceId: `s1`, userId: `u`, name: `Alex`, teamId: `t1` }, { spaceId: `s2`, userId: `v`, name: `Mira`, teamId: `t1` }] })
    render(<MemberSpaceMembers rowId="r1" />)
    expect(await screen.findByText(`Mira`)).toBeTruthy()
    calls.forDevice.mockResolvedValue({ members: [{ spaceId: `s1`, userId: `u`, name: `Alex`, teamId: `t1` }] })
    fireEvent.click(screen.getAllByText(MEMBER_SPACE_COPY.remove)[1]!)
    await waitFor(() => expect(screen.queryByText(`Mira`)).toBeNull())
    expect(calls.remove).toHaveBeenCalledWith({ spaceId: `s2` }, expect.anything())
  })

  it(`a team owner's off switch on a device they do not own, only for their opted-in teams`, async () => {
    const device = { rowId: `r1`, deviceId: `srv`, deviceLabel: `unraid`, assistantTeamIds: [`t1`] } as SteerDevice
    render(<TeamOwnerSpaces device={device} ownedTeams={[{ id: `t1`, name: `Alpha` }, { id: `t2`, name: `Beta` }]} />)
    expect(screen.queryByText(`Beta`)).toBeNull()
    fireEvent.click(screen.getByRole(`switch`))
    await waitFor(() => expect(calls.drop).toHaveBeenCalledWith({ id: `r1`, teamId: `t1` }, expect.anything()))
  })
})
