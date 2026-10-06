import { StrictMode } from "react"
import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { SignInMethodsSection } from "@/components/account/sign-in-methods-section"
import type { SignInMethods } from "@/lib/auth/sign-in-methods"

// EXP-1031: the OAuth-return outcome (`?linked=` / `?link_error=`) is a
// toast now, not an inline notice: fired once from the mount effect (the
// root's Toaster is already subscribed, see root-toaster-order.test.tsx),
// then the params are stripped.

const mockState = vi.hoisted(() => ({
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}))

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: mockState.toastSuccess, error: mockState.toastError },
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    users: {
      signInMethods: { query: vi.fn() },
      unlinkSignInMethod: { mutate: vi.fn() },
    },
  },
}))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: { oauth2: { link: vi.fn() }, linkSocial: vi.fn() },
  invalidateSessionCache: vi.fn(),
}))

vi.mock(`@/components/account/change-email-dialog`, () => ({
  ChangeEmailDialog: () => null,
}))

const methods: SignInMethods = {
  email: `danny@example.com`,
  emailVerified: true,
  emailOtpEnabled: true,
  passwordEnabled: false,
  passkeyEnabled: false,
  providers: [
    {
      id: `google`,
      name: `Google`,
      kind: `google`,
      available: true,
      linked: true,
      linkedAt: null,
    },
  ],
  passkeys: [],
  waysIn: 2,
}

function renderSection(linkReturn: { linked?: string; linkError?: string }) {
  const consumed = vi.fn()
  render(
    <StrictMode>
      <SignInMethodsSection
        initialMethods={methods}
        teamSlug="acme"
        linkReturn={linkReturn}
        onLinkReturnConsumed={consumed}
      />
    </StrictMode>
  )
  return consumed
}

beforeEach(() => {
  mockState.toastSuccess.mockReset()
  mockState.toastError.mockReset()
})

describe(`SignInMethodsSection link return`, () => {
  it(`toasts a successful link once and consumes the params`, () => {
    const consumed = renderSection({ linked: `google` })
    expect(mockState.toastSuccess).toHaveBeenCalledTimes(1)
    expect(mockState.toastSuccess).toHaveBeenCalledWith(
      `Google linked to your account.`
    )
    expect(mockState.toastError).not.toHaveBeenCalled()
    expect(consumed).toHaveBeenCalled()
    expect(screen.queryByRole(`status`)).toBeNull()
  })

  it(`toasts a failed link as an error`, () => {
    renderSection({ linkError: `access_denied` })
    expect(mockState.toastError).toHaveBeenCalledTimes(1)
    expect(mockState.toastError).toHaveBeenCalledWith(`Linking was cancelled.`)
    expect(mockState.toastSuccess).not.toHaveBeenCalled()
  })

  it(`toasts nothing on a plain visit`, () => {
    const consumed = renderSection({})
    expect(mockState.toastSuccess).not.toHaveBeenCalled()
    expect(mockState.toastError).not.toHaveBeenCalled()
    expect(consumed).not.toHaveBeenCalled()
  })
})

describe(`SignInMethodsSection last way in`, () => {
  const only = (provider: Partial<SignInMethods["providers"][number]>) => (
    <SignInMethodsSection
      initialMethods={{
        ...methods,
        providers: [{ ...methods.providers[0], ...provider }],
        waysIn: provider.available === false ? 0 : 1,
      }}
      teamSlug="acme"
      linkReturn={{}}
      onLinkReturnConsumed={vi.fn()}
    />
  )

  it(`blocks unlinking the only way in`, () => {
    render(only({}))
    const unlink = screen.getByRole(`button`, { name: `Unlink` }) as HTMLButtonElement
    expect(unlink.disabled).toBe(true)
    expect(screen.getByText(`Linked · your only way to sign in`)).toBeTruthy()
  })

  it(`lets a row that is no way in go (EXP-1209)`, () => {
    render(only({ available: false }))
    const unlink = screen.getByRole(`button`, { name: `Unlink` }) as HTMLButtonElement
    expect(unlink.disabled).toBe(false)
    expect(
      screen.getByText(`Linked · no longer offered on this instance`)
    ).toBeTruthy()
  })
})
