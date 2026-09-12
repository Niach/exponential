import { describe, expect, it } from "vitest"
import {
  fileCardMoreLabel,
  fileCardTitle,
  sessionFileCards,
  toolDiffFiles,
  type FileCardFeedItem,
} from "@/lib/session-file-cards"

// EXP-850 §12: the per-turn file card derivation.

/** The shape the engine publishes for one edit (`steer::unified_diff`): a
 *  BARE unified diff, no `diff --git` header. */
const editDiff = (path: string, adds: number, dels: number) =>
  [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -1,${dels + 1} +1,${adds + 1} @@`,
    ` const kept = 1`,
    ...Array.from({ length: dels }, (_, i) => `-const gone${i} = 1`),
    ...Array.from({ length: adds }, (_, i) => `+const fresh${i} = 1`),
  ].join(`\n`)

const edit = (
  id: number,
  path: string,
  adds = 1,
  dels = 1
): FileCardFeedItem => ({
  id,
  kind: `tool`,
  toolKind: `edit`,
  settled: true,
  diff: editDiff(path, adds, dels),
})

describe(`toolDiffFiles`, () => {
  it(`reads the engine's bare per-call patch`, () => {
    const files = toolDiffFiles(editDiff(`src/a.ts`, 2, 1))
    expect(files).toHaveLength(1)
    expect(files[0].filename).toBe(`src/a.ts`)
    expect(files[0].additions).toBe(2)
    expect(files[0].deletions).toBe(1)
    expect(files[0].patch?.startsWith(`@@`)).toBe(true)
  })

  it(`reads full git diff output too`, () => {
    const files = toolDiffFiles(
      [
        `diff --git a/src/b.ts b/src/b.ts`,
        `--- a/src/b.ts`,
        `+++ b/src/b.ts`,
        `@@ -1 +1,2 @@`,
        ` kept`,
        `+added`,
      ].join(`\n`)
    )
    expect(files.map((f) => f.filename)).toEqual([`src/b.ts`])
    expect(files[0].additions).toBe(1)
  })

  it(`a create names its new path and counts as added`, () => {
    const files = toolDiffFiles(
      [`--- /dev/null`, `+++ b/src/new.ts`, `@@ -0,0 +1 @@`, `+fresh`].join(`\n`)
    )
    expect(files).toEqual([
      expect.objectContaining({
        filename: `src/new.ts`,
        status: `added`,
        additions: 1,
        deletions: 0,
      }),
    ])
  })

  it(`drops the publisher's truncation footer`, () => {
    const files = toolDiffFiles(
      `${editDiff(`src/c.ts`, 1, 0)}\n\\ 120 more lines truncated`
    )
    expect(files.map((f) => f.filename)).toEqual([`src/c.ts`])
  })

  it(`a patchless string yields nothing`, () => {
    expect(toolDiffFiles(``)).toEqual([])
    expect(toolDiffFiles(`not a diff at all`)).toEqual([])
  })
})

describe(`sessionFileCards`, () => {
  it(`one card per turn segment, anchored on its last row`, () => {
    const feed: FileCardFeedItem[] = [
      { id: 1, kind: `user_message` },
      { id: 2, kind: `narration` },
      edit(3, `src/a.ts`, 2, 1),
      { id: 4, kind: `user_message` },
      edit(5, `src/b.ts`, 4, 0),
      { id: 6, kind: `narration` },
    ]
    expect(sessionFileCards(feed)).toEqual([
      {
        afterId: 3,
        files: [{ path: `src/a.ts`, additions: 2, deletions: 1 }],
      },
      {
        afterId: 6,
        files: [{ path: `src/b.ts`, additions: 4, deletions: 0 }],
      },
    ])
  })

  it(`sums a file edited twice in one segment into one row`, () => {
    const cards = sessionFileCards([
      edit(1, `src/a.ts`, 2, 1),
      edit(2, `src/a.ts`, 3, 0),
    ])
    expect(cards).toEqual([
      { afterId: 2, files: [{ path: `src/a.ts`, additions: 5, deletions: 1 }] },
    ])
  })

  it(`ignores unsettled edits, other kinds and subagent rows`, () => {
    expect(
      sessionFileCards([
        { ...edit(1, `src/a.ts`), settled: undefined },
        { id: 2, kind: `tool`, toolKind: `read`, settled: true, diff: editDiff(`src/b.ts`, 1, 0) },
        { id: 3, kind: `tool`, toolKind: `edit`, settled: true },
        { ...edit(4, `src/c.ts`), subagentId: `toolu_a` },
      ])
    ).toEqual([])
  })

  it(`a delete and a move count like an edit`, () => {
    const cards = sessionFileCards([
      { ...edit(1, `src/gone.ts`, 0, 3), toolKind: `delete` },
      { ...edit(2, `src/moved.ts`, 1, 1), toolKind: `move` },
    ])
    expect(cards[0].files.map((f) => f.path)).toEqual([
      `src/gone.ts`,
      `src/moved.ts`,
    ])
  })

  it(`an empty feed has no cards`, () => {
    expect(sessionFileCards([])).toEqual([])
  })
})

describe(`card copy`, () => {
  it(`titles singular and plural`, () => {
    expect(fileCardTitle(1)).toBe(`1 file edited`)
    expect(fileCardTitle(4)).toBe(`4 files edited`)
  })

  it(`folds everything past the fifth path`, () => {
    expect(fileCardMoreLabel(5)).toBeNull()
    expect(fileCardMoreLabel(7)).toBe(`2 more`)
  })
})
