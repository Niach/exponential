import type { ReactElement } from "react"
import { fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react"
import { TooltipProvider } from "@exp/ui"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { REASON_AGENT, REASON_BUSY, REASON_OFFLINE, WALL_SWITCH_LABEL } from "@/lib/session-account-switch"

// EXP-849: the usage wall's "Switch account" control — THE account picker
// behind a button, a pick = `sessions.send({ sessionId, account })`.

const deviceRow = {
  deviceId: `dev-1`,
  userId: `me`,
  lastSeenAt: new Date(),
  launchDefaults: { defaultAgent: `claude` },
  agentAccounts: {
    claude: {
      signedIn: true,
      profiles: [
        { id: `home`, label: `home`, email: `home@acme.test`, signedIn: true, active: true },
        { id: `work`, label: `work`, email: `work@acme.test`, signedIn: true, active: false },
      ],
    },
  },
  agentUsage: {},
}
let devices: unknown[] = [deviceRow]

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => true,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => ({ data: devices }),
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

import { SessionAccountSwitchButton } from "@/components/session-account-switch"

const session = {
  id: `s-1`,
  cli: `claude`,
  account: `home`,
  deviceId: `dev-1`,
  userId: `me`,
  busy: false,
  shared: false,
}

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const render = (ui: ReactElement) => rtlRender(<TooltipProvider>{ui}</TooltipProvider>)

/** The refusal rides the control's own tooltip. */
async function expectRefused(reason: string) {
  const button = screen.getByTestId(`session-account-switch`) as HTMLButtonElement
  expect(button.disabled).toBe(true)
  fireEvent.focus(button.parentElement!)
  expect((await screen.findAllByText(reason)).length).toBeGreaterThan(0)
}

beforeEach(() => {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  devices = [deviceRow]
  send.mockReset()
  failed.mockReset()
})

describe(`the wall's Switch account control`, () => {
  it(`switches to the picked login with no text`, async () => {
    send.mockResolvedValue({})
    render(<SessionAccountSwitchButton session={session} />)
    const button = screen.getByTestId(`session-account-switch`)
    expect(button.textContent).toContain(WALL_SWITCH_LABEL)
    fireEvent.click(button)
    fireEvent.click(await screen.findByText(`work@acme.test`))
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith({ sessionId: `s-1`, account: `work` }, expect.anything())
    )
    expect(failed).not.toHaveBeenCalled()
  })

  it(`toasts a refused switch`, async () => {
    send.mockRejectedValue(new Error(`nope`))
    render(<SessionAccountSwitchButton session={session} />)
    fireEvent.click(screen.getByTestId(`session-account-switch`))
    fireEvent.click(await screen.findByText(`work@acme.test`))
    await waitFor(() =>
      expect(failed).toHaveBeenCalledWith(`switch`, `the account`, expect.any(Error))
    )
  })

  it.each([
    [`codex`, { cli: `codex` }, REASON_AGENT],
    [`busy`, { busy: true }, REASON_BUSY],
  ])(`is disabled on a %s session`, async (_name, over, reason) => {
    render(<SessionAccountSwitchButton session={{ ...session, ...over }} />)
    await expectRefused(reason)
  })

  it(`is disabled while the machine is offline`, async () => {
    devices = [{ ...deviceRow, lastSeenAt: new Date(0) }]
    render(<SessionAccountSwitchButton session={session} />)
    await expectRefused(REASON_OFFLINE)
  })

  it(`is hidden from a member who may not steer a private session`, () => {
    render(<SessionAccountSwitchButton session={{ ...session, userId: `someone-else` }} />)
    expect(screen.queryByTestId(`session-account-switch`)).toBeNull()
  })

  it(`shows on a shared session for every member`, () => {
    render(
      <SessionAccountSwitchButton session={{ ...session, userId: `someone-else`, shared: true }} />
    )
    expect(screen.getByTestId(`session-account-switch`)).toBeTruthy()
  })
})
