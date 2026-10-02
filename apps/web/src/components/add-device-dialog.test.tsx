import { beforeEach, describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import type { SteerDevice } from "@/lib/steer-devices"

const mocks = vi.hoisted(() => {
  const device = Object.assign(vi.fn(), { approve: vi.fn() })
  return { device, createInstallToken: vi.fn() }
})

vi.mock(`@/lib/auth/client`, () => ({ authClient: { device: mocks.device } }))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { devices: { createInstallToken: { mutate: mocks.createInstallToken } } },
}))

import {
  AddDeviceDialog,
  buildServerInstallSnippet,
} from "@/components/add-device-dialog"

const ORIGIN = `https://app.example.com`
const TOKEN = `expi_${`a`.repeat(43)}`

const dev = (deviceId: string, online: boolean): SteerDevice => ({
  deviceId,
  deviceLabel: `Box ${deviceId}`,
  online,
})

beforeEach(() => {
  vi.clearAllMocks()
  mocks.createInstallToken.mockResolvedValue({
    token: TOKEN,
    expiresAt: new Date(Date.now() + 15 * 60_000).toISOString(),
  })
  mocks.device.mockResolvedValue({ data: { status: `pending` }, error: null })
  mocks.device.approve.mockResolvedValue({ data: { success: true }, error: null })
  Object.assign(navigator, { clipboard: { writeText: vi.fn() } })
})

describe(`install one-liner (EXP-1111)`, () => {
  it(`carries the one-time token, or none as the fallback`, () => {
    expect(buildServerInstallSnippet(ORIGIN, TOKEN)).toBe(
      `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${ORIGIN} EXP_INSTALL_TOKEN=${TOKEN} sh`
    )
    expect(buildServerInstallSnippet(ORIGIN)).toBe(
      `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${ORIGIN} sh`
    )
  })
})

// EXP-1169: the dialog is one host of the shared device-setup block, the same
// content the wizard's devices step and the join step render.
describe(`AddDeviceDialog`, () => {
  it(`renders the device-setup block: both cards, then the caller's devices`, async () => {
    const { rerender } = render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    await waitFor(() =>
      expect(screen.getByTestId(`install-snippet`).textContent).toContain(TOKEN)
    )
    expect(screen.getByText(`Get the desktop app`)).toBeTruthy()
    expect(screen.getByText(`Set up a server`)).toBeTruthy()
    expect(
      screen.getByText(
        `No devices yet. Sign in on the desktop app or a server and it shows up here.`
      )
    ).toBeTruthy()

    rerender(
      <AddDeviceDialog
        open
        onOpenChange={() => {}}
        devices={[dev(`a`, true)]}
        origin={ORIGIN}
      />
    )
    expect(screen.getByText(`Box a`)).toBeTruthy()
  })

  it(`the server card is ONE command: the token one-liner, minted once`, async () => {
    render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    await waitFor(() =>
      expect(screen.getByTestId(`install-snippet`).textContent).toContain(
        `EXP_INSTALL_TOKEN=${TOKEN}`
      )
    )
    expect(screen.getAllByTestId(`install-snippet`)).toHaveLength(1)
    expect(screen.getAllByRole(`button`, { name: `Copy install command` })).toHaveLength(1)
    expect(mocks.createInstallToken).toHaveBeenCalledTimes(1)
  })

  it(`keeps the plain command, and says nothing, when the mint fails`, async () => {
    mocks.createInstallToken.mockRejectedValue(new Error(`nope`))
    render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    await waitFor(() => expect(mocks.createInstallToken).toHaveBeenCalled())
    const snippet = screen.getByTestId(`install-snippet`).textContent ?? ``
    expect(snippet).toContain(`EXP_INSTANCE=${ORIGIN} sh`)
    expect(snippet).not.toContain(`EXP_INSTALL_TOKEN`)
    expect(screen.queryByText(/nope|Couldn't/)).toBeNull()
  })

  it(`shows the code field only after the command was copied, and approves in place`, async () => {
    render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    await waitFor(() =>
      expect(screen.getByTestId(`install-snippet`).textContent).toContain(TOKEN)
    )
    const label = `If the CLI shows a code, enter it here`
    expect(screen.queryByLabelText(label)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: `Copy install command` }))
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      buildServerInstallSnippet(ORIGIN, TOKEN)
    )
    fireEvent.change(screen.getByLabelText(label), {
      target: { value: `zp3hv7hk` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Approve` }))
    await waitFor(() => screen.getByTestId(`device-code-approved`))
    expect(mocks.device).toHaveBeenCalledWith({ query: { user_code: `ZP3H-V7HK` } })
    expect(mocks.device.approve).toHaveBeenCalledWith({ userCode: `ZP3H-V7HK` })
  })

  it(`shows the device-code error and approves nothing`, async () => {
    mocks.device.mockResolvedValue({ data: null, error: { error: `expired_token` } })
    render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Copy install command` }))
    fireEvent.change(
      screen.getByLabelText(`If the CLI shows a code, enter it here`),
      { target: { value: `ZP3H-V7HK` } }
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Approve` }))
    await waitFor(() =>
      screen.getByText(
        `That code has expired. Run the login command again to get a new one.`
      )
    )
    expect(mocks.device.approve).not.toHaveBeenCalled()
  })

  it(`mints nothing while closed`, () => {
    render(
      <AddDeviceDialog
        open={false}
        onOpenChange={() => {}}
        devices={[]}
        origin={ORIGIN}
      />
    )
    expect(mocks.createInstallToken).not.toHaveBeenCalled()
  })
})
