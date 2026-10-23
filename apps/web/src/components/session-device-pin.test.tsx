import type { ReactElement } from "react"
import { fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { TooltipProvider } from "@exp/ui"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  REASON_ENDED,
  REASON_OFFLINE,
  REASON_STEER,
} from "@/lib/session-device-move"

// EXP-1252: the session composer's Device pin — THE launch line's device
// picker; a pick = `sessions.send({ sessionId, deviceId })`; the current
// device does nothing (NEW-8: nothing is pending on the row).

const TEAM = `t1`
const deviceRow = (over: Record<string, unknown>) => ({
  deviceId: `mac`,
  userId: `me`,
  label: `Mac`,
  kind: `desktop`,
  icon: null,
  agents: [`claude`],
  agentAccounts: {
    claude: {
      signedIn: true,
      profiles: [{ id: `home`, label: `home`, email: `me@acme.test`, signedIn: true, active: true }],
    },
  },
  unauthedAgents: [],
  acpAgents: null,
  sharedTeamIds: [],
  lastSeenAt: new Date(),
  // The first handoff-capable build (`MOVE_MIN_VERSION`): a target before
  // it reads REASON_UPDATE.
  version: `0.14.73`,
  activeSessions: 0,
  ...over,
})
let devices: unknown[] = []

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => ({ data: devices }),
}))
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  userCollection: {},
  sessionCollection: { utils: { awaitTxId: () => Promise.resolve() } },
}))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { id: `me` } } }),
}))
const send = vi.fn()
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { sessions: { send: { mutate: (...args: unknown[]) => send(...args) } } },
}))
const failed = vi.fn()
vi.mock(`@/lib/action-error`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  failedAction: (...args: unknown[]) => failed(...args),
}))

import { SessionDevicePin } from "@/components/session-device-pin"

const session = {
  id: `s-1`,
  agentId: null as string | null,
  deviceId: `mac`,
  shared: false,
  userId: `me`,
  cli: `claude`,
  status: `idle`,
} as const

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const render = (ui: ReactElement) => rtlRender(<TooltipProvider>{ui}</TooltipProvider>)

const trigger = () =>
  screen
    .getByTestId(`session-composer-device-pin`)
    .querySelector(`[data-slot="combobox-inline-trigger"]`) as HTMLButtonElement

beforeEach(() => {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  devices = [
    deviceRow({}),
    deviceRow({ deviceId: `box`, label: `Box` }),
    deviceRow({ deviceId: `old`, label: `Old`, lastSeenAt: new Date(0) }),
  ]
  send.mockReset()
  failed.mockReset()
})

describe(`the session Device pin`, () => {
  it(`reads the current device and moves to the picked one`, async () => {
    send.mockResolvedValue({ txid: 1 })
    render(<SessionDevicePin teamId={TEAM} row={session as never} />)
    expect(trigger().textContent).toContain(`Mac`)
    fireEvent.click(trigger())
    expect(await screen.findByText(REASON_OFFLINE)).toBeTruthy()
    fireEvent.click(await screen.findByText(`Box`))
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        { teamId: TEAM, sessionId: `s-1`, deviceId: `box` },
        expect.anything()
      )
    )
    expect(failed).not.toHaveBeenCalled()
  })

  it(`picking the current device sends nothing`, async () => {
    render(<SessionDevicePin teamId={TEAM} row={session as never} />)
    fireEvent.click(trigger())
    const options = await screen.findAllByText(`Mac`)
    fireEvent.click(options[options.length - 1]!)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(send).not.toHaveBeenCalled()
  })

  it(`toasts a refused move`, async () => {
    send.mockRejectedValue(new Error(`nope`))
    render(<SessionDevicePin teamId={TEAM} row={session as never} />)
    fireEvent.click(trigger())
    fireEvent.click(await screen.findByText(`Box`))
    await waitFor(() =>
      expect(failed).toHaveBeenCalledWith(`move`, `the session`, expect.any(Error))
    )
  })

  it.each([
    [`ended`, { status: `ended` }, REASON_ENDED],
    [`private teammate's`, { userId: `someone-else` }, REASON_STEER],
  ])(`is disabled on an %s session`, async (_name, over, reason) => {
    render(<SessionDevicePin teamId={TEAM} row={{ ...session, ...over } as never} />)
    expect(trigger().disabled).toBe(true)
    fireEvent.focus(trigger().parentElement!)
    expect((await screen.findAllByText(reason)).length).toBeGreaterThan(0)
  })

  it(`is hidden on an agent's conversation`, () => {
    render(
      <SessionDevicePin
        teamId={TEAM}
        row={{ ...session, agentId: `11111111-1111-4111-8111-111111111111` } as never}
      />
    )
    expect(screen.queryByTestId(`session-composer-device-pin`)).toBeNull()
  })
})
