import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TeamRepositoriesSection } from "@/components/team/repositories-section"

// EXP-365 regressions under test: picking a repo row must only SELECT it (the
// footer button connects), a failed add must stay visible inside the open
// dialog, and the status line must keep the account list (with its unlink ✕)
// visible alongside — never replaced by — the named reconnect warning.

const mockState = vi.hoisted(() => ({
  listQuery: vi.fn(),
  statusQuery: vi.fn(),
  reposQuery: vi.fn(),
  addMutate: vi.fn(),
  unlinkMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    repositories: {
      list: { query: mockState.listQuery },
      add: { mutate: mockState.addMutate },
      remove: { mutate: vi.fn() },
    },
    integrations: {
      github: {
        status: { query: mockState.statusQuery },
        repos: { query: mockState.reposQuery },
        unlink: { mutate: mockState.unlinkMutate },
      },
    },
  },
}))

vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => ({ teamSlug: `acme` }),
  Link: ({ children }: { children?: React.ReactNode }) => <a>{children}</a>,
}))

const installation = (overrides: Record<string, unknown> = {}) => ({
  installationId: 1,
  accountLogin: `siteviewer-app`,
  accountType: `Organization`,
  manageUrl: `https://github.com/settings/installations/1`,
  suspended: false,
  needsReauth: false,
  ...overrides,
})

const githubStatus = (installations: Array<Record<string, unknown>>) => ({
  configured: true as const,
  installed: true,
  installUrl: `https://github.com/apps/test/installations/new`,
  connectUrl: `https://github.com/login/oauth/authorize?x=1`,
  accounts: installations
    .map((inst) => inst.accountLogin)
    .filter(Boolean) as string[],
  installations,
})

const githubRepos = (installations: Array<Record<string, unknown>>) => ({
  ...githubStatus(installations),
  repos: [
    {
      fullName: `siteviewer-app/app`,
      private: true,
      defaultBranch: `main`,
      installationId: 1,
    },
    {
      fullName: `siteviewer-app/website-siteviewer`,
      private: true,
      defaultBranch: `main`,
      installationId: 1,
    },
  ],
  hasMore: false,
})

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

function renderSection() {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  return render(<TeamRepositoriesSection teamId="team-1" />)
}

describe(`TeamRepositoriesSection`, () => {
  beforeEach(() => {
    mockState.listQuery.mockReset().mockResolvedValue([])
    mockState.statusQuery
      .mockReset()
      .mockResolvedValue(githubStatus([installation()]))
    mockState.reposQuery
      .mockReset()
      .mockResolvedValue(githubRepos([installation()]))
    mockState.addMutate.mockReset().mockResolvedValue({ repository: {} })
    mockState.unlinkMutate.mockReset().mockResolvedValue({})
  })

  it(`selecting a row does NOT connect — the footer Add button does`, async () => {
    renderSection()
    fireEvent.click(
      await screen.findByRole(`button`, { name: /Add repository/ })
    )

    const row = await screen.findByText(`siteviewer-app/app`)
    fireEvent.click(row)
    expect(mockState.addMutate).not.toHaveBeenCalled()

    // The footer button (inside the dialog) fires the mutation.
    const buttons = screen.getAllByRole(`button`, { name: /Add repository/ })
    fireEvent.click(buttons[buttons.length - 1])
    await waitFor(() => expect(mockState.addMutate).toHaveBeenCalledTimes(1))
    expect(mockState.addMutate.mock.calls[0][0]).toMatchObject({
      teamId: `team-1`,
      fullName: `siteviewer-app/app`,
    })
  })

  it(`a failed add keeps the dialog open with the error inside it`, async () => {
    mockState.addMutate.mockRejectedValue(
      new Error(`You don't have access to siteviewer-app/app on GitHub.`)
    )
    renderSection()
    fireEvent.click(
      await screen.findByRole(`button`, { name: /Add repository/ })
    )

    fireEvent.click(await screen.findByText(`siteviewer-app/app`))
    const buttons = screen.getAllByRole(`button`, { name: /Add repository/ })
    fireEvent.click(buttons[buttons.length - 1])

    // Error renders, and the dialog (its search input) is still up.
    await screen.findByText(/You don't have access/)
    expect(screen.getByPlaceholderText(`Search repositories…`)).toBeTruthy()
  })

  it(`needsReauth keeps the account list + unlink visible and names the stale account`, async () => {
    const stale = installation({
      installationId: 2,
      accountLogin: `Niach`,
      needsReauth: true,
    })
    mockState.statusQuery.mockResolvedValue(
      githubStatus([installation(), stale])
    )
    renderSection()

    // Both accounts stay listed (the unlink ✕ lives on this line)…
    await screen.findByText(/siteviewer-app/)
    expect(screen.getAllByText(/Niach/).length).toBeGreaterThan(0)
    expect(
      screen.getAllByTitle(`Disconnect this GitHub account from the team`)
    ).toHaveLength(2)
    // …and the warning names the offending account.
    expect(
      screen.getByText(/which repositories you can access.*from Niach/)
    ).toBeTruthy()
    expect(screen.getByRole(`button`, { name: `Reconnect` })).toBeTruthy()
  })

  it(`suspension outranks reconnect and never offers it`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([
        installation({ suspended: true, needsReauth: false }),
        installation({
          installationId: 2,
          accountLogin: `Niach`,
          needsReauth: true,
        }),
      ])
    )
    renderSection()

    await screen.findByText(/GitHub suspended the Exponential app/)
    expect(screen.queryByRole(`button`, { name: `Reconnect` })).toBeNull()
  })
})
