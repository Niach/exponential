import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

// SLOP-7: the Add-repository picker lists the viewer's push-able repos LIVE
// and, when a prerequisite is missing, says which one and offers the ONE fix:
// Connect GitHub (the guided page in a popup), or Install the app (GitHub's
// install page in a popup). The list explains itself with per-account
// Configure links, Install on another account, Refresh and a by-name escape
// hatch whose server error names the real reason.

const mockState = vi.hoisted(() => ({
  reposQuery: vi.fn(),
  lookupQuery: vi.fn(),
  openGithubConnect: vi.fn(() => true),
  openGithubPopup: vi.fn(() => true),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    integrations: {
      github: {
        repos: { query: mockState.reposQuery },
        lookupRepo: { query: mockState.lookupQuery },
      },
    },
  },
}))
vi.mock(`@/lib/github-connect`, () => ({
  openGithubConnect: mockState.openGithubConnect,
  openGithubPopup: mockState.openGithubPopup,
  POPUP_BLOCKED_MESSAGE: `blocked`,
}))

import { GithubRepoPicker } from "@/components/github-repo-picker"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const INSTALL_URL = `https://github.com/apps/test/installations/new`
const CONNECT_URL = `https://app.example.com/integrations/github?team=team-1`

const reposResult = (overrides: Record<string, unknown> = {}) => ({
  configured: true as const,
  connectConfigured: true,
  linked: true,
  needsReconnect: false,
  login: `octocat`,
  installed: true,
  installUrl: INSTALL_URL,
  connectUrl: CONNECT_URL,
  repos: [
    {
      fullName: `acme/app`,
      private: true,
      defaultBranch: `main`,
      installationId: 1,
    },
  ],
  hasMore: false,
  installations: [
    {
      installationId: 1,
      accountLogin: `acme`,
      accountType: `Organization`,
      manageUrl: `https://github.com/organizations/acme/settings/installations/1`,
      suspended: false,
      needsReauth: false,
      stale: false,
      hasMore: false,
    },
    {
      installationId: 2,
      accountLogin: `octocat`,
      accountType: `User`,
      manageUrl: `https://github.com/settings/installations/2`,
      suspended: false,
      needsReauth: false,
      stale: false,
      hasMore: false,
    },
  ],
  ...overrides,
})

function renderPicker(onSelect = vi.fn()) {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  render(<GithubRepoPicker teamId="team-1" onSelect={onSelect} />)
  return onSelect
}

