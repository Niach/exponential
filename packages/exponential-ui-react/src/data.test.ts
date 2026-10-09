// VAPP-87: the data model, bindings and the client functions.

import { describe, expect, it } from "vitest"
import { absolutePath, boundPath, getPointer, resolveValue, setPointer } from "./data"

describe(`JSON pointers`, () => {
  const data = { user: { name: `Ada`, tags: [`a`, `b`] }, items: [{ title: `one` }, { title: `two` }], "a/b": 1 }
  it(`reads absolute and escaped paths`, () => {
    expect(getPointer(data, `/user/name`)).toBe(`Ada`)
    expect(getPointer(data, `/items/1/title`)).toBe(`two`)
    expect(getPointer(data, `/a~1b`)).toBe(1)
    expect(getPointer(data, `/nope/deeper`)).toBeUndefined()
    expect(getPointer(data, ``)).toBe(data)
  })
  it(`writes immutably, creating OBJECT intermediates (the core's JSON-Pointer rules)`, () => {
    const next = setPointer(data, `/user/name`, `Bob`)
    expect(next.user.name).toBe(`Bob`)
    expect(data.user.name).toBe(`Ada`)
    expect(next.items).toBe(data.items)
    expect(setPointer({}, `/list/0/x`, 1)).toEqual({ list: { 0: { x: 1 } } })
    expect(setPointer({ list: [1] }, `/list/-`, 2)).toEqual({ list: [1, 2] })
    expect(setPointer({ list: [1] }, `/list/1`, 2)).toEqual({ list: [1, 2] })
    // Refused (an index past the end, a key on an array): data unchanged.
    const list = { list: [1] }
    expect(setPointer(list, `/list/3`, 2)).toBe(list)
    expect(setPointer(list, `/list/x`, 2)).toBe(list)
    expect(setPointer({ a: 1, b: 2 }, `/a`, undefined)).toEqual({ b: 2 })
    expect(setPointer({}, ``, { whole: true })).toEqual({ whole: true })
  })
  it(`relative paths resolve against a template scope`, () => {
    expect(absolutePath(`title`, `/items/1`)).toBe(`/items/1/title`)
    expect(absolutePath(`/user/name`, `/items/1`)).toBe(`/user/name`)
    expect(absolutePath(`title`)).toBe(`/title`)
    expect(boundPath({ value: { path: `name` } }, `value`, `/rows/2`)).toBe(`/rows/2/name`)
    expect(boundPath({ value: `literal` }, `value`)).toBeUndefined()
  })
})

describe(`resolveValue`, () => {
  const ctx = { data: { draft: { title: `Hello`, n: 3 }, ok: true, flags: [true, false] } }
  it(`bindings, calls, nested objects and arrays`, () => {
    expect(resolveValue({ path: `/draft/title` }, ctx)).toBe(`Hello`)
    expect(resolveValue({ call: `required`, args: { value: { path: `/draft/title` } } }, ctx)).toBe(true)
    expect(resolveValue({ call: `required`, args: { value: { path: `/draft/missing` } } }, ctx)).toBe(false)
    expect(resolveValue([{ path: `/ok` }, 1], ctx)).toEqual([true, 1])
    expect(resolveValue({ label: { path: `/draft/title` }, value: `x` }, ctx)).toEqual({ label: `Hello`, value: `x` })
  })
  it(`the catalog's 14 functions`, () => {
    const call = (name: string, args: Record<string, unknown>) => resolveValue({ call: name, args }, ctx)
    expect(call(`regex`, { value: `abc`, pattern: `^a` })).toBe(true)
    expect(call(`length`, { value: `abc`, min: 2, max: 3 })).toBe(true)
    expect(call(`length`, { value: `abcd`, max: 3 })).toBe(false)
    expect(call(`numeric`, { value: 5, min: 1, max: 10 })).toBe(true)
    expect(call(`email`, { value: `a@b.co` })).toBe(true)
    expect(call(`email`, { value: `nope` })).toBe(false)
    expect(call(`formatString`, { value: `Hi \${/draft/title}!` })).toBe(`Hi Hello!`)
    expect(call(`formatNumber`, { value: 1234.5, decimals: 1 })).toMatch(/1.?234[.,]5/)
    expect(call(`formatCurrency`, { value: 12, currency: `EUR`, decimals: 0 })).toMatch(/12/)
    expect(call(`formatDate`, { value: `2026-10-14T00:00:00`, format: `yyyy-MM-dd` })).toBe(`2026-10-14`)
    expect(call(`pluralize`, { value: 1, one: `item`, other: `items` })).toBe(`item`)
    expect(call(`pluralize`, { value: 3, one: `item`, other: `items` })).toBe(`items`)
    expect(call(`and`, { values: [{ path: `/ok` }, true] })).toBe(true)
    expect(call(`or`, { values: [false, { path: `/ok` }] })).toBe(true)
    expect(call(`not`, { value: { path: `/ok` } })).toBe(false)
    let opened = ``
    resolveValue({ call: `openUrl`, args: { url: `https://x.y` } }, { ...ctx, openUrl: (u) => (opened = u) })
    expect(opened).toBe(`https://x.y`)
  })
  it(`a host function overrides a catalog one and an unknown call is undefined`, () => {
    expect(resolveValue({ call: `required`, args: { value: `` } }, { ...ctx, functions: { required: () => `host` } })).toBe(`host`)
    expect(resolveValue({ call: `nope`, args: {} }, ctx)).toBeUndefined()
  })
})
