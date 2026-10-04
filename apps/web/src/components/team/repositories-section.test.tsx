import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { TeamRepositoriesSection } from "@/components/team/repositories-section"

// EXP-365/FEED-42/SLOP-7 regressions under test: picking a repo row adds it
// at once (tap adds, ×4) and closes the dialog, a failed add must stay
// visible inside the open dialog, and the connection block renders the
// viewer's own GitHub connection (not linked → Connect GitHub opens the
// guided page; linked → the login, the installed accounts with Configure
// links, Install on another account; expired → Reconnect) with a
// confirm-first Disconnect.

const mockState = vi.hoisted(() => ({
  listQuery: vi.fn(),
  statusQuery: vi.fn(),
  reposQuery: vi.fn(),
  addMutate: vi.fn(),
  unlinkMutate: vi.fn(),
  listBranchesQuery: vi.fn(),
  setDefaultBranchMutate: vi.fn(),
  openGithubConnect: vi.fn(() => true),
  openGithubPopup: vi.fn(() => true),
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
        disconnect: { mutate: mockState.unlinkMutate },
      },
    },
  },
}))
vi.mock(`@/lib/github-connect`, () => ({
  openGithubConnect: mockState.openGithubConnect,
  openGithubPopup: mockState.openGithubPopup,
  POPUP_BLOCKED_MESSAGE: `blocked`,
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

const githubStatus = (
  installations: Array<Record<string, unknown>>,
  overrides: Record<string, unknown> = {}
) => ({
  configured: true as const,
  connectConfigured: true,
  linked: true,
  needsReconnect: false,
  login: `octocat`,
  installed: installations.length > 0,
  installUrl: `https://github.com/apps/test/installations/new`,
  connectUrl: `https://app.example.com/integrations/github?team=team-1`,
  installations,
  ...overrides,
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
    mockState.openGithubConnect.mockClear()
    mockState.openGithubPopup.mockClear()
  })

  it(`tapping a row adds it immediately and closes the dialog`, async () => {
    renderSection()
    fireEvent.click(
      await screen.findByRole(`button`, { name: /Add repository/ })
    )

    fireEvent.click(await screen.findByText(`siteviewer-app/app`))
    await waitFor(() => expect(mockState.addMutate).toHaveBeenCalledTimes(1))
    expect(mockState.addMutate.mock.calls[0][0]).toMatchObject({
      teamId: `team-1`,
      fullName: `siteviewer-app/app`,
    })
    await waitFor(() =>
      expect(screen.queryByPlaceholderText(`Search repositories…`)).toBeNull()
    )
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

    // Error renders, and the dialog (its search input) is still up.
    await screen.findByText(/You don't have access/)
    expect(screen.getByPlaceholderText(`Search repositories…`)).toBeTruthy()
  })

  it(`not linked → Connect GitHub opens the guided page for the team`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([], { linked: false, login: null })
    )
    renderSection()

    await screen.findByText(`No GitHub account connected`)
    fireEvent.click(screen.getByRole(`button`, { name: `Connect GitHub` }))
    expect(mockState.openGithubConnect).toHaveBeenCalledWith({ teamId: `team-1` })
  })

  it(`an expired connection offers Reconnect, never a per-account nag`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([], { needsReconnect: true })
    )
    renderSection()

    await screen.findByText(`Your GitHub connection expired.`)
    fireEvent.click(screen.getByRole(`button`, { name: `Reconnect` }))
    expect(mockState.openGithubConnect).toHaveBeenCalledWith({ teamId: `team-1` })
  })

  it(`Disconnect confirms first, then unlinks the viewer's GitHub account`, async () => {
    renderSection()

    await screen.findByText(`Connected as octocat`)
    fireEvent.click(screen.getByRole(`button`, { name: `Disconnect` }))
    expect(mockState.unlinkMutate).not.toHaveBeenCalled()
    await screen.findByText(
      `This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect.`
    )
    fireEvent.click(screen.getAllByRole(`button`, { name: `Disconnect` }).at(-1)!)
    await waitFor(() =>
      expect(mockState.unlinkMutate).toHaveBeenCalledTimes(1)
    )
  })

  it(`a GitHub-grant FORBIDDEN add shows the reconnect arm inline`, async () => {
    const { TRPCClientError } = await import(`@trpc/client`)
    const err = new TRPCClientError(
      `You need push access to siteviewer-app/app on GitHub to connect it. Reconnect GitHub in team settings.`
    )
    ;(err as { data?: unknown }).data = { code: `FORBIDDEN` }
    mockState.addMutate.mockRejectedValue(err)
    renderSection()
    fireEvent.click(
      await screen.findByRole(`button`, { name: /Add repository/ })
    )
    fireEvent.click(await screen.findByText(`siteviewer-app/app`))

    await screen.findByText(
      `GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again.`
    )
    expect(
      screen.getAllByRole(`button`, { name: `Reconnect GitHub` }).length
    ).toBeGreaterThan(0)
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
  it(`linked but nothing installed → Install the app opens GitHub's install page`, async () => {
    mockState.statusQuery.mockResolvedValue(githubStatus([]))
    renderSection()

    await screen.findByText(
      `The Exponential app isn’t installed on any of your GitHub accounts yet.`
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Install the app` }))
    expect(mockState.openGithubPopup).toHaveBeenCalledWith(
      `https://github.com/apps/test/installations/new`
    )
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

  it(`a failed status probe says so instead of rendering nothing (EXP-774)`, async () => {
    mockState.statusQuery.mockRejectedValue(new Error(`boom`))
    renderSection()

    await screen.findByText(/Couldn.t reach GitHub connect state\./)
    expect(screen.queryByText(/No GitHub account connected/)).toBeNull()

    // FEED-42: Retry refetches the connect state.
    mockState.statusQuery.mockResolvedValue(githubStatus([installation()]))
    fireEvent.click(screen.getByRole(`button`, { name: `Retry` }))
    await screen.findByText(`GitHub accounts with the app installed`)
  })

  it(`a suspended account says so and links its GitHub page`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([installation({ suspended: true })])
    )
    renderSection()

    await screen.findByText(/GitHub suspended the Exponential app for siteviewer-app/)
    expect(screen.getByRole(`link`, { name: /Manage/ }).getAttribute(`href`)).toBe(
      `https://github.com/settings/installations/1`
    )
  })

  it(`offers "Install on another account" with an existing installation and opens the install URL`, async () => {
    renderSection()

    fireEvent.click(
      await screen.findByRole(`button`, { name: `Install on another account` })
    )
    expect(mockState.openGithubPopup).toHaveBeenCalledWith(
      `https://github.com/apps/test/installations/new`
    )
  })

  it(`renders one row per account with its Configure link`, async () => {
    mockState.statusQuery.mockResolvedValue(
      githubStatus([
        installation(),
        installation({
          installationId: 2,
          accountLogin: `Niach`,
          accountType: `User`,
          manageUrl: `https://github.com/settings/installations/2`,
        }),
      ])
    )
    renderSection()

    await screen.findByText(`siteviewer-app`)
    const links = screen.getAllByRole(`link`, { name: /Configure/ })
    expect(links.map((a) => a.getAttribute(`href`))).toEqual([
      `https://github.com/settings/installations/1`,
      `https://github.com/settings/installations/2`,
    ])
    expect(
      screen.getByText(/Repositories come from these accounts/)
    ).toBeTruthy()
  })
})
