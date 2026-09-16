import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { contract } from "@exp/domain-contract"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { ChangesTopBar } from "@/components/changes-top-bar"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  useParams: () => ({ teamSlug: `acme` }),
  useLocation: ({ select }: { select: (l: { pathname: string }) => unknown }) =>
    select({ pathname: `/t/acme/reviews` }),
  useSearch: ({ select }: { select: (s: Record<string, unknown>) => unknown }) =>
    select({}),
}))

const file = (filename: string, adds: number): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: adds,
    deletions: 0,
    patch: [
      `@@ -1 +1,${adds + 1} @@`,
      ` kept`,
      ...Array.from({ length: adds }, (_, i) => `+added ${i}`),
    ].join(`\n`),
  })

const FILES = [file(`src/a.ts`, 2), file(`src/b.ts`, 3)]

const MERGE = {
  issueId: `i1`,
  prState: `open`,
  prNumber: 7,
  branch: `exp/MET-12`,
  updatedAt: null,
  steerEnabled: true,
}

// EXP-916: the REVIEWS header. `identifier · branch · state · summary` then the
// actions — reject, merge, GitHub — every label straight off `contract.diffUi`.
// A run's Changes face has no bar at all now, so this carries exactly one merge
// control and it is the review's.
describe(`ChangesTopBar`, () => {
  it(`identifies the review and sizes it with the summary label`, () => {
    render(
      <ChangesTopBar
        identifier="MET-12"
        files={FILES}
        branch="exp/MET-12"
        prState="open"
      />
    )
    expect(screen.getByText(`MET-12`)).toBeTruthy()
    expect(screen.getByText(`exp/MET-12`)).toBeTruthy()
    expect(screen.getByText(`open`)).toBeTruthy()
    // `summaryLabel(2, 5, 0)` — U+2212 on the deletions.
    expect(screen.getByTestId(`changes-file-count`).textContent).toBe(
      `2 files +5 −0`
    )
  })

  it(`one file reads in the singular; no PR says so`, () => {
    render(<ChangesTopBar files={[FILES[0]]} />)
    expect(screen.getByTestId(`changes-file-count`).textContent).toBe(
      `1 file +2 −0`
    )
    expect(screen.getByText(`No pull request`)).toBeTruthy()
  })

  it(`carries EXACTLY ONE merge control, and it says the contract's words`, () => {
    render(<ChangesTopBar files={FILES} prState="open" merge={MERGE} />)
    expect(
      screen.getAllByRole(`button`, { name: `Merge pull request` })
    ).toHaveLength(1)
    expect(screen.getByText(contract.diffUi.mergePr)).toBeTruthy()
  })

  it(`no merge target = no merge control at all`, () => {
    render(<ChangesTopBar files={FILES} prState="open" merge={null} />)
    expect(
      screen.queryByRole(`button`, { name: `Merge pull request` })
    ).toBeNull()
  })

  it(`reject stands only while the PR is open; GitHub rides the end`, () => {
    const onClosePr = vi.fn()
    render(
      <ChangesTopBar
        files={FILES}
        prState="open"
        prUrl="https://github.com/o/r/pull/7"
        merge={MERGE}
        onClosePr={onClosePr}
      />
    )
    const reject = screen.getByTestId(`changes-close-pr`)
    expect(reject.getAttribute(`title`)).toBe(contract.diffUi.closePr)
    reject.click()
    expect(onClosePr).toHaveBeenCalledOnce()
    const github = screen.getByTestId(`changes-github-link`)
    expect(github.getAttribute(`title`)).toBe(contract.diffUi.openOnGithub)
  })

  it(`no PR url, no GitHub control`, () => {
    render(<ChangesTopBar files={FILES} prState="open" merge={MERGE} />)
    expect(screen.queryByTestId(`changes-github-link`)).toBeNull()
  })

  it(`a closed PR offers neither reject nor merge`, () => {
    render(
      <ChangesTopBar
        files={FILES}
        prState="merged"
        merge={{ ...MERGE, prState: `merged` }}
        onClosePr={vi.fn()}
      />
    )
    expect(screen.queryByTestId(`changes-close-pr`)).toBeNull()
    expect(
      screen.queryByRole(`button`, { name: `Merge pull request` })
    ).toBeNull()
  })
})
