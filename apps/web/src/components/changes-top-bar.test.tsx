import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { ChangesTopBar, CHANGES_TITLE } from "@/components/changes-top-bar"

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
  teamId: `t1`,
  updatedAt: null,
  steerEnabled: true,
}

// EXP-895: the ONE md+ header of a Changes surface. The locked shape: the totals
// read like the file column's summary, and there is EXACTLY ONE merge control
// (the run header's own Merge pill stands down while a Changes face is up).
describe(`ChangesTopBar`, () => {
  it(`captions the diff with the same numbers the summary label carries`, () => {
    render(<ChangesTopBar files={FILES} branch="exp/MET-12" prState="open" />)
    expect(screen.getByText(CHANGES_TITLE)).toBeTruthy()
    // `+5 −0`, U+2212 on the deletions — `DiffCounts`, not a hand-rolled cell.
    expect(screen.getByText(`+5`)).toBeTruthy()
    expect(screen.getByText(`−0`)).toBeTruthy()
    expect(screen.getByTestId(`changes-file-count`).textContent).toBe(`2 files`)
    expect(screen.getByText(`exp/MET-12`)).toBeTruthy()
    expect(screen.getByText(`open`)).toBeTruthy()
  })

  it(`one file reads in the singular; no PR says so`, () => {
    render(<ChangesTopBar files={[FILES[0]]} />)
    expect(screen.getByTestId(`changes-file-count`).textContent).toBe(`1 file`)
    expect(screen.getByText(`No pull request`)).toBeTruthy()
  })

  it(`carries EXACTLY ONE merge control`, () => {
    render(<ChangesTopBar files={FILES} prState="open" merge={MERGE} />)
    expect(
      screen.getAllByRole(`button`, { name: `Merge pull request` })
    ).toHaveLength(1)
  })

  it(`no merge target = no merge control at all`, () => {
    render(<ChangesTopBar files={FILES} prState="open" merge={null} />)
    expect(
      screen.queryByRole(`button`, { name: `Merge pull request` })
    ).toBeNull()
  })

  it(`close-PR is the review's own action; GitHub rides the end`, () => {
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
    screen
      .getByRole(`button`, { name: `Close pull request without merging` })
      .click()
    expect(onClosePr).toHaveBeenCalledOnce()
    expect(screen.getByTestId(`changes-github-link`)).toBeTruthy()
  })

  it(`a closed PR offers neither close nor merge`, () => {
    render(
      <ChangesTopBar
        files={FILES}
        prState="merged"
        merge={{ ...MERGE, prState: `merged` }}
        onClosePr={vi.fn()}
      />
    )
    expect(
      screen.queryByRole(`button`, { name: `Close pull request without merging` })
    ).toBeNull()
    expect(
      screen.queryByRole(`button`, { name: `Merge pull request` })
    ).toBeNull()
  })
})
