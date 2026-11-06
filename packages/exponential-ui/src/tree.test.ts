// #VAPP-285: fixtures/tree.json replays (the Rust core's tests/tree_fixture.rs
// replays the same file).

import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/tree.json"
import { treeKey, treeRows, type TreeKeyRow } from "./tree"

describe(`fixtures/tree.json`, () => {
  test(`treeRows: the flat rows for every expanded set`, () => {
    expect(fixture.rows.length).toBeGreaterThan(5)
    for (const c of fixture.rows) expect(JSON.parse(JSON.stringify(treeRows(fixture.items, c.expanded))), c.name).toEqual(c.rows)
  })

  test(`treeKey: every key on every row, ltr and rtl`, () => {
    for (const c of fixture.keys) expect(treeKey(fixture.keyRows as TreeKeyRow[], c.index, c.key, c.rtl), `${c.index} ${c.key} ${c.rtl}`).toEqual(c.result as never)
  })

  test(`a literal expanded hides closed subtrees, a lazy open branch gets its loading row`, () => {
    const rows = (expanded: string[]) => treeRows(fixture.items, expanded).map((r) => `${r.depth}:${r.id}:${r.kind}`)
    expect(rows([])).toEqual([`0:apps:branch`, `0:packages:branch`, `0:README.md:leaf`, `0:package.json:leaf`])
    expect(rows([`apps/web`])).toEqual(rows([]))
    expect(rows([`apps`, `apps/desktop`]).slice(0, 5)).toEqual([`0:apps:branch`, `1:apps/web:branch`, `1:apps/desktop:branch`, `2:apps/desktop/loading:loading`, `0:packages:branch`])
  })

  test(`items without an id are skipped; a missing title reads the id`, () => {
    expect(treeRows([{ title: `no id` }, { id: `a` }, `x`, null], []).map((r) => [r.id, r.title])).toEqual([[`a`, `a`]])
  })
})
