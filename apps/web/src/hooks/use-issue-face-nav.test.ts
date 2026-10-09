import { describe, expect, it } from "vitest"
import { runFaceSearch } from "@/hooks/use-issue-face-nav"

// EXP-1251: opening a run keeps the list origin, and a Guide target carries
// its section page and the file in focus.
describe(`runFaceSearch`, () => {
  it(`opens the Run face with the origin alone`, () => {
    expect(runFaceSearch({ from: `inbox`, section: 2, file: `src/a.ts` })).toEqual({
      from: `inbox`,
    })
    expect(runFaceSearch({})).toEqual({})
  })

  it(`opens the Guide on a section page with its file`, () => {
    expect(
      runFaceSearch({ from: `inbox`, guide: true, section: 2, file: `src/a.ts` })
    ).toEqual({ from: `inbox`, view: `guide`, section: 2, file: `src/a.ts` })
  })

  it(`drops a file without a section page`, () => {
    expect(runFaceSearch({ guide: true, file: `src/a.ts` })).toEqual({
      view: `guide`,
    })
  })
})
