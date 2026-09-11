import { render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { BoardRepoField } from "@/components/board-repo-field"

// FEED-32: the field keeps its own copy of `repositories.list`, but the host
// can retarget the board at a repo that copy has never seen (the settings
// dialog connects a new repo and the live board row flips first). The
// trigger must never render blank: it re-lists once for an unknown id and
// labels the states explicitly (loading / unavailable / the repo's name).

const mockState = vi.hoisted(() => ({
  listQuery: vi.fn(),
  listBranchesQuery: vi.fn(),
  reposQuery: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    repositories: {
      list: { query: mockState.listQuery },
      listBranches: { query: mockState.listBranchesQuery },
    },
    integrations: {
      github: {
        repos: { query: mockState.reposQuery },
      },
    },
  },
}))

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const repoRow = (overrides: Record<string, unknown> = {}) => ({
  id: `repo-1`,
  teamId: `team-1`,
  fullName: `acme/app`,
  defaultBranch: `main`,
  githubDefaultBranch: `main`,
  defaultBranchOverride: null,
  private: false,
  installationId: 1,
  inaccessibleAt: null,
  sharedBy: null,
  boards: [],
  ...overrides,
})

function renderField(repositoryId: string | null) {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  return render(
    <BoardRepoField
      teamId="team-1"
      repositoryId={repositoryId}
      onSelectRegistry={() => {}}
      onConnectNew={() => {}}
      branch={null}
      onBranchChange={() => {}}
    />
  )
}

const trigger = () => screen.getByRole(`combobox`, { name: `Repository` })

describe(`BoardRepoField (FEED-32)`, () => {
  beforeEach(() => {
    mockState.listQuery.mockReset()
    mockState.listBranchesQuery
      .mockReset()
      .mockResolvedValue({ branches: [`main`] })
    mockState.reposQuery.mockReset()
  })

  it(`re-lists once when the value is missing from the first list and shows the name`, async () => {
    mockState.listQuery
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([repoRow()])
    renderField(`repo-1`)

    await waitFor(() => expect(mockState.listQuery).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(trigger().textContent).toContain(`acme/app`))
  })

  it(`shows the selected repo's full name when the list already has it`, async () => {
    mockState.listQuery.mockResolvedValue([repoRow()])
    renderField(`repo-1`)

    await waitFor(() => expect(trigger().textContent).toContain(`acme/app`))
    expect(mockState.listQuery).toHaveBeenCalledTimes(1)
  })

  it(`labels an id the list never resolves instead of rendering blank`, async () => {
    mockState.listQuery.mockResolvedValue([])
    renderField(`repo-gone`)

    await waitFor(() => expect(mockState.listQuery).toHaveBeenCalledTimes(2))
    await waitFor(() =>
      expect(trigger().textContent).toContain(`Repository unavailable`)
    )
  })

  it(`a failed list shows the error and keeps a labelled trigger`, async () => {
    mockState.listQuery.mockRejectedValue(new Error(`boom`))
    renderField(`repo-1`)

    await screen.findByText(/Couldn.t load the team.s repositories: boom/)
    expect(trigger().textContent).not.toBe(``)
    expect(trigger().textContent).toContain(`Repository unavailable`)
  })

  it(`reads "No repository" with nothing selected`, async () => {
    mockState.listQuery.mockResolvedValue([repoRow()])
    renderField(null)

    await waitFor(() => expect(trigger().textContent).toContain(`No repository`))
  })
})
