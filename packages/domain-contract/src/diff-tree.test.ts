// EXP-916: the fixture is the contract. Every mirror (web `@exp/ui`
// `FileDiffTree`, desktop `domain::diff_tree`, iOS `DiffTree.swift`, Android
// `DiffTree.kt`) replays `fixtures/diff/tree.json` with THESE test names.
//
// A case: `files` (path + counts; status is irrelevant to the tree and
// defaults to `modified`), an optional `query`, and `expected` =
// `renderDiffTree(diffFileTree(files, query))`.

import { describe, expect, test } from "bun:test"

import cases from "../fixtures/diff/tree.json" with { type: "json" }
import type { DiffFile } from "./diff"
import { diffFileTree, renderDiffTree } from "./diff-tree"

interface Case {
  name: string
  files: { path: string; additions: number; deletions: number }[]
  query?: string
  expected: string[]
}

const fixture = cases as unknown as Case[]

function toFile(file: Case[`files`][number]): DiffFile {
  return {
    path: file.path,
    status: `modified`,
    additions: file.additions,
    deletions: file.deletions,
    binary: false,
    hunks: [],
  }
}

describe(`diff file tree (EXP-916)`, () => {
  test(`every fixture case renders byte-exact`, () => {
    for (const item of fixture) {
      const tree = diffFileTree(item.files.map(toFile), item.query ?? ``)
      expect({ name: item.name, rows: renderDiffTree(tree) }).toEqual({
        name: item.name,
        rows: item.expected,
      })
    }
  })

  test(`the fixture covers a compaction, a query and an empty input`, () => {
    const names = fixture.map((item) => item.name).join(`\n`)
    expect(names).toContain(`compacts`)
    expect(names).toContain(`query`)
    expect(names).toContain(`no files`)
  })

  test(`a file node keeps its input index and full path`, () => {
    const files = [`src/b.ts`, `src/a.ts`, `top.ts`].map((path) =>
      toFile({ path, additions: 1, deletions: 0 })
    )
    const tree = diffFileTree(files)
    expect(tree.map((node) => [node.kind, node.path, node.index])).toEqual([
      [`dir`, `src`, -1],
      [`file`, `top.ts`, 2],
    ])
    expect(tree[0].children.map((node) => [node.path, node.index])).toEqual([
      [`src/a.ts`, 1],
      [`src/b.ts`, 0],
    ])
  })

  test(`a compacted directory's path is the deepest segment's`, () => {
    const tree = diffFileTree([
      toFile({ path: `apps/web/src/a.ts`, additions: 1, deletions: 0 }),
    ])
    expect(tree[0].name).toBe(`apps/web/src`)
    expect(tree[0].path).toBe(`apps/web/src`)
  })
})