describe(`GithubRepoPicker (SLOP-7)`, () => {
  beforeEach(() => {
    mockState.reposQuery.mockReset()
    mockState.lookupQuery.mockReset()
    mockState.openGithubConnect.mockClear()
    mockState.openGithubPopup.mockClear()
    mockState.reposQuery.mockResolvedValue(reposResult())
  })

  it(`renders one configure link per installation under the list`, async () => {
    renderPicker()
    const footer = await screen.findByTestId(`repo-picker-footer`)
    const links = [...footer.querySelectorAll(`a`)]
    expect(links.map((a) => a.textContent)).toEqual([`acme`, `octocat`])
    expect(links[0]!.getAttribute(`href`)).toBe(
      `https://github.com/organizations/acme/settings/installations/1`
    )
  })

  it(`a row tap adds the repo (tap adds, ×4)`, async () => {
    const onSelect = renderPicker()
    fireEvent.click(await screen.findByText(`acme/app`))
    expect(onSelect).toHaveBeenCalledWith({
      fullName: `acme/app`,
      private: true,
      defaultBranch: `main`,
      installationId: 1,
    })
  })

  it(`not linked → Connect GitHub opens the guided page`, async () => {
    mockState.reposQuery.mockResolvedValue(
      reposResult({ linked: false, installed: false, repos: [], installations: [] })
    )
    renderPicker()
    await screen.findByText(
      `Connect your GitHub account to pick a repository. You’ll come right back here.`
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Connect GitHub` }))
    expect(mockState.openGithubConnect).toHaveBeenCalledWith({ teamId: `team-1` })
  })

  it(`an expired connection → Reconnect GitHub opens the guided page`, async () => {
    mockState.reposQuery.mockResolvedValue(
      reposResult({ needsReconnect: true, installed: false, repos: [], installations: [] })
    )
    renderPicker()
    await screen.findByText(`Your GitHub connection expired. Reconnect to list your repositories.`)
    fireEvent.click(screen.getByRole(`button`, { name: `Reconnect GitHub` }))
    expect(mockState.openGithubConnect).toHaveBeenCalledWith({ teamId: `team-1` })
  })

  it(`linked but nothing installed → Install the app opens GitHub's install page`, async () => {
    mockState.reposQuery.mockResolvedValue(
      reposResult({ installed: false, repos: [], installations: [] })
    )
    renderPicker()
    await screen.findByText(
      `Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here.`
    )
    fireEvent.click(screen.getByRole(`button`, { name: `Install the app` }))
    expect(mockState.openGithubPopup).toHaveBeenCalledWith(INSTALL_URL)
    expect(mockState.openGithubConnect).not.toHaveBeenCalled()
  })

  it(`"I’ve done that" re-lists with refresh`, async () => {
    mockState.reposQuery.mockResolvedValue(
      reposResult({ linked: false, installed: false, repos: [], installations: [] })
    )
    renderPicker()
    fireEvent.click(await screen.findByRole(`button`, { name: `I’ve done that` }))
    await waitFor(() =>
      expect(mockState.reposQuery).toHaveBeenLastCalledWith({
        teamId: `team-1`,
        refresh: true,
      })
    )
  })

  it(`"Install on another account" opens the install URL; "Refresh" re-lists`, async () => {
    renderPicker()
    fireEvent.click(
      await screen.findByRole(`button`, { name: `Install on another account` })
    )
    expect(mockState.openGithubPopup).toHaveBeenLastCalledWith(INSTALL_URL)

    fireEvent.click(screen.getByRole(`button`, { name: `Refresh` }))
    await waitFor(() =>
      expect(mockState.reposQuery).toHaveBeenLastCalledWith({
        teamId: `team-1`,
        refresh: true,
      })
    )
  })

  it(`notes the page cap when the listing was truncated`, async () => {
    mockState.reposQuery.mockResolvedValue(reposResult({ hasMore: true }))
    renderPicker()
    await screen.findByText(/Showing the first 500 repositories per account/)
  })

  it(`a successful by-name lookup adds the repo like a row click`, async () => {
    const repo = {
      fullName: `acme/hidden`,
      private: true,
      defaultBranch: `develop`,
      installationId: 1,
    }
    mockState.lookupQuery.mockResolvedValue(repo)
    const onSelect = renderPicker()

    const input = await screen.findByLabelText(`Add repository by name`)
    const lookUp = screen.getByRole(`button`, { name: `Look up` })
    expect((lookUp as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(input, { target: { value: `not a repo` } })
    expect((lookUp as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(input, { target: { value: `acme/hidden` } })
    expect((lookUp as HTMLButtonElement).disabled).toBe(false)
    fireEvent.click(lookUp)

    await waitFor(() => expect(onSelect).toHaveBeenCalledWith(repo))
    expect(mockState.lookupQuery).toHaveBeenCalledWith({
      teamId: `team-1`,
      fullName: `acme/hidden`,
    })
  })

  it(`a failed lookup shows the server's message inline`, async () => {
    mockState.lookupQuery.mockRejectedValue(
      new Error(`You need push access to acme/hidden on GitHub to connect it.`)
    )
    const onSelect = renderPicker()

    const input = await screen.findByLabelText(`Add repository by name`)
    fireEvent.change(input, { target: { value: `acme/hidden` } })
    fireEvent.keyDown(input, { key: `Enter` })

    await screen.findByText(`You need push access to acme/hidden on GitHub to connect it.`)
    expect(onSelect).not.toHaveBeenCalled()
  })

  it(`keeps the footer in the empty state so the way out is visible`, async () => {
    mockState.reposQuery.mockResolvedValue(reposResult({ repos: [] }))
    renderPicker()

    await screen.findByText(
      `The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh.`
    )
    expect(screen.getByTestId(`repo-picker-footer`)).toBeTruthy()
  })

  it(`a suspended installation explains itself instead of an empty list`, async () => {
    mockState.reposQuery.mockResolvedValue(
      reposResult({
        repos: [],
        installations: [
          {
            installationId: 1,
            accountLogin: `acme`,
            accountType: `Organization`,
            manageUrl: `https://github.com/organizations/acme/settings/installations/1`,
            suspended: true,
            needsReauth: false,
            stale: false,
            hasMore: false,
          },
        ],
      })
    )
    renderPicker()
    await screen.findByText(/GitHub suspended the Exponential app for acme/)
  })
})
