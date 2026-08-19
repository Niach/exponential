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
  listBranchesQuery: vi.fn(),
  setDefaultBranchMutate: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    repositories: {
      list: { query: mockState.listQuery },
      add: { mutate: mockState.addMutate },
      remove: { mutate: vi.fn() },
      listBranches: { query: mockState.listBranchesQuery },
      setDefaultBranch: { mutate: mockState.setDefaultBranchMutate },
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
  stale: false,
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

function renderSection(
  props: { currentUserId?: string; isOwner?: boolean } = {}
) {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  return render(
    <TeamRepositoriesSection
      teamId="team-1"
      currentUserId={props.currentUserId ?? `user-1`}
      isOwner={props.isOwner ?? true}
    />
  )
}

const repoRow = (overrides: Record<string, unknown> = {}) => ({
  id: `repo-1`,
  teamId: `team-1`,
  fullName: `siteviewer-app/app`,
  defaultBranch: `master`,
  githubDefaultBranch: `master`,
  defaultBranchOverride: null,
  private: false,
  installationId: 1,
  inaccessibleAt: null,
  sharedBy: null,
  boards: [],
  ...overrides,
})

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
    mockState.listBranchesQuery
      .mockReset()
      .mockResolvedValue({ branches: [`master`, `develop`, `staging`] })
    mockState.setDefaultBranchMutate
      .mockReset()
      .mockResolvedValue({ repository: {} })
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

  // EXP-462: the row's branch badge is a picker — branches load on open, a
  // non-default pick pins it, and picking GitHub's own default clears the pin.
  // EXP-469: it is a searchable Command popover, so opening happens on click.
  it(`the branch badge picks a default branch (default choice clears the pin)`, async () => {
    mockState.listQuery.mockResolvedValue([repoRow()])
    renderSection()

    const trigger = await screen.findByLabelText(
      `Default branch for siteviewer-app/app`
    )
    fireEvent.click(trigger)

    // Branches were fetched lazily and render behind a search input, the
    // GitHub default labeled.
    await screen.findByText(`develop`)
    expect(mockState.listBranchesQuery).toHaveBeenCalledWith({
      repositoryId: `repo-1`,
    })
    expect(screen.getByPlaceholderText(`Search branches…`)).toBeTruthy()
    expect(screen.getByText(`default`)).toBeTruthy()

    // Typing filters the list (EXP-469).
    fireEvent.change(screen.getByPlaceholderText(`Search branches…`), {
      target: { value: `dev` },
    })
    await waitFor(() => expect(screen.queryByText(`staging`)).toBeNull())

    fireEvent.click(screen.getByText(`develop`))
    await waitFor(() =>
      expect(mockState.setDefaultBranchMutate).toHaveBeenCalledTimes(1)
    )
    expect(mockState.setDefaultBranchMutate.mock.calls[0][0]).toEqual({
      repositoryId: `repo-1`,
      branch: `develop`,
    })
  })

  it(`picking GitHub's own default sends branch: null`, async () => {
    mockState.listQuery.mockResolvedValue([
      repoRow({
        defaultBranch: `develop`,
        defaultBranchOverride: `develop`,
      }),
    ])
    renderSection()

    const trigger = await screen.findByLabelText(
      `Default branch for siteviewer-app/app`
    )
    fireEvent.click(trigger)

    fireEvent.click(await screen.findByText(`master`))
    await waitFor(() =>
      expect(mockState.setDefaultBranchMutate).toHaveBeenCalledTimes(1)
    )
    expect(mockState.setDefaultBranchMutate.mock.calls[0][0]).toEqual({
      repositoryId: `repo-1`,
      branch: null,
    })
  })

  // EXP-557: a stale link (zero grants from anyone) renders a visible
  // Disconnect button behind a confirm dialog, instead of the reconnect nag.
  it(`a stale account offers Disconnect (confirm-first) instead of Reconnect`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([
        installation(),
        installation({
          installationId: 2,
          accountLogin: `exponential-play-review`,
          needsReauth: false,
          stale: true,
        }),
      ])
    )
    renderSection()

    const disconnect = await screen.findByRole(`button`, {
      name: `Disconnect account`,
    })
    expect(screen.queryByRole(`button`, { name: `Reconnect` })).toBeNull()

    fireEvent.click(disconnect)
    // Confirm-first: nothing mutates until the dialog's Disconnect.
    expect(mockState.unlinkMutate).not.toHaveBeenCalled()
    fireEvent.click(await screen.findByRole(`button`, { name: `Disconnect` }))
    await waitFor(() =>
      expect(mockState.unlinkMutate).toHaveBeenCalledTimes(1)
    )
    expect(mockState.unlinkMutate.mock.calls[0][0]).toMatchObject({
      teamId: `team-1`,
      installationId: 2,
    })
  })

  // EXP-557 sharer-or-owner rows: a non-manager sees "Shared by X" but gets
  // neither the remove button nor the branch picker.
  it(`a plain member sees Shared by and no manage affordances on a teammate's repo`, async () => {
    mockState.listQuery.mockResolvedValue([
      repoRow({
        sharedBy: { id: `user-2`, name: `Danny`, email: `d@example.com` },
      }),
    ])
    renderSection({ currentUserId: `user-1`, isOwner: false })

    await screen.findByText(/Shared by Danny/)
    expect(screen.queryByTitle(`Remove repository`)).toBeNull()
    expect(
      screen.queryByLabelText(`Default branch for siteviewer-app/app`)
    ).toBeNull()
    // The branch still renders read-only.
    expect(screen.getAllByText(`master`).length).toBeGreaterThan(0)
  })

  it(`the sharer keeps the manage affordances without being owner`, async () => {
    mockState.listQuery.mockResolvedValue([
      repoRow({
        sharedBy: { id: `user-1`, name: `Me`, email: `me@example.com` },
      }),
    ])
    renderSection({ currentUserId: `user-1`, isOwner: false })

    await screen.findByText(/Shared by Me/)
    expect(screen.getByTitle(`Remove repository`)).toBeTruthy()
    expect(
      screen.getByLabelText(`Default branch for siteviewer-app/app`)
    ).toBeTruthy()
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
