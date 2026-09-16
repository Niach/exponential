// EXP-895: the fixture is the contract. Every mirror (desktop `coding::scm` /
// `ui::diff`, web, iOS `ExpCore/Sources/Domain/Diff.swift`, Android
// `domain/Diff.kt`) replays `fixtures/diff/cases.json` + `summary.json` with
// THESE four test names, so a rule that moves here moves everywhere or four
// suites go red at once.
//
// How a case is parsed (the fixture's own contract):
//   - `form: "pullFile"` → `fromPullFile(pullFile)`: GitHub's raw PullFile
//     (`filename`, `previous_filename`, raw `status`, `additions`,
//     `deletions`, `patch`), the ONE mapping every client's PR diff runs.
//   - `form: "hunks"` WITH a `path` → `parsePatch(path, status, input)`
//     (GitHub's PullFile shape: the path and status arrive beside the patch).
//   - every other case, `form: "hunks"` WITHOUT a path included →
//     `parseDiff(input)`, which auto-detects the form. A pathless `hunks`
//     case is exactly the auto-detect path: one file, empty path, `modified`.
//   - `expected` is `renderDiff(…)`, `summary` is `summaryLabel(totals(…))`,
//     and `unchanged` is the FIRST file's `[unchangedBefore(h0),
//     unchangedBetween(h0, h1), …]` (empty when it has no hunks, or when
//     there is no file at all).
//   - `expectedMerged`, when present, is `renderDiff` over
//     `mergeFilesByPath(files)`.

import { describe, expect, test } from "bun:test"

import cases from "../fixtures/diff/cases.json" with { type: "json" }
import summaries from "../fixtures/diff/summary.json" with { type: "json" }
import {
  additionsLabel,
  deletionsLabel,
  fromPullFile,
  mergeFilesByPath,
  parseDiff,
  parsePatch,
  pullFileStatus,
  renderDiff,
  summaryLabel,
  totals,
  unchangedBefore,
  unchangedBetween,
  unchangedLabel,
  type Diff,
  type DiffStatus,
} from "./diff"

interface Case {
  name: string
  form: `git` | `bare` | `hunks` | `none` | `pullFile`
  input?: string
  pullFile?: Parameters<typeof fromPullFile>[0]
  path?: string
  status?: DiffStatus
  expected: string[]
  expectedMerged?: string[]
  summary: string
  unchanged: number[]
}

const fixture = cases as unknown as Case[]
const summaryFixture = summaries as unknown as {
  files: number
  additions: number
  deletions: number
  expected: string
}[]

function parseCase(entry: Case): Diff {
  if (entry.form === `pullFile` && entry.pullFile) {
    return { files: [fromPullFile(entry.pullFile)] }
  }
  if (entry.form === `hunks` && entry.path !== undefined) {
    return {
      files: [
        parsePatch(entry.path, entry.status ?? `modified`, entry.input ?? ``),
      ],
    }
  }
  return parseDiff(entry.input ?? ``)
}

function unchangedRun(diff: Diff): number[] {
  const first = diff.files[0]
  if (!first) return []
  return first.hunks.map((hunk, i) =>
    i === 0 ? unchangedBefore(hunk) : unchangedBetween(first.hunks[i - 1], hunk)
  )
}

describe(`diff`, () => {
  test(`every fixture case parses byte exact`, () => {
    for (const entry of fixture) {
      const diff = parseCase(entry)
      expect(renderDiff(diff), entry.name).toEqual(entry.expected)
      if (entry.expectedMerged) {
        expect(
          renderDiff({ ...diff, files: mergeFilesByPath(diff.files) }),
          `${entry.name} (merged)`
        ).toEqual(entry.expectedMerged)
      }
    }
  })

  test(`the fixture covers every input form`, () => {
    expect(fixture.length).toBeGreaterThanOrEqual(18)
    const forms = new Set(fixture.map((entry) => entry.form))
    expect([...forms].sort()).toEqual([
      `bare`,
      `git`,
      `hunks`,
      `none`,
      `pullFile`,
    ])
    // A `none` case is the empty parse; every other form yields files.
    for (const entry of fixture) {
      const files = parseCase(entry).files
      if (entry.form === `none`) expect(files, entry.name).toEqual([])
      else expect(files.length, entry.name).toBeGreaterThan(0)
    }
  })

  test(`summary label matches every fixture case`, () => {
    for (const entry of fixture) {
      const t = totals(parseCase(entry).files)
      expect(summaryLabel(t.files, t.additions, t.deletions), entry.name).toBe(
        entry.summary
      )
    }
    for (const row of summaryFixture) {
      expect(
        summaryLabel(row.files, row.additions, row.deletions),
        JSON.stringify(row)
      ).toBe(row.expected)
    }
    expect(summaryFixture.length).toBeGreaterThanOrEqual(4)
  })

  test(`unchanged line counts match every fixture case`, () => {
    for (const entry of fixture) {
      expect(unchangedRun(parseCase(entry)), entry.name).toEqual(
        entry.unchanged
      )
    }
  })
})

