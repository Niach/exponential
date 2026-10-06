import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { PasskeysSection } from "@/components/account/passkeys-section"

// EXP-1209: the last passkey (waysIn <= 1) is blocked up front, with the
// same "your only way to sign in" label the provider rows use.

const mockState = vi.hoisted(() => ({ listUserPasskeys: vi.fn() }))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: {
    passkey: {
      listUserPasskeys: mockState.listUserPasskeys,
      addPasskey: vi.fn(),
    },
  },
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { users: { deletePasskey: { mutate: vi.fn() } } },
}))

beforeEach(() => {
  mockState.listUserPasskeys.mockReset()
  mockState.listUserPasskeys.mockResolvedValue({
    data: [
      {
        id: `pk1`,
        name: `Mac`,
        backedUp: true,
        createdAt: `2026-09-02T00:00:00.000Z`,
      },
    ],
    error: null,
  })
})

describe(`PasskeysSection last way in`, () => {
  it(`disables Remove and says so when the passkey is the only way in`, async () => {
    render(<PasskeysSection passkeyEnabled waysIn={1} />)
    const remove = await screen.findByRole(`button`, { name: `Remove` })
    expect((remove as HTMLButtonElement).disabled).toBe(true)
    expect(screen.getByText(/your only way to sign in/)).toBeTruthy()
    expect(screen.queryByText(/synced across your devices/)).toBeNull()
  })

  it(`keeps Remove enabled while another way in remains`, async () => {
    render(<PasskeysSection passkeyEnabled waysIn={2} />)
    const remove = await screen.findByRole(`button`, { name: `Remove` })
    expect((remove as HTMLButtonElement).disabled).toBe(false)
    expect(screen.getByText(/synced across your devices/)).toBeTruthy()
    expect(screen.queryByText(/your only way to sign in/)).toBeNull()
  })
})
