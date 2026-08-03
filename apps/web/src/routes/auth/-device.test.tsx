// Tests for the /auth/device verification page (EXP-403 CLI device-code
// login). Lives under a `-` prefix so the route generator ignores it.
import { describe, expect, it, vi, beforeEach } from "vitest"
import { render, screen, fireEvent, waitFor } from "@testing-library/react"

const mocks = vi.hoisted(() => {
  const device = Object.assign(vi.fn(), {
    approve: vi.fn(),
    deny: vi.fn(),
  })
  return { device, fetchSessionOnce: vi.fn() }
})

vi.mock(`@/lib/auth/client`, () => ({
  authClient: { device: mocks.device },
  fetchSessionOnce: mocks.fetchSessionOnce,
}))

import { DeviceVerificationView, normalizeUserCode } from "@/routes/auth/device"

// Better Auth resolves to `{ data, error }` — it does not throw.
const resolved = (data: unknown, error: unknown = null) =>
  Promise.resolve({ data, error })

beforeEach(() => {
  mocks.device.mockReset()
  mocks.device.approve.mockReset()
  mocks.device.deny.mockReset()
})

describe(`normalizeUserCode`, () => {
  it(`uppercases and strips anything outside the code charset`, () => {
    expect(normalizeUserCode(`abcd-1234`)).toBe(`ABCD-1234`)
    expect(normalizeUserCode(` ab cd `)).toBe(`ABCD`)
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

    await waitFor(() =>
      expect(mocks.device.deny).toHaveBeenCalledWith({ userCode: `ABCD1234` })
    )
    expect(await screen.findByText(`Request denied.`)).toBeTruthy()
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
    // Still on the entry step — no approve button appeared.
    expect(screen.queryByRole(`button`, { name: `Approve` })).toBeNull()
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
