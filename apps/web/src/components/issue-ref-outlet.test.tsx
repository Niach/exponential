import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  IssueRefContext,
  IssueRefOutlet,
  useIssueRefs,
  type IssueRefContextValue,
  type ResolvedIssueRef,
} from "@/components/issue-ref-provider"

// EXP-1347: chip navigation = the innermost outlet — inside a thread, a pill
// of one of the thread's issues selects its tab; any other issue defers to
// the team provider (the full issue page).

vi.mock(`@/lib/collections`, () => ({ issueCollection: {} }))

const ref = (id: string, identifier: string) =>
  ({ id, identifier, title: identifier, boardSlug: `b` }) as unknown as ResolvedIssueRef

function Pill({ identifier }: { identifier: string }) {
  const refs = useIssueRefs()
  return <button onClick={() => refs?.open(identifier)}>{identifier}</button>
}

function setup() {
  const refs = new Map([
    [`EXP-1`, ref(`i1`, `EXP-1`)],
    [`EXP-2`, ref(`i2`, `EXP-2`)],
    [`EXP-3`, ref(`i3`, `EXP-3`)],
  ])
  const parentOpen = vi.fn()
  const parent = {
    teamId: `t`,
    rows: [...refs.values()],
    resolve: (identifier: string) => refs.get(identifier.toUpperCase()) ?? null,
    resolveById: () => null,
    fromHit: () => null,
    search: () => [],
    open: parentOpen,
  } satisfies IssueRefContextValue
  const onOpen = vi.fn()
  render(
    <IssueRefContext.Provider value={parent}>
      <IssueRefOutlet issueIds={[`i1`, `i2`]} onOpen={onOpen}>
        <Pill identifier="exp-1" />
        <Pill identifier="EXP-2" />
        <Pill identifier="EXP-3" />
      </IssueRefOutlet>
    </IssueRefContext.Provider>
  )
  return { parentOpen, onOpen }
}

describe(`IssueRefOutlet`, () => {
  it(`a thread issue (top or nested) selects its tab, never navigating`, () => {
    const { parentOpen, onOpen } = setup()
    fireEvent.click(screen.getByText(`exp-1`))
    fireEvent.click(screen.getByText(`EXP-2`))
    expect(onOpen.mock.calls).toEqual([[`i1`], [`i2`]])
    expect(parentOpen).not.toHaveBeenCalled()
  })

  it(`any other issue defers to the enclosing provider`, () => {
    const { parentOpen, onOpen } = setup()
    fireEvent.click(screen.getByText(`EXP-3`))
    expect(parentOpen).toHaveBeenCalledWith(`EXP-3`, undefined)
    expect(onOpen).not.toHaveBeenCalled()
  })
})