describe(`mergeFilesByPath`, () => {
  const file = (
    path: string,
    over: Partial<ReturnType<typeof parsePatch>> = {}
  ): ReturnType<typeof parsePatch> => ({
    path,
    status: `modified`,
    additions: 1,
    deletions: 1,
    binary: false,
    hunks: [],
    ...over,
  })

  test(`folds repeats in order of first appearance`, () => {
    const merged = mergeFilesByPath([file(`b.ts`), file(`a.ts`), file(`b.ts`)])
    expect(merged.map((f) => f.path)).toEqual([`b.ts`, `a.ts`])
    expect(merged[0].additions).toBe(2)
    expect(merged[0].deletions).toBe(2)
  })

  test(`later status, binary and previousPath win`, () => {
    const merged = mergeFilesByPath([
      file(`a.ts`, { status: `modified` }),
      file(`a.ts`, { status: `renamed`, binary: true, previousPath: `old.ts` }),
    ])
    expect(merged).toHaveLength(1)
    expect(merged[0].status).toBe(`renamed`)
    expect(merged[0].binary).toBe(true)
    expect(merged[0].previousPath).toBe(`old.ts`)
  })

  test(`concatenates hunks and never mutates its input`, () => {
    const one = parsePatch(`a.ts`, `modified`, `@@ -1 +1 @@\n-a\n+A\n`)
    const two = parsePatch(`a.ts`, `modified`, `@@ -9 +9 @@\n-b\n+B\n`)
    const merged = mergeFilesByPath([one, two])
    expect(merged[0].hunks).toHaveLength(2)
    expect(one.hunks).toHaveLength(1)
    expect(one.additions).toBe(1)
  })
})

describe(`parsePatch and fromPullFile`, () => {
  test(`an absent patch is a file with no hunks`, () => {
    for (const patch of [null, undefined, ``]) {
      const file = parsePatch(`logo.png`, `modified`, patch)
      expect(file).toEqual({
        path: `logo.png`,
        status: `modified`,
        additions: 0,
        deletions: 0,
        binary: false,
        hunks: [],
      })
    }
  })

  test(`the patch never renames the file it was handed`, () => {
    const file = parsePatch(
      `given.ts`,
      `added`,
      `diff --git a/other.ts b/other.ts\n--- /dev/null\n+++ b/other.ts\n@@ -0,0 +1 @@\n+x\n`
    )
    expect(file.path).toBe(`given.ts`)
    expect(file.status).toBe(`added`)
    expect(file.additions).toBe(1)
  })

  test(`github's status vocabulary maps onto ours`, () => {
    expect(pullFileStatus(`added`)).toBe(`added`)
    expect(pullFileStatus(`removed`)).toBe(`removed`)
    expect(pullFileStatus(`renamed`)).toBe(`renamed`)
    expect(pullFileStatus(`copied`)).toBe(`copied`)
    expect(pullFileStatus(`changed`)).toBe(`modified`)
    expect(pullFileStatus(`unchanged`)).toBe(`modified`)
    expect(pullFileStatus(`something-new`)).toBe(`modified`)
  })

  test(`a patchless pull file keeps github's own counts`, () => {
    expect(
      fromPullFile({
        filename: `assets/hero.png`,
        status: `modified`,
        additions: 0,
        deletions: 0,
      })
    ).toEqual({
      path: `assets/hero.png`,
      status: `modified`,
      additions: 0,
      deletions: 0,
      binary: false,
      hunks: [],
    })
    const huge = fromPullFile({
      filename: `bun.lock`,
      status: `modified`,
      additions: 4210,
      deletions: 118,
      patch: null,
    })
    expect(huge.additions).toBe(4210)
    expect(huge.deletions).toBe(118)
  })

  test(`a pull file with a patch counts its own lines and keeps previous_filename`, () => {
    const file = fromPullFile({
      filename: `src/new.ts`,
      previous_filename: `src/old.ts`,
      status: `renamed`,
      additions: 99,
      deletions: 99,
      patch: `@@ -1,2 +1,2 @@\n keep\n-a\n+A\n`,
    })
    expect(file.path).toBe(`src/new.ts`)
    expect(file.previousPath).toBe(`src/old.ts`)
    expect(file.status).toBe(`renamed`)
    expect(file.additions).toBe(1)
    expect(file.deletions).toBe(1)
  })
})

describe(`labels`, () => {
  test(`additions, deletions and unchanged lines`, () => {
    expect(additionsLabel(0)).toBe(`+0`)
    expect(additionsLabel(12)).toBe(`+12`)
    // U+2212 MINUS SIGN, never an ASCII hyphen.
    expect(deletionsLabel(4)).toBe(`−4`)
    expect(deletionsLabel(4).charCodeAt(0)).toBe(0x2212)
    expect(unchangedLabel(1)).toBe(`1 unchanged line`)
    expect(unchangedLabel(12)).toBe(`12 unchanged lines`)
  })

  test(`the summary label`, () => {
    expect(summaryLabel(0, 0, 0)).toBe(`No changes`)
    expect(summaryLabel(0, 9, 9)).toBe(`No changes`)
    expect(summaryLabel(1, 2, 0)).toBe(`1 file +2 −0`)
    expect(summaryLabel(3, 12, 4)).toBe(`3 files +12 −4`)
  })

  test(`unchanged runs never go negative`, () => {
    const hunk = (newStart: number, newLines: number) => ({
      oldStart: newStart,
      oldLines: newLines,
      newStart,
      newLines,
      header: ``,
      lines: [],
    })
    expect(unchangedBefore(hunk(1, 3))).toBe(0)
    expect(unchangedBefore(hunk(0, 0))).toBe(0)
    expect(unchangedBefore(hunk(40, 3))).toBe(39)
    expect(unchangedBetween(hunk(1, 3), hunk(20, 3))).toBe(16)
    expect(unchangedBetween(hunk(1, 30), hunk(20, 3))).toBe(0)
  })
})
