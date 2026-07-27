import {
  act,
  fireEvent,
  render,
  renderHook,
  screen,
} from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  APPLE_PROVIDER_KEY,
  GOOGLE_PROVIDER_KEY,
  OAuthProviderButtons,
  useOAuthSignIn,
} from "@/components/oauth-provider-buttons"

const mockState = vi.hoisted(() => ({
  social: vi.fn(),
  oauth2: vi.fn(),
}))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: {
    signIn: {
      social: mockState.social,
      oauth2: mockState.oauth2,
    },
  },
}))

// Better Auth resolves to `{ data, error }` — it does not throw on a failed
// OAuth start, which is exactly what the handlers have to notice.
const resolved = (error: { code?: string; message?: string } | null) =>
  Promise.resolve({
    data: error ? null : { url: `https://idp.test/auth` },
    error,
  })

function Harness() {
  const {
    pendingProvider,
    error,
    signInWithOidc,
    signInWithGoogle,
    signInWithApple,
  } = useOAuthSignIn(undefined)
  return (
    <>
      <OAuthProviderButtons
        oidcProviders={[{ id: `authentik`, name: `Authentik` }]}
        googleLoginEnabled
        appleLoginEnabled
        verb="Sign in"
        pendingProvider={pendingProvider}
        showDivider={false}
        onOidc={signInWithOidc}
        onGoogle={signInWithGoogle}
        onApple={signInWithApple}
      />
      {error && <p data-testid="error">{error}</p>}
    </>
  )
}

const button = (name: string) =>
  screen.getByRole(`button`, { name }) as HTMLButtonElement

describe(`useOAuthSignIn`, () => {
  beforeEach(() => {
    mockState.social.mockReset()
    mockState.oauth2.mockReset()
  })

  it(`surfaces a resolved Google error and clears the pending provider`, async () => {
    mockState.social.mockReturnValue(
      resolved({ code: `PROVIDER_NOT_FOUND`, message: `Provider not found` })
    )
    const { result } = renderHook(() => useOAuthSignIn(`/t/acme`))

    await act(async () => {
      await result.current.signInWithGoogle()
    })

    expect(mockState.social).toHaveBeenCalledWith({
      provider: `google`,
      callbackURL: `/t/acme`,
    })
    expect(result.current.error).toBe(`Provider not found`)
    expect(result.current.pendingProvider).toBeNull()
  })

  it(`falls back to a generic message when the error carries none`, async () => {
    mockState.social.mockReturnValue(resolved({ code: `UNKNOWN` }))
    const { result } = renderHook(() => useOAuthSignIn(undefined))

    await act(async () => {
      await result.current.signInWithApple()
    })

    expect(mockState.social).toHaveBeenCalledWith({
      provider: `apple`,
      callbackURL: `/`,
    })
    expect(result.current.error).toBe(`Couldn't sign you in. Try again.`)
    expect(result.current.pendingProvider).toBeNull()
  })

  it(`surfaces a resolved OIDC error`, async () => {
    mockState.oauth2.mockReturnValue(
      resolved({ code: `RATE_LIMITED`, message: `Too many requests` })
    )
    const { result } = renderHook(() => useOAuthSignIn(undefined))

    await act(async () => {
      await result.current.signInWithOidc(`authentik`)
    })

    expect(mockState.oauth2).toHaveBeenCalledWith({
      providerId: `authentik`,
      callbackURL: `/`,
    })
    expect(result.current.error).toBe(`Too many requests`)
    expect(result.current.pendingProvider).toBeNull()
  })

  it(`still handles a rejected start`, async () => {
    mockState.social.mockRejectedValue(new Error(`offline`))
    const { result } = renderHook(() => useOAuthSignIn(undefined))

    await act(async () => {
      await result.current.signInWithGoogle()
    })

    expect(result.current.error).toBe(`An unexpected error occurred`)
    expect(result.current.pendingProvider).toBeNull()
  })

  it(`keeps the pending provider while a successful redirect is in flight`, async () => {
    mockState.social.mockReturnValue(resolved(null))
    const { result } = renderHook(() => useOAuthSignIn(undefined))

    await act(async () => {
      await result.current.signInWithGoogle()
    })

    expect(result.current.error).toBe(``)
    expect(result.current.pendingProvider).toBe(GOOGLE_PROVIDER_KEY)
  })

  it(`exposes distinct keys for the built-in providers`, () => {
    expect(GOOGLE_PROVIDER_KEY).not.toBe(APPLE_PROVIDER_KEY)
  })
})

describe(`OAuthProviderButtons`, () => {
  beforeEach(() => {
    mockState.social.mockReset()
    mockState.oauth2.mockReset()
  })

  it(`re-enables every provider button after a failed start`, async () => {
    mockState.social.mockReturnValue(
      resolved({ code: `PROVIDER_NOT_FOUND`, message: `Provider not found` })
    )
    render(<Harness />)

    await act(async () => {
      fireEvent.click(button(`Sign in with Google`))
    })

    expect(screen.getByTestId(`error`).textContent).toBe(`Provider not found`)
    expect(screen.queryByText(`Redirecting...`)).toBeNull()
    expect(button(`Sign in with Google`).disabled).toBe(false)
    expect(button(`Sign in with Apple`).disabled).toBe(false)
    expect(button(`Sign in with Authentik`).disabled).toBe(false)
  })
})
