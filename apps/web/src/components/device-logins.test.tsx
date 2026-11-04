import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { SteerDevice } from "@/lib/steer-devices"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/add-account-dialog`, () => ({ AddAccountDialog: () => null }))
vi.mock(`@/components/agent-login-dialog`, () => ({ requestAgentLogin: vi.fn() }))

const { DeviceAccountIcons } = await import(`@/components/device-logins`)

const NOW = new Date(`2026-11-04T12:00:00Z`)
const row = (key: string, email: string) => ({
  key,
  agent: `claude`,
  profileId: key,
  email,
  plan: `Max`,
  signedIn: true,
  active: false,
  health: `ok`,
})

function draw(rows: ReturnType<typeof row>[], readOnly = false) {
  const device = { deviceId: `d1`, deviceLabel: `mint`, agentAccounts: [] } as unknown as SteerDevice
  return render(
    <DeviceAccountIcons device={device} rows={rows as never} attention={[]} now={NOW} readOnly={readOnly} />
  )
}

// #EXP-1643 (devices-list.json THE ACCOUNT PANEL).
describe(`DeviceAccountIcons`, () => {
  beforeEach(() => {
    const store = new Map<string, string>()
    vi.stubGlobal(`localStorage`, {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    })
  })

  it(`opens a lone login by default and remembers closing it`, () => {
    const one = [row(`a`, `a@x.at`)]
    const first = draw(one)
    expect(screen.getByText(`a@x.at`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`device-login-chip-a`))
    expect(screen.queryByText(`a@x.at`)).toBeNull()
    first.unmount()
    draw(one)
    expect(screen.queryByText(`a@x.at`)).toBeNull()
  })

  it(`opens one login at a time and remembers it per device`, () => {
    const two = [row(`a`, `a@x.at`), row(`b`, `b@x.at`)]
    const first = draw(two)
    expect(screen.queryByText(`a@x.at`)).toBeNull()
    fireEvent.click(screen.getByTestId(`device-login-chip-a`))
    fireEvent.click(screen.getByTestId(`device-login-chip-b`))
    expect(screen.queryByText(`a@x.at`)).toBeNull()
    expect(screen.getByText(`b@x.at`)).toBeTruthy()
    first.unmount()
    draw(two)
    expect(screen.getByText(`b@x.at`)).toBeTruthy()
    expect(screen.getByTestId(`device-login-chip-b`).getAttribute(`aria-expanded`)).toBe(`true`)
  })

  it(`shows a read-only device's panel without actions`, () => {
    draw([row(`a`, `a@x.at`)], true)
    expect(screen.getByText(`a@x.at`)).toBeTruthy()
    expect(screen.queryByTestId(`device-account-sign-out`)).toBeNull()
    expect(screen.queryByTestId(`device-account-remove`)).toBeNull()
  })
})
