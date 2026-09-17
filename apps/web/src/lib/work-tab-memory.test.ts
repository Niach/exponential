import { beforeEach, describe, expect, it } from "vitest"
import {
  WORK_TAB_MEMORY_CAP,
  forgetClosedTabs,
  issueMemoryOwner,
  readTabMemory,
  resetTabMemory,
  runMemoryOwner,
  tabMemoryOwners,
  tabMemoryOwnersForTest,
  writeTabMemory,
} from "@/lib/work-tab-memory"
import type { WorkTab } from "@/lib/work-tabs"

// EXP-894: what a work tab remembers across a switch — the store is pure, so
// its rules are pinned here rather than through a mounted route.

const issueTab = (issueId: string, runId: string | null = null): WorkTab => ({
  kind: `issue`,
  issueId,
  face: `issue`,
  runId,
  from: null,
  live: false,
})
const runTab = (runId: string): WorkTab => ({
  kind: `run`,
  runId,
  from: null,
  live: false,
})

beforeEach(() => resetTabMemory())

describe(`tab memory slots`, () => {
  it(`round-trips a draft per owner and slot`, () => {
    writeTabMemory(issueMemoryOwner(`i1`), `comment`, `half a thought`)
    writeTabMemory(issueMemoryOwner(`i1`), `reply:c1`, `a reply`)
    writeTabMemory(issueMemoryOwner(`i2`), `comment`, `other issue`)

    expect(readTabMemory(issueMemoryOwner(`i1`), `comment`)).toBe(`half a thought`)
    expect(readTabMemory(issueMemoryOwner(`i1`), `reply:c1`)).toBe(`a reply`)
    expect(readTabMemory(issueMemoryOwner(`i2`), `comment`)).toBe(`other issue`)
    expect(readTabMemory(issueMemoryOwner(`i2`), `reply:c1`)).toBeUndefined()
  })

  it(`clears a slot on an empty value and drops an owner left empty`, () => {
    writeTabMemory(`issue:i1`, `comment`, `text`)
    writeTabMemory(`issue:i1`, `comment`, ``)
    expect(readTabMemory(`issue:i1`, `comment`)).toBeUndefined()
    expect(tabMemoryOwnersForTest()).toEqual([])

    writeTabMemory(`issue:i1`, `scroll`, 120)
    writeTabMemory(`issue:i1`, `replyingTo`, `c1`)
    writeTabMemory(`issue:i1`, `replyingTo`, null)
    expect(readTabMemory(`issue:i1`, `scroll`)).toBe(120)
    expect(tabMemoryOwnersForTest()).toEqual([`issue:i1`])
  })

  it(`evicts the least recently touched owner past the cap`, () => {
    for (let i = 0; i < WORK_TAB_MEMORY_CAP; i++) {
      writeTabMemory(`issue:${i}`, `comment`, `draft ${i}`)
    }
    // Reading the oldest makes it recent again.
    expect(readTabMemory(`issue:0`, `comment`)).toBe(`draft 0`)
    writeTabMemory(`issue:new`, `comment`, `new`)

    const owners = tabMemoryOwnersForTest()
    expect(owners).toHaveLength(WORK_TAB_MEMORY_CAP)
    expect(owners).toContain(`issue:0`)
    expect(owners).not.toContain(`issue:1`)
    expect(owners.at(-1)).toBe(`issue:new`)
  })
})

describe(`tabMemoryOwners`, () => {
  it(`names an issue tab's issue and its bound run`, () => {
    expect(tabMemoryOwners(issueTab(`i1`))).toEqual([`issue:i1`])
    expect(tabMemoryOwners(issueTab(`i1`, `r1`))).toEqual([
      issueMemoryOwner(`i1`),
      runMemoryOwner(`r1`),
    ])
    expect(tabMemoryOwners(runTab(`r2`))).toEqual([`run:r2`])
    expect(
      tabMemoryOwners({ kind: `support`, threadId: `t1`, from: null })
    ).toEqual([])
  })
})

describe(`forgetClosedTabs`, () => {
  it(`forgets a closed tab's drafts and keeps the open ones`, () => {
    writeTabMemory(`issue:i1`, `comment`, `keep`)
    writeTabMemory(`issue:i2`, `comment`, `drop`)
    writeTabMemory(`run:r1`, `scroll`, 40)

    forgetClosedTabs(
      [issueTab(`i1`), issueTab(`i2`, `r1`)],
      [issueTab(`i1`)]
    )

    expect(readTabMemory(`issue:i1`, `comment`)).toBe(`keep`)
    expect(readTabMemory(`issue:i2`, `comment`)).toBeUndefined()
    expect(readTabMemory(`run:r1`, `scroll`)).toBeUndefined()
  })

  it(`keeps an owner another surviving tab still names`, () => {
    writeTabMemory(`run:r1`, `scroll`, 40)

    forgetClosedTabs([runTab(`r1`), issueTab(`i1`, `r1`)], [issueTab(`i1`, `r1`)])

    expect(readTabMemory(`run:r1`, `scroll`)).toBe(40)
  })
})
