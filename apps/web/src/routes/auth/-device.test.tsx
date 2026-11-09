// Tests for the /auth/device verification page (EXP-403 CLI device-code
// login). Lives under a `-` prefix so the route generator ignores it.
import { describe, expect, it, vi, beforeEach } from "vitest"
import { render, screen, fireEvent, waitFor } from "@testing-library/react"

const mocks = vi.hoisted(() => {
  const device = Object.assign(vi.fn(), {
    approve: vi.fn(),
    deny: vi.fn(),
  })
  return {
    device,
    fetchSessionOnce: vi.fn(),
    toastError: vi.fn(),
    pendingLogin: vi.fn(),
  }
})

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: vi.fn(), error: mocks.toastError },
}))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: { device: mocks.device },
  fetchSessionOnce: mocks.fetchSessionOnce,
}))

// GHSA-q84f-53jg-9ppm: the asking client comes from the server's
// `device_codes` lookup, never the URL.
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { devices: { pendingLogin: { query: mocks.pendingLogin } } },
}))

import { DeviceVerificationView, normalizeUserCode } from "@/routes/auth/device"

// Better Auth resolves to `{ data, error }` — it does not throw.
const resolved = (data: unknown, error: unknown = null) =>
  Promise.resolve({ data, error })

beforeEach(() => {
  mocks.toastError.mockReset()
  mocks.device.mockReset()
  mocks.device.approve.mockReset()
  mocks.device.deny.mockReset()
  mocks.pendingLogin.mockReset()
  mocks.pendingLogin.mockResolvedValue({ clientName: `Exponential desktop or CLI` })
})

describe(`normalizeUserCode`, () => {
  it(`uppercases and strips anything outside the code charset`, () => {
    expect(normalizeUserCode(`abcd-1234`)).toBe(`ABCD-1234`)
    expect(normalizeUserCode(` ab cd `)).toBe(`ABCD`)
  })

  it(`auto-inserts the dash once a 5th char exists`, () => {
    // Typing: no dash through 4 chars (so backspacing over the dash
    // deletes instead of fighting the formatter), then it appears.
    expect(normalizeUserCode(`ABCD`)).toBe(`ABCD`)
    expect(normalizeUserCode(`ABCD1`)).toBe(`ABCD-1`)
    expect(normalizeUserCode(`abcd1234`)).toBe(`ABCD-1234`)
    // Pasting the printed form stays stable, and overflow is capped.
    expect(normalizeUserCode(`ABCD-1234`)).toBe(`ABCD-1234`)
    expect(normalizeUserCode(`ABCD-1234XYZ`)).toBe(`ABCD-1234`)
  })
})

