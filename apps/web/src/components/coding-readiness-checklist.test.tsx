import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Board } from "@/db/schema"
import {
  codingReadiness,
  READINESS_COPY,
  type CodingReadinessInput,
} from "@/lib/coding-readiness"
import type {
  CodingReadinessState,
  ReadinessRepoList,
} from "@/hooks/use-coding-readiness"

// EXP-1121: the checklist's CHROME over the fixture-locked model — the rows'
// three states, the fixes of the current row, the owner-only Board settings,
// the inline repository picker (rows, tags, the pick) and the capsule's
// three looks. The model and its copy have their own fixture test.

const setRepository = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-router`, () => ({
  useParams: () => ({ teamSlug: `acme` }),
  Link: ({
    children,
    to,
    params: _params,
    ...rest
  }: {
    children: React.ReactNode
    to: string
    params?: unknown
  }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    boards: { setRepository: { mutate: setRepository } },
    repositories: { add: { mutate: vi.fn() } },
  },
}))
vi.mock(`@/components/add-device-dialog`, () => ({
  AddDeviceDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="add-device-dialog" /> : null,
}))
vi.mock(`@/components/github-repo-picker`, () => ({
  GithubRepoPicker: () => <div data-testid="github-repo-picker" />,
}))
const openGithubConnect = vi.hoisted(() => vi.fn(() => true))
vi.mock(`@/lib/github-connect`, () => ({
  openGithubConnect,
  POPUP_BLOCKED_MESSAGE: `blocked`,
}))

import {
  ReadinessStartPill,
  ReadinessSteps,
} from "@/components/coding-readiness-checklist"
import { readinessRepoRows } from "@/lib/coding-readiness"
import { readinessBoard } from "@/hooks/use-coding-readiness"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const NOW = Date.UTC(2026, 8, 28, 12)

const board = {
  id: `b-app`,
  name: `App`,
  slug: `app`,
  teamId: `t1`,
  repositoryId: null,
} as unknown as Board

const repos = [
  { id: `r-web`, fullName: `acme-inc/website`, boards: [{ id: `b-web`, name: `Website`, slug: `website` }] },
  { id: `r-app`, fullName: `acme-inc/app`, boards: [] },
  { id: `r-cli`, fullName: `acme-inc/cli`, boards: [] },
] as unknown as ReadinessRepoList

function input(overrides: Partial<CodingReadinessInput> = {}): CodingReadinessInput {
  return {
    isMember: true,
    remoteStartEnabled: true,
    teamName: `Acme`,
    boardName: `App`,
    boardRepository: null,
    github: { connected: true, label: `acme-inc` },
    devices: [
      {
        label: `MacBook Pro`,
        own: true,
        online: false,
        lastSeenAtMs: NOW - 2 * 3_600_000,
      },
    ],
    nowMs: NOW,
    ...overrides,
  }
}

function state(
  overrides: Partial<CodingReadinessInput> = {},
  extra: Partial<CodingReadinessState> = {}
): CodingReadinessState {
  return {
    readiness: codingReadiness(input(overrides)),
    board,
    teamId: `t1`,
    isOwner: true,
    repos,
    githubLinked: true,
    ownDevices: [],
    reload: vi.fn(),
    ...extra,
  }
}

describe(`ReadinessSteps`, () => {
  it(`draws met / current / pending with the current row's fixes`, () => {
    render(<ReadinessSteps state={state()} />)
    const rows = screen
      .getByTestId(`coding-readiness-steps`)
      .querySelectorAll(`[data-step]`)
    expect([...rows].map((row) => row.getAttribute(`data-state`))).toEqual([
      `met`,
      `current`,
      `pending`,
    ])
    expect(rows[0]!.textContent).toContain(`acme-inc`)
    expect(rows[1]!.textContent).toContain(`Connect a repository to App`)
    expect(rows[2]!.textContent).toContain(`Your MacBook Pro was last seen 2 h ago.`)
    expect(screen.getByTestId(`readiness-fix-choose_repository`).textContent).toBe(
      READINESS_COPY.fixChooseRepository
    )
    expect(screen.getByTestId(`readiness-fix-board_settings`)).toBeTruthy()
  })

  it(`hides the owner-only Board settings from members`, () => {
    render(<ReadinessSteps state={state({}, { isOwner: false })} />)
    expect(screen.queryByTestId(`readiness-fix-board_settings`)).toBeNull()
    expect(screen.getByTestId(`readiness-fix-choose_repository`)).toBeTruthy()
  })

  it(`swaps the fixes for the inline picker and points the board`, async () => {
    setRepository.mockReset().mockResolvedValue({ txId: 1 })
    render(<ReadinessSteps state={state()} />)
    fireEvent.click(screen.getByTestId(`readiness-fix-choose_repository`))
    expect(screen.getByTestId(`readiness-repo-picker`)).toBeTruthy()
    expect(screen.queryByTestId(`readiness-fix-choose_repository`)).toBeNull()
    expect(
      screen.getByPlaceholderText(READINESS_COPY.pickerSearch)
    ).toBeTruthy()
    fireEvent.click(screen.getByText(`acme-inc/app`))
    expect(setRepository).toHaveBeenCalledWith({
      boardId: `b-app`,
      repositoryId: `r-app`,
    })
  })

  it(`Connect GitHub opens the guided page for the board (SLOP-7)`, () => {
    openGithubConnect.mockClear()
    render(
      <ReadinessSteps
        state={state({ github: { connected: false, label: null } }, { githubLinked: false })}
      />
    )
    fireEvent.click(screen.getByTestId(`readiness-fix-connect_github`))
    expect(openGithubConnect).toHaveBeenCalledWith({ teamId: `t1`, boardId: `b-app` })
  })

  it(`opens the add-device dialog from Set up a server`, () => {
    render(
      <ReadinessSteps
        state={state({
          boardRepository: `acme-inc/app`,
          github: null,
          devices: [],
        })}
      />
    )
    expect(screen.queryByTestId(`readiness-fix-open_devices`)).toBeNull()
    fireEvent.click(screen.getByTestId(`readiness-fix-set_up_server`))
    expect(screen.getByTestId(`add-device-dialog`)).toBeTruthy()
  })
})

