// FEED-128: the pure helpers the web renderers share (React's DOM suites
// cover them through the painters too).

import { describe, expect, it } from "bun:test"
import { acceptsFile, failingChecks, formatFileSize, joinIds, num, phrase, str } from "./coerce"
import { sortRows } from "./table"
import { calendarStop, monthGrid } from "./dates"
import { HostOwnedController, parseLocaleNumber } from "./inputs"
import { linkAt, parseList, parseMarkdown } from "./markdown"
import { FormRegistry, fieldErrorsVisible, type FormField } from "./form"
import { nextEnabledIndex, typeaheadStep } from "./listbox"
import { acquireMedia, clearMediaCache, fetchLimited, mediaCacheStats, releaseMedia } from "./media"

describe(`coercions`, () => {
  it(`str / num / checks / files / ids`, () => {
    expect(str({ a: 1 })).toBe(`{"a":1}`)
    expect(num(` 4 `)).toBe(4)
    expect(num(`x`, 7)).toBe(7)
    expect(failingChecks([{ condition: false, message: `a` }, { condition: true, message: `b` }, { condition: `` }], `fallback`)).toEqual([`a`, `fallback`])
    expect(formatFileSize(1536, `de-DE`)).toBe(`1,5 kB`)
    expect(acceptsFile(`.pdf`, { name: `X.PDF`, type: `` })).toBe(true)
    expect(acceptsFile(`image/*`, { name: `a.txt`, type: `text/plain` })).toBe(false)
    expect(joinIds(`a`, false, undefined, `b`)).toBe(`a b`)
    expect(joinIds()).toBeUndefined()
    const ctx = (strings: Record<string, string>) => ({ strings, t: (id: string, p?: Record<string, unknown>) => strings[id]!.replace(`{name}`, String(p?.name)) })
    expect(phrase(ctx({ removeItem: `{name} entfernen` }), `removeItem`, { name: `a.txt` }, () => `fallback`)).toBe(`a.txt entfernen`)
  })
})

describe(`table, dates, numbers`, () => {
  it(`sorts locally with missing values last`, () => {
    expect(sortRows([{ v: `b` }, { v: `a` }, {}], { key: `v`, direction: `asc` }, { key: `v`, label: `V` }, `en`)).toEqual([1, 0, 2])
    expect(sortRows([{ v: 2 }, { v: 10 }], { key: `v`, direction: `desc` }, { key: `v`, type: `number` }, `en`)).toEqual([1, 0])
  })
  it(`month grids start on the first weekday; the tab stop skips disabled days`, () => {
    expect(monthGrid(new Date(2026, 9, 1), 1)[0]!.getDay()).toBe(1)
    const view = new Date(2026, 9, 1)
    const days = monthGrid(view, 1)
    expect(calendarStop(days, view, new Date(2026, 9, 8), (d) => d < new Date(2026, 9, 20))!.getDate()).toBe(20)
    expect(calendarStop(days, view, new Date(2026, 9, 25), () => true)).toBeNull()
  })
  it(`parses locale numbers and digits`, () => {
    expect(parseLocaleNumber(`1.234,5`, `de-DE`)).toBe(1234.5)
    expect(parseLocaleNumber(`٥`, `ar-EG`)).toBe(5)
    expect(parseLocaleNumber(`-`, `en-US`)).toBeNull()
  })
})

describe(`HostOwnedController`, () => {
  it(`debounces changes, commits at once, adopts an echo only when idle and acknowledged`, async () => {
    const sent: [string, number, string][] = []
    let resolve: () => void = () => {}
    let changes = 0
    const c = new HostOwnedController<string>({
      initial: ``,
      debounceMs: 5,
      send: (v, rev, kind) => {
        sent.push([v, rev, kind])
        return new Promise<void>((r) => (resolve = r))
      },
      onChange: () => changes++,
    })
    c.focus()
    c.edit(`a`)
    c.edit(`ab`)
    expect(c.pending).toBe(true)
    expect(c.revision).toBe(2)
    await new Promise((r) => setTimeout(r, 15))
    expect(sent).toEqual([[`ab`, 2, `change`]])
    // A stale echo while focused / unacknowledged is ignored.
    expect(c.push(`a`)).toBe(false)
    c.blur()
    expect(sent[1]).toEqual([`ab`, 2, `commit`])
    expect(c.push(`a`)).toBe(false)
    resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(c.pending).toBe(false)
    expect(c.push(`server`)).toBe(true)
    expect(c.value).toBe(`server`)
    expect(changes).toBeGreaterThan(0)
    c.dispose()
  })
})

