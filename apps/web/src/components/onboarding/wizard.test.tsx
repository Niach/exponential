import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen } from "@testing-library/react"

const signOut = vi.fn()
vi.mock(`@/hooks/use-sign-out`, () => ({ useSignOut: () => signOut }))
vi.mock(`@/hooks/use-session`, () => ({
  useSession: () => ({ data: { user: { email: `who@example.com` } } }),
}))
vi.mock(`@tanstack/react-router`, () => ({ useNavigate: () => vi.fn() }))

import { OnboardingWizard } from "./wizard"

// A team-less user has no sidebar and so no user menu: the wizard itself
// must carry the sign-out escape (the phones' persistent one), on every step.
describe(`OnboardingWizard sign-out escape`, () => {
  it(`names the signed-in account and signs out from the choice step`, () => {
    render(<OnboardingWizard initialTeam={null} />)
    expect(screen.getByText(`Welcome to Exponential`)).toBeTruthy()
    expect(screen.getByTestId(`onboarding-signed-in-as`).textContent).toBe(
      `Signed in as who@example.com`
    )
    fireEvent.click(screen.getByRole(`button`, { name: /Sign out/ }))
    expect(signOut).toHaveBeenCalledTimes(1)
  })

  it(`keeps the escape on the join step`, () => {
    render(<OnboardingWizard initialTeam={null} />)
    fireEvent.click(screen.getByRole(`button`, { name: /Join a team/ }))
    expect(screen.getByText(`Join a team`, { selector: `h2` })).toBeTruthy()
    expect(screen.getByRole(`button`, { name: /Sign out/ })).toBeTruthy()
  })
})