describe(`readinessRepoRows`, () => {
  it(`puts the board's match first and tags the rest`, () => {
    expect(readinessRepoRows(repos, board)).toEqual([
      { id: `r-app`, fullName: `acme-inc/app`, matches: true, tag: `matches board` },
      { id: `r-web`, fullName: `acme-inc/website`, matches: false, tag: `used by Website` },
      { id: `r-cli`, fullName: `acme-inc/cli`, matches: false, tag: null },
    ])
  })

  it(`filters on a substring of the full name`, () => {
    expect(readinessRepoRows(repos, board, `WEB`).map((row) => row.id)).toEqual([
      `r-web`,
    ])
  })
})

describe(`readinessBoard`, () => {
  const withRepo = { ...board, id: `b2`, repositoryId: `r-web` } as Board
  it(`prefers the first repo-backed board of a selection`, () => {
    expect(readinessBoard([board, withRepo], [`b-app`, `b2`])?.id).toBe(`b2`)
    expect(readinessBoard([board, withRepo], [`b-app`])?.id).toBe(`b-app`)
    expect(readinessBoard([board], [`missing`])).toBeNull()
  })
})

describe(`ReadinessStartPill`, () => {
  it(`is dashed while a step is missing, inert while loading, primary once ready`, () => {
    const onClick = vi.fn()
    const { rerender } = render(
      <ReadinessStartPill
        readiness={codingReadiness(input())}
        tone="primary"
        onClick={onClick}
        testId="pill"
      />
    )
    const pill = () => screen.getByTestId(`pill`)
    expect(pill().getAttribute(`data-readiness`)).toBe(`missing`)
    expect(pill().className).toContain(`border-dashed`)
    fireEvent.click(pill())
    expect(onClick).toHaveBeenCalledTimes(1)

    rerender(
      <ReadinessStartPill
        readiness={codingReadiness(input({ devices: null }))}
        tone="primary"
        onClick={onClick}
        testId="pill"
      />
    )
    expect(pill().getAttribute(`data-readiness`)).toBe(`loading`)
    expect(pill().getAttribute(`aria-disabled`)).toBe(`true`)
    fireEvent.click(pill())
    expect(onClick).toHaveBeenCalledTimes(1)

    rerender(
      <ReadinessStartPill
        readiness={codingReadiness(
          input({
            boardRepository: `acme-inc/app`,
            devices: [
              { label: `MacBook Pro`, own: true, online: true, lastSeenAtMs: NOW },
            ],
          })
        )}
        tone="primary"
        onClick={onClick}
        testId="pill"
      />
    )
    expect(pill().getAttribute(`data-readiness`)).toBe(`ready`)
    expect(pill().className).not.toContain(`border-dashed`)
    expect(pill().className).toContain(`bg-primary`)
  })
})