describe(`markdown tree`, () => {
  it(`parses blocks and inlines with raw destinations`, () => {
    const tree = parseMarkdown(`# Hi **you**\n\nsee [docs](https://x/A_(b)) and \`code\`\n\n![alt](/a.png)\n\n> q\n\n| a | b |\n| :-- | --: |\n| 1 | 2 |\n\n---`)
    expect(tree.map((b) => b.type)).toEqual([`heading`, `paragraph`, `image`, `quote`, `table`, `rule`])
    expect(tree[0]).toEqual({ type: `heading`, level: 1, children: [`Hi `, { type: `strong`, children: [`you`] }] })
    expect(tree[1]).toEqual({ type: `paragraph`, children: [`see `, { type: `link`, dest: `https://x/A_(b)`, children: [`docs`] }, ` and `, { type: `code`, text: `code` }] })
    expect(tree[4]).toMatchObject({ type: `table`, align: [`start`, `end`] })
    expect(linkAt(`[a](x y)`, 0)).toBeNull()
    const list = parseList([`- a`, `  - a1`, `- [x] *done*`])
    expect(list.items[1]).toMatchObject({ task: true, content: [{ type: `em`, children: [`done`] }] })
    expect(list.items[0]!.children[0]!.items[0]!.text).toBe(`a1`)
  })
})

describe(`form registry`, () => {
  it(`collects every failure and the first invalid field, else the values`, () => {
    const reg = new FormRegistry()
    let bad = true
    const field = (name: string, value: unknown, errors: () => string[]): FormField => ({ name, domId: name, value: () => value, errors, focus: () => {}, element: () => null })
    const a = field(`a`, 1, () => [])
    const b = field(`b`, 2, () => (bad ? [`b1`, `b2`] : []))
    reg.register(a)
    const off = reg.register(b)
    const failed = reg.check()
    expect(failed).toEqual({ ok: false, errors: [{ name: `b`, message: `b1` }, { name: `b`, message: `b2` }], first: b })
    bad = false
    expect(reg.stillFailing([{ name: `b`, message: `b1` }])).toEqual([])
    expect(reg.check()).toEqual({ ok: true, values: { a: 1, b: 2 } })
    off()
    expect(reg.check()).toEqual({ ok: true, values: { a: 1 } })
    expect(fieldErrorsVisible({ validateOn: `blur`, submitted: false, attempts: 0, dirty: true, touched: false })).toBe(false)
    expect(fieldErrorsVisible({ validateOn: `change`, submitted: false, attempts: 0, dirty: true, touched: false })).toBe(true)
  })
})

describe(`listbox`, () => {
  it(`wraps past disabled options; type-ahead by label`, () => {
    expect(nextEnabledIndex(3, 2, 1, (i) => i === 0)).toBe(1)
    expect(nextEnabledIndex(0, 0, 1)).toBe(-1)
    const labels = [`apple`, `banana`, `blueberry`]
    const step = typeaheadStep({ text: ``, at: 0 }, `b`, 10_000, 0, 3, () => false, (i) => labels[i]!)
    expect(step.index).toBe(1)
    expect(typeaheadStep(step.buffer, `l`, 10_100, 1, 3, () => false, (i) => labels[i]!).index).toBe(2)
  })
})

describe(`media`, () => {
  it(`fetchLimited refuses an oversized body`, async () => {
    const real = globalThis.fetch
    globalThis.fetch = (async () => new Response(new Uint8Array(200), { headers: { "content-length": `200` } })) as unknown as typeof fetch
    try {
      await expect(fetchLimited(`https://x.test/a`, {}, { maxBytes: 100, timeoutMs: 1000, maxPixels: 1000 })).rejects.toThrow(/over 100 bytes/)
    } finally {
      globalThis.fetch = real
    }
  })
  it(`the cache keeps entries in use and drops idle ones on clear`, async () => {
    clearMediaCache()
    const e = acquireMedia(`k`, async () => `blob:x`)
    await e.promise
    expect(mediaCacheStats()).toEqual({ entries: 1, inUse: 1 })
    releaseMedia(`k`, e)
    expect(mediaCacheStats()).toEqual({ entries: 1, inUse: 0 })
    clearMediaCache()
    expect(mediaCacheStats()).toEqual({ entries: 0, inUse: 0 })
  })
})