describe(`DeviceVerificationView`, () => {
  it(`claims the code via GET /device, then approves`, async () => {
    mocks.device.mockReturnValue(
      resolved({ user_code: `ABCD1234`, status: `pending` })
    )
    mocks.device.approve.mockReturnValue(resolved({ success: true }))

    render(<DeviceVerificationView initialCode="abcd-1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    await waitFor(() =>
      expect(mocks.device).toHaveBeenCalledWith({
        query: { user_code: `ABCD-1234` },
      })
    )
    fireEvent.click(await screen.findByRole(`button`, { name: `Approve` }))
    await waitFor(() =>
      expect(mocks.device.approve).toHaveBeenCalledWith({
        userCode: `ABCD-1234`,
      })
    )
    expect(await screen.findByText(`Device connected.`)).toBeTruthy()
  })

  it(`deny reports the device was not signed in`, async () => {
    mocks.device.mockReturnValue(
      resolved({ user_code: `ABCD1234`, status: `pending` })
    )
    mocks.device.deny.mockReturnValue(resolved({ success: true }))

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))
    fireEvent.click(await screen.findByRole(`button`, { name: `Deny` }))

    // The formatter dashes the prefilled bare code; the server strips it.
    await waitFor(() =>
      expect(mocks.device.deny).toHaveBeenCalledWith({ userCode: `ABCD-1234` })
    )
    expect(await screen.findByText(`Request denied.`)).toBeTruthy()
  })

  it(`reports settling so the page can drop its header`, async () => {
    mocks.device.mockReturnValue(
      resolved({ user_code: `ABCD1234`, status: `pending` })
    )
    mocks.device.approve.mockReturnValue(resolved({ success: true }))
    const onSettled = vi.fn()

    render(
      <DeviceVerificationView initialCode="ABCD1234" onSettled={onSettled} />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))
    fireEvent.click(await screen.findByRole(`button`, { name: `Approve` }))

    await screen.findByText(`Device connected.`)
    expect(onSettled).toHaveBeenCalledTimes(1)
  })

  it(`renders the expired-code error from the claim probe`, async () => {
    mocks.device.mockReturnValue(
      resolved(null, { error: `expired_token`, error_description: `expired` })
    )

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    expect(
      await screen.findByText(
        `That code has expired. Run the login command again to get a new one.`
      )
    ).toBeTruthy()
    // Still on the entry step — no approve button appeared. The refusal is
    // about the typed code: inline under the field (FEED-79), not a toast.
    expect(screen.queryByRole(`button`, { name: `Approve` })).toBeNull()
    expect(screen.getByRole(`alert`).textContent).toContain(`expired`)
    expect(
      screen.getByLabelText(`Code`).getAttribute(`aria-invalid`)
    ).toBe(`true`)
    expect(mocks.toastError).not.toHaveBeenCalled()
  })

  it(`treats an already-decided code as spent`, async () => {
    mocks.device.mockReturnValue(
      resolved({ user_code: `ABCD1234`, status: `approved` })
    )

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    expect(
      await screen.findByText(
        `That code has already been used. Run the login command again.`
      )
    ).toBeTruthy()
  })
})

describe(`DeviceVerificationView approve failure`, () => {
  it(`toasts a failed approve and stays on the confirm step`, async () => {
    mocks.device.mockReturnValue(
      resolved({ user_code: `ABCD1234`, status: `pending` })
    )
    mocks.device.approve.mockReturnValue(
      resolved(null, { error: `expired_token`, error_description: `expired` })
    )

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))
    fireEvent.click(await screen.findByRole(`button`, { name: `Approve` }))

    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1))
    expect(mocks.toastError.mock.calls[0][0]).toBe(`Couldn't approve the device`)
    expect(screen.queryByText(`Device connected.`)).toBeNull()
    expect(screen.getByRole(`button`, { name: `Approve` })).toBeTruthy()
  })
})

describe(`DeviceVerificationView names the asking client (GHSA-q84f-53jg-9ppm)`, () => {
  it(`shows the server-checked client, what it gets and the code before Approve`, async () => {
    mocks.device.mockReturnValue(resolved({ user_code: `ABCD1234`, status: `pending` }))
    mocks.pendingLogin.mockResolvedValue({ clientName: `Server Named Client` })

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    await screen.findByRole(`button`, { name: `Approve` })
    expect(mocks.pendingLogin).toHaveBeenCalledWith({ userCode: `ABCD-1234` })
    expect(screen.getByText(`Server Named Client`)).toBeTruthy()
    expect(screen.getByText(/full session as you/)).toBeTruthy()
    expect(screen.getByText(`ABCD-1234`)).toBeTruthy()
  })

  it(`refuses a code whose client is outside the allow-list`, async () => {
    mocks.device.mockReturnValue(resolved({ user_code: `ABCD1234`, status: `pending` }))
    mocks.pendingLogin.mockResolvedValue({ clientName: null })

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    expect(
      await screen.findByText(`That code isn't valid. Check for typos, or run the login command again.`)
    ).toBeTruthy()
    expect(screen.queryByRole(`button`, { name: `Approve` })).toBeNull()
  })

  it(`a code another account claimed never reaches Approve`, async () => {
    mocks.device.mockReturnValue(resolved({ user_code: `ABCD1234`, status: `pending` }))
    mocks.pendingLogin.mockRejectedValue({ data: { code: `NOT_FOUND` } })

    render(<DeviceVerificationView initialCode="ABCD1234" />)
    fireEvent.click(screen.getByRole(`button`, { name: `Continue` }))

    expect(await screen.findByText(`This code was requested from a different account.`)).toBeTruthy()
    expect(screen.queryByRole(`button`, { name: `Approve` })).toBeNull()
  })
})
