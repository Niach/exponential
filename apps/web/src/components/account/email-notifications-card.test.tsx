import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { EmailNotificationsCard } from "@/components/account/email-notifications-card"

// EXP-452 ("digest at 10 AM instead of the 8 PM I set"). The send hour is read
// in the ACCOUNT's timezone and an account that never had one captured is
// scheduled in UTC — a silent multi-hour shift whose only symptom was mail
// landing at the wrong time, because the panel showed the bare hour and
// nothing else. These lock what the panel now has to say.

const mockState = vi.hoisted(() => ({
  updateEmailPrefs: vi.fn(),
  setTimezone: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    notifications: {
      updateEmailPrefs: { mutate: mockState.updateEmailPrefs },
    },
    users: { setTimezone: { mutate: mockState.setTimezone } },
  },
}))

vi.mock(`@/lib/auth/client`, () => ({
  authClient: { sendVerificationEmail: vi.fn() },
}))

const prefs = (overrides: Record<string, unknown> = {}) =>
  ({
    emailEnabled: true,
    typePrefs: {},
    digest: `daily`,
    digestHour: 8,
    transportConfigured: true,
    emailVerified: true,
    email: `a@example.com`,
    timezone: `Europe/Berlin`,
    nextDigestAt: null,
    ...overrides,
  }) as never

beforeEach(() => {
  mockState.updateEmailPrefs.mockReset().mockResolvedValue({})
  mockState.setTimezone.mockReset().mockResolvedValue({ saved: true })
})

describe(`EmailNotificationsCard send time (EXP-452)`, () => {
  it(`names the zone the send hour is read in`, () => {
    render(
      <EmailNotificationsCard
        emailPrefs={prefs()}
        verifyCallbackPath="/settings"
      />
    )
    expect(screen.getByText(/Europe\/Berlin/)).toBeTruthy()
    // A configured account gets no warning.
    expect(screen.queryByText(/timezone isn't set/i)).toBeNull()
  })

  it(`shows the resolved next send point, not just the bare hour`, () => {
    render(
      <EmailNotificationsCard
        emailPrefs={prefs({ digestHour: 20 })}
        verifyCallbackPath="/settings"
      />
    )
    // 20:00 in Berlin — the schedule the reporter set.
    expect(screen.getByText(/Next digest .*20:00/)).toBeTruthy()
  })

  it(`warns that an unset zone is scheduled in UTC`, () => {
    render(
      <EmailNotificationsCard
        emailPrefs={prefs({ timezone: null })}
        verifyCallbackPath="/settings"
      />
    )
    expect(screen.getByText(/timezone isn't set/i)).toBeTruthy()
    expect(screen.getByText(/read in UTC/i)).toBeTruthy()
  })

  it(`claims the browser's zone explicitly, then drops the warning`, async () => {
    const zone = Intl.DateTimeFormat().resolvedOptions().timeZone
    render(
      <EmailNotificationsCard
        emailPrefs={prefs({ timezone: null })}
        verifyCallbackPath="/settings"
      />
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Use ${zone}` }))
    // Explicit pick → NO onlyIfUnset, which would no-op on a set column.
    await waitFor(() =>
      expect(mockState.setTimezone).toHaveBeenCalledWith({ timezone: zone })
    )
    await waitFor(() =>
      expect(screen.queryByText(/timezone isn't set/i)).toBeNull()
    )
  })

  it(`has no send point row on the hourly cadence`, () => {
    render(
      <EmailNotificationsCard
        emailPrefs={prefs({ digest: `off`, timezone: null })}
        verifyCallbackPath="/settings"
      />
    )
    expect(screen.queryByText(/Next digest/)).toBeNull()
    expect(screen.queryByText(/timezone isn't set/i)).toBeNull()
  })
})
