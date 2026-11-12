import { describe, expect, it } from "vitest"
import { compactViewResult, isViewListOp, viewListSummary } from "./view-results"

const json = (data: unknown) => ({ content: [{ type: `text`, text: JSON.stringify(data, null, 2) }] })

const issue = (n: number) => ({
  id: `00000000-0000-4000-8000-${String(n).padStart(12, `0`)}`,
  identifier: `EXP-${n}`,
  title: `Issue number ${n} with a long description head`,
  description: `x`.repeat(200),
  status: `backlog`,
  priority: n % 2 ? `high` : `none`,
  assigneeId: null,
  labelIds: [],
})

describe(`compactViewResult (#VAPP-292)`, () => {
  it(`keeps the model's text a summary and hands the view every row`, () => {
    const rows = Array.from({ length: 200 }, (_, i) => issue(i + 1))
    const full = json(rows)
    const out = compactViewResult(`issues`, { limit: 200, offset: 0 }, full)
    const text = (out.content[0] as { text: string }).text
    expect(text.length).toBeLessThan((full.content[0] as { text: string }).text.length / 4)
    expect(text.length).toBeLessThan(25_000)
    expect(text).toContain(`200 issues (1–200)`)
    expect(text).toContain(`- EXP-7 · backlog · high · Issue number 7`)
    expect(text).toContain(`More: call again with offset 200`)
    expect(out.structuredContent).toEqual({ items: rows, page: { offset: 0, limit: 200, nextOffset: 200 } })
  })

  it(`says the last page and an empty one plainly`, () => {
    expect(viewListSummary(`issues`, [], { offset: 0, limit: 50, nextOffset: null })).toBe(`No issues.`)
    const out = compactViewResult(`devices`, { limit: 50, offset: 0 }, json([{ deviceId: `d1`, label: `mint`, online: true, platform: `linux`, agents: [`claude`] }]))
    expect((out.content[0] as { text: string }).text).toContain(`- mint · online · linux · agents claude (deviceId d1)`)
    expect((out.content[0] as { text: string }).text).not.toContain(`More:`)
  })

  it(`passes errors and non-list answers through`, () => {
    const error = { isError: true, content: [{ type: `text`, text: `nope` }] }
    expect(compactViewResult(`issues`, {}, error)).toBe(error)
    const object = json({ id: `x` })
    expect(compactViewResult(`issues`, {}, object)).toBe(object)
  })

  it(`names the view list ops`, () => {
    expect(isViewListOp(`issues`, `show`)).toBe(true)
    expect(isViewListOp(`issues`, `create`)).toBe(false)
    expect(isViewListOp(`notifications`, `list`)).toBe(true)
    expect(isViewListOp(`labels`, `list`)).toBe(false)
  })
})
