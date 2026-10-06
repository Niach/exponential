import { describe, expect, it } from "vitest"
import { parseSessionResultGroups } from "@exp/ui"
import {
  diffFromPrFiles,
  resumedRunId,
  runAttachmentIds,
  runIsLive,
  runMarkState,
  runPillLabel,
  runResultRecords,
  runWorkingCaption,
  steerFeedback,
} from "./run-model"
import type { RunResult } from "./model"

const A = `0b0c6a52-2b0e-4b8e-9d5d-1f9f0e7c1a01`
const B = `0b0c6a52-2b0e-4b8e-9d5d-1f9f0e7c1a02`

const results: RunResult[] = [
  { topic: `Summary`, text: `Did the thing.` },
  { topic: `Run face`, text: `Header + diff.`, files: [`src/run-view.tsx`] },
  { topic: `Run face`, label: `web`, url: `https://exp.test/api/attachments/${A}` },
  { topic: `Run face`, url: `/api/attachments/${B}?x=1` },
  { topic: `Run face`, label: `bad`, url: `https://elsewhere.test/a.png` },
]

describe(`runResultRecords`, () => {
  it(`maps text entries and pictures by attachment id`, () => {
    expect(runResultRecords(results)).toEqual([
      { topic: `Summary`, text: `Did the thing.`, files: [] },
      { topic: `Run face`, text: `Header + diff.`, files: [`src/run-view.tsx`] },
      { topic: `Run face`, label: `web`, attachmentId: A },
      { topic: `Run face`, label: `Run face`, attachmentId: B },
    ])
  })

  it(`drops pictures whose signed URL has not resolved`, () => {
    const records = runResultRecords(results, { [A]: `https://signed/a`, [B]: null })
    expect(records.filter((record) => `attachmentId` in record)).toEqual([
      { topic: `Run face`, label: `web`, attachmentId: A },
    ])
  })

  it(`feeds parseSessionResultGroups (text, files, pictures)`, () => {
    const groups = parseSessionResultGroups(runResultRecords(results))
    expect(groups.map((group) => group.topic)).toEqual([`Summary`, `Run face`])
    expect(groups[1].files).toEqual([`src/run-view.tsx`])
    expect(groups[1].entries.map((entry) => entry.attachmentId)).toEqual([A, B])
  })
})

describe(`runAttachmentIds`, () => {
  it(`lists distinct ids in order`, () => {
    expect(runAttachmentIds([...results, results[2]])).toEqual([A, B])
    expect(runAttachmentIds(undefined)).toEqual([])
  })
})

describe(`run state`, () => {
  it(`live = running or in_review`, () => {
    expect(runIsLive({ status: `running` })).toBe(true)
    expect(runIsLive({ status: `in_review` })).toBe(true)
    expect(runIsLive({ status: `ended` })).toBe(false)
  })

  it(`mark state, pill and caption`, () => {
    const working = { id: `r`, status: `running`, agentBusy: true, agentCaption: ` Reading files ` }
    expect(runMarkState(working)).toBe(`working`)
    expect(runPillLabel(working)).toBe(`Working`)
    expect(runWorkingCaption(working)).toBe(`Reading files`)
    const asking = { ...working, needsInput: true }
    expect(runMarkState(asking)).toBe(`needs_input`)
    expect(runWorkingCaption(asking)).toBeNull()
    const review = { id: `r`, status: `in_review` }
    expect(runMarkState(review)).toBe(`review`)
    expect(runPillLabel(review)).toBe(`In review`)
    const ended = { id: `r`, status: `ended`, agentBusy: true }
    expect(runMarkState(ended)).toBeUndefined()
    expect(runPillLabel(ended)).toBe(`Ended`)
    expect(runWorkingCaption(ended)).toBeNull()
  })
})

describe(`diffFromPrFiles`, () => {
  it(`parses pull files and ignores junk`, () => {
    const files = diffFromPrFiles({
      files: [
        {
          filename: `a.ts`,
          status: `modified`,
          additions: 1,
          deletions: 1,
          patch: `@@ -1 +1 @@\n-old\n+new`,
        },
        { filename: `img.png`, status: `added`, additions: 0, deletions: 0 },
        null,
        { nope: true },
      ],
    })
    expect(files.map((file) => [file.path, file.status, file.additions, file.deletions])).toEqual([
      [`a.ts`, `modified`, 1, 1],
      [`img.png`, `added`, 0, 0],
    ])
    expect(diffFromPrFiles(null)).toEqual([])
    expect(diffFromPrFiles({ files: `x` })).toEqual([])
  })
})

describe(`tool answers`, () => {
  it(`steer feedback`, () => {
    expect(steerFeedback({ delivered: true, queued: true })).toMatch(/^Queued/)
    expect(steerFeedback({ delivered: true, consumed: true })).toBe(`Sent.`)
    expect(steerFeedback({ delivered: true, consumed: false })).toMatch(/not picked it up/)
  })

  it(`resumed run id`, () => {
    expect(resumedRunId({ sessionId: `n1` })).toBe(`n1`)
    expect(resumedRunId({ session: { id: `n2` } })).toBe(`n2`)
    expect(resumedRunId({})).toBeNull()
  })
})
