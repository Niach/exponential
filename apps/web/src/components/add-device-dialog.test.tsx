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
  newlyOnlineDevice,
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

  it(`a new device = an unseen id, or one offline when the dialog opened`, () => {
    const baseline = new Map([
      [`a`, true],
      [`b`, false],
    ])
    expect(newlyOnlineDevice(baseline, [dev(`a`, true)])).toBeNull()
    expect(newlyOnlineDevice(baseline, [dev(`a`, true), dev(`b`, true)])?.deviceId).toBe(`b`)
    expect(newlyOnlineDevice(baseline, [dev(`c`, true)])?.deviceId).toBe(`c`)
    expect(newlyOnlineDevice(baseline, [dev(`c`, false)])).toBeNull()
  })
})

describe(`AddDeviceDialog`, () => {
  it(`mints a token, approves a typed code and waits for the new machine`, async () => {
    const { rerender } = render(
      <AddDeviceDialog
        open
        onOpenChange={() => {}}
        devices={[dev(`a`, true)]}
        origin={ORIGIN}
      />
    )
    await waitFor(() =>
      expect(screen.getByTestId(`install-snippet-token`).textContent).toContain(
        `EXP_INSTALL_TOKEN=${TOKEN}`
      )
    )
    expect(screen.getByTestId(`install-snippet-plain`).textContent).not.toContain(
      `EXP_INSTALL_TOKEN`
    )
    expect(mocks.createInstallToken).toHaveBeenCalledTimes(1)

    fireEvent.change(screen.getByLabelText(`Or enter the code the CLI shows`), {
      target: { value: `zp3hv7hk` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Approve` }))
    await waitFor(() => screen.getByTestId(`device-code-approved`))
    expect(mocks.device).toHaveBeenCalledWith({ query: { user_code: `ZP3H-V7HK` } })
    expect(mocks.device.approve).toHaveBeenCalledWith({ userCode: `ZP3H-V7HK` })
    expect(screen.getByText(`Waiting for the device to come online…`)).toBeTruthy()

    rerender(
      <AddDeviceDialog
        open
        onOpenChange={() => {}}
        devices={[dev(`a`, true), dev(`n`, true)]}
        origin={ORIGIN}
      />
    )
    expect(screen.getByTestId(`add-device-online`).textContent).toBe(
      `Box n is online`
    )
  })

  it(`shows the device-code error and keeps the plain command on a mint failure`, async () => {
    mocks.createInstallToken.mockRejectedValue(new Error(`nope`))
    mocks.device.mockResolvedValue({ data: null, error: { error: `expired_token` } })
    render(
      <AddDeviceDialog open onOpenChange={() => {}} devices={[]} origin={ORIGIN} />
    )
    await waitFor(() =>
      screen.getByText(`Couldn't create a one-time install command.`)
    )
    expect(screen.getByTestId(`install-snippet-plain`)).toBeTruthy()
    fireEvent.change(screen.getByLabelText(`Or enter the code the CLI shows`), {
      target: { value: `ZP3H-V7HK` },
    })
    fireEvent.click(screen.getByRole(`button`, { name: `Approve` }))
    await waitFor(() =>
      screen.getByText(
        `That code has expired. Run the login command again to get a new one.`
      )
    )
    expect(mocks.device.approve).not.toHaveBeenCalled()
  })
})
