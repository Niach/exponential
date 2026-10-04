import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  liveDiffHandoff,
  runFaceSearch,
  useLiveDiffHandoff,
} from "@/hooks/use-issue-face-nav"

// EXP-1154: the issue route hands a live diff to the run that owns it, and
// carries the tapped file along.
describe(`runFaceSearch`, () => {
  it(`carries the tapped file to the run's Changes face`, () => {
    expect(
      runFaceSearch({ from: `inbox`, view: `diff`, file: `src/a.ts` })
    ).toEqual({ from: `inbox`, view: `diff`, file: `src/a.ts` })
  })

  it(`drops the file off the diff face and empty parts`, () => {
    expect(runFaceSearch({ view: `results`, file: `src/a.ts` })).toEqual({
      view: `results`,
    })
    expect(runFaceSearch({})).toEqual({})
  })
})

describe(`liveDiffHandoff`, () => {
  it(`hands a diff deep link to the run only with a live diff`, () => {
    expect(
      liveDiffHandoff({ showChanges: true, liveDiff: true, runId: `r1` })
    ).toBe(`r1`)
    expect(
      liveDiffHandoff({ showChanges: true, liveDiff: false, runId: `r1` })
    ).toBeNull()
    expect(
      liveDiffHandoff({ showChanges: false, liveDiff: true, runId: `r1` })
    ).toBeNull()
    expect(
      liveDiffHandoff({ showChanges: true, liveDiff: true, runId: null })
    ).toBeNull()
  })

  it(`redirects once the live diff shows up, with the deep-linked file`, () => {
    const goRun = vi.fn()
    const { rerender } = renderHook(
      ({ liveDiff }: { liveDiff: boolean }) =>
        useLiveDiffHandoff(
          { showChanges: true, liveDiff, runId: `r1`, file: `src/a.ts` },
          goRun
        ),
      { initialProps: { liveDiff: false } }
    )
    expect(goRun).not.toHaveBeenCalled()
    rerender({ liveDiff: true })
    expect(goRun).toHaveBeenCalledWith(`r1`, `diff`, `src/a.ts`)
  })
})
