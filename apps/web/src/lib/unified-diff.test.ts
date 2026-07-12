import { describe, expect, it } from "vitest"
import { splitUnifiedDiff, unifiedDiffStats } from "./unified-diff"

const MULTI = [
  `diff --git a/src/a.ts b/src/a.ts`,
  `index 111..222 100644`,
  `--- a/src/a.ts`,
  `+++ b/src/a.ts`,
  `@@ -1,3 +1,4 @@`,
  ` context`,
  `-old line`,
  `+new line`,
  `+added line`,
  `diff --git a/src/new.ts b/src/new.ts`,
  `new file mode 100644`,
  `index 000..333`,
  `--- /dev/null`,
  `+++ b/src/new.ts`,
  `@@ -0,0 +1,2 @@`,
  `+alpha`,
  `+beta`,
  `diff --git a/gone.txt b/gone.txt`,
  `deleted file mode 100644`,
  `index 444..000`,
  `--- a/gone.txt`,
  `+++ /dev/null`,
  `@@ -1 +0,0 @@`,
  `-bye`,
  ``,
].join(`\n`)

describe(`splitUnifiedDiff`, () => {
  it(`splits per file with status, counts, and hunk-only patches`, () => {
    const files = splitUnifiedDiff(MULTI)
    expect(files).toHaveLength(3)

    expect(files[0]).toMatchObject({
      filename: `src/a.ts`,
      status: `modified`,
      additions: 2,
      deletions: 1,
    })
    // Patch starts at the hunk header — no index/---/+++ noise.
    expect(files[0].patch?.startsWith(`@@ -1,3 +1,4 @@`)).toBe(true)
    expect(files[0].patch).not.toContain(`+++`)

    expect(files[1]).toMatchObject({
      filename: `src/new.ts`,
      status: `added`,
      additions: 2,
      deletions: 0,
    })
    // Deleted files resolve their name from the a/ side.
    expect(files[2]).toMatchObject({
      filename: `gone.txt`,
      status: `removed`,
      additions: 0,
      deletions: 1,
    })
  })

  it(`marks renames and uses the rename target as the filename`, () => {
    const diff = [
      `diff --git a/old/name.ts b/new/name.ts`,
      `similarity index 96%`,
      `rename from old/name.ts`,
      `rename to new/name.ts`,
      `--- a/old/name.ts`,
      `+++ b/new/name.ts`,
      `@@ -1 +1 @@`,
      `-x`,
      `+y`,
    ].join(`\n`)
    const [file] = splitUnifiedDiff(diff)
    expect(file.filename).toBe(`new/name.ts`)
    expect(file.status).toBe(`renamed`)
  })

  it(`leaves patch undefined for binary sections`, () => {
    const diff = [
      `diff --git a/logo.png b/logo.png`,
      `index 111..222 100644`,
      `Binary files a/logo.png and b/logo.png differ`,
    ].join(`\n`)
    const [file] = splitUnifiedDiff(diff)
    expect(file.filename).toBe(`logo.png`)
    expect(file.patch).toBeUndefined()
  })

  it(`returns nothing for empty input`, () => {
    expect(splitUnifiedDiff(``)).toEqual([])
  })
})

describe(`unifiedDiffStats`, () => {
  it(`sums additions and deletions across files`, () => {
    expect(unifiedDiffStats(MULTI)).toEqual({ additions: 4, deletions: 2 })
  })
})
