import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { GithubRepoPicker } from "@/components/github-repo-picker"

// FEED-30: the Add-repository picker lists exactly the viewer's grant
// snapshot and used to say nothing about WHY a repo was missing or where to
// fix it. It now explains itself: per-account GitHub configure links, an
// install-another-account button, a refresh, and a by-name escape hatch whose
// server error names the real reason.

const mockState = vi.hoisted(() => ({
  reposQuery: vi.fn(),
  lookupQuery: vi.fn(),
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

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const INSTALL_URL = `https://github.com/apps/test/installations/new?state=s`
const CONNECT_URL = `https://github.com/login/oauth/authorize?x=1`

const reposResult = (overrides: Record<string, unknown> = {}) => ({
  configured: true as const,
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
      hasMore: false,
    },
    {
      installationId: 2,
      accountLogin: `octocat`,
      accountType: `User`,
      manageUrl: `https://github.com/settings/installations/2`,
      suspended: false,
      needsReauth: false,
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

describe(`GithubRepoPicker (FEED-30)`, () => {
  beforeEach(() => {
    mockState.reposQuery.mockReset().mockResolvedValue(reposResult())
    mockState.lookupQuery.mockReset()
  })

  it(`renders one configure link per installation under the list`, async () => {
    renderPicker()

    await screen.findByText(`acme/app`)
    const footer = screen.getByTestId(`repo-picker-footer`)
    expect(footer.textContent).toContain(
      `Only repositories your GitHub installation grants appear here.`
    )
    const links = screen.getAllByRole(`link`)
    expect(links.map((a) => [a.textContent, a.getAttribute(`href`)])).toEqual([
      [`acme`, `https://github.com/organizations/acme/settings/installations/1`],
      [`octocat`, `https://github.com/settings/installations/2`],
    ])
  })

  it(`"Install on another account" opens the install URL; "Refresh" the OAuth re-auth`, async () => {
    const popup = { focus: vi.fn(), closed: true }
    const open = vi
      .spyOn(window, `open`)
      .mockReturnValue(popup as unknown as Window)
    renderPicker()

    fireEvent.click(
      await screen.findByRole(`button`, { name: `Install on another account` })
    )
    expect(open).toHaveBeenLastCalledWith(
      INSTALL_URL,
      `gh-install`,
      expect.any(String)
    )

    fireEvent.click(screen.getByRole(`button`, { name: `Refresh` }))
    expect(open).toHaveBeenLastCalledWith(
      CONNECT_URL,
      `gh-install`,
      expect.any(String)
    )
    open.mockRestore()
  })

  it(`"Refresh" re-lists directly when the instance has no OAuth hop`, async () => {
    mockState.reposQuery.mockResolvedValue(reposResult({ connectUrl: null }))
    const open = vi.spyOn(window, `open`)
    renderPicker()

    fireEvent.click(await screen.findByRole(`button`, { name: `Refresh` }))

    await waitFor(() =>
      expect(mockState.reposQuery).toHaveBeenLastCalledWith({
        teamId: `team-1`,
        refresh: true,
      })
    )
    expect(open).not.toHaveBeenCalled()
    open.mockRestore()
  })

  it(`notes the page cap when the listing was truncated`, async () => {
    mockState.reposQuery.mockResolvedValue(reposResult({ hasMore: true }))
    renderPicker()

    await screen.findByText(/Showing the first 500 repositories per account/)
  })

  it(`a successful by-name lookup selects the repo like a row click`, async () => {
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
      new Error(`You don't have access to acme/hidden on GitHub`)
    )
    const onSelect = renderPicker()

    const input = await screen.findByLabelText(`Add repository by name`)
    fireEvent.change(input, { target: { value: `acme/hidden` } })
    fireEvent.keyDown(input, { key: `Enter` })

    await screen.findByText(`You don't have access to acme/hidden on GitHub`)
    expect(onSelect).not.toHaveBeenCalled()
  })

  it(`keeps the footer in the empty state so the way out is visible`, async () => {
    mockState.reposQuery.mockResolvedValue(reposResult({ repos: [] }))
    renderPicker()

    await screen.findByText(
      `None of your connected GitHub accounts grants a repository yet.`
    )
    expect(screen.getByTestId(`repo-picker-footer`)).toBeTruthy()
  })
})
