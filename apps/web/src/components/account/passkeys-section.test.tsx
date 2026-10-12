import { render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { PasskeysSection } from "@/components/account/passkeys-section"

// EXP-1209: the last passkey (waysIn <= 1) is blocked up front, with the
// same "your only way to sign in" label the provider rows use.

const mockState = vi.hoisted(() => ({
  listUserPasskeys: vi.fn(),
  toastError: vi.fn(),
}))

// FEED-79: a failed load is a toast over the empty state, never a rose line.
vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: vi.fn(), error: mockState.toastError },
}))

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
  mockState.toastError.mockReset()
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

describe(`PasskeysSection load failure`, () => {
  it(`toasts once with Retry and draws the empty state`, async () => {
    mockState.listUserPasskeys.mockResolvedValue({
      data: null,
      error: { message: `Server unavailable` },
    })
    render(<PasskeysSection passkeyEnabled waysIn={2} />)
    await waitFor(() => expect(mockState.toastError).toHaveBeenCalledTimes(1))
    const [title, options] = mockState.toastError.mock.calls[0]
    expect(title).toBe(`Couldn't load your passkeys`)
    expect(options.description).toBe(`Server unavailable`)
    expect(options.action?.label).toBeTruthy()
    expect(screen.queryByRole(`button`, { name: `Remove` })).toBeNull()
    expect(screen.queryByRole(`alert`)).toBeNull()
  })
})
