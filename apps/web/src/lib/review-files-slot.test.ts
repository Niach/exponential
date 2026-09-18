import { describe, expect, it } from "vitest"
import { renderHook, act } from "@testing-library/react"
import {
  publishReviewFiles,
  useReviewFilesSlot,
  useReviewFilesSubjectId,
  type ReviewFilesSlot,
} from "./review-files-slot"

const slot = (subjectId: string): ReviewFilesSlot => ({
  subjectId,
  status: `files`,
  files: [],
  selected: null,
  onSelect: () => {},
})

// EXP-916: the page publishes, the sidebar reads — and a cleared slot
// leaves nothing behind.
describe(`review files slot`, () => {
  it(`hands the sidebar what the page published, live`, () => {
    publishReviewFiles(null)
    const { result } = renderHook(() => useReviewFilesSlot())
    expect(result.current).toBeNull()
    const a = slot(`i1`)
    act(() => publishReviewFiles(a))
    expect(result.current).toBe(a)
    const b = slot(`i2`)
    act(() => publishReviewFiles(b))
    expect(result.current).toBe(b)
    act(() => publishReviewFiles(null))
    expect(result.current).toBeNull()
  })

  it(`the subject selector is a primitive: a republish of the same subject is no render`, () => {
    publishReviewFiles(null)
    let renders = 0
    const { result } = renderHook(() => {
      renders++
      return useReviewFilesSubjectId()
    })
    expect(result.current).toBeNull()
    act(() => publishReviewFiles(slot(`i1`)))
    expect(result.current).toBe(`i1`)
    const after = renders
    // A file pick republishes a FRESH object for the same subject.
    act(() => publishReviewFiles({ ...slot(`i1`), selected: `a.ts` }))
    expect(renders).toBe(after)
    act(() => publishReviewFiles(slot(`i2`)))
    expect(result.current).toBe(`i2`)
    act(() => publishReviewFiles(null))
    expect(result.current).toBeNull()
  })
})
