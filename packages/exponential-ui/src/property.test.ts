// VAPP-107: property tests (fast-check) on the modules the Rust core
// fuzzes (fuzz/): reduce + expand on arbitrary JSON, the expression
// language, JSON pointer read/write, theme loading, the host decoders and
// router, the media header readers and the URL policy. Each law holds for
// EVERY input: never a throw, and the invariants below. The nightly job
// runs them with FC_NUM_RUNS=5000; a PR run keeps the default budget.

import { describe, expect, test } from "bun:test"
import fc from "fast-check"
import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID } from "./catalog"
import { readPointer, resolveDynamic, writePointer } from "./dynamic"
import { evalCondition, evalExpr, evalValue } from "./expr"
import { reduceNested, reduceSurface } from "./reducer"
import { tryLoadTheme, validateTheme } from "./theme"
import { BUILTIN_THEMES, themeOrDefault } from "./themes"
import { ExponentialHost, HostRouter, JsonlDecoder, SseDecoder, decideUrl, decodeJsonl, imageDimensions, imageFrames, isPrivateHost, mcpActionCall, mediaImageWithinLimits, mediaRequest, messagesFromMcpResult, validatePackage } from "./host"
import type { Decoded } from "./host/decoders"
import type { FlatComponent, NestedNode } from "./types"

const runs = Number(process.env.FC_NUM_RUNS ?? 200)
const params = { numRuns: runs }

/** Arbitrary JSON, shallow enough to stay fast. */
const json = fc.jsonValue({ maxDepth: 4 })
/** A component-ish object: an id, a component name, maybe props and children. */
const component = (): fc.Arbitrary<Record<string, unknown>> =>
  fc.record(
    {
      id: fc.oneof(fc.constantFrom(`root`, `a`, `b`, `c`), fc.string({ maxLength: 8 })),
      component: fc.oneof(fc.constantFrom(`Text`, `Box`, `Column`, `Row`, `Button`, `Card`, `List`, `Image`, `Link`, `Nope`), fc.string({ maxLength: 8 })),
      children: fc.array(fc.constantFrom(`root`, `a`, `b`, `c`), { maxLength: 4 }),
      text: json,
      style: json,
      visible: json,
      on: json,
    },
    { requiredKeys: [`id`, `component`] }
  )

describe(`VAPP-107 property: reduce + expand never throw`, () => {
  test(`a flat list or a nested tree of arbitrary components reduces to a tree and issues`, () => {
    fc.assert(
      fc.property(fc.array(component(), { maxLength: 8 }), fc.constantFrom(CORE_CATALOG_ID, A2UI_BASIC_CATALOG_ID), (components, catalogId) => {
        const out = reduceSurface(components as unknown as FlatComponent[], { catalogId })
        expect(Array.isArray(out.issues)).toBe(true)
        expect(JSON.stringify(out).length).toBeGreaterThan(0)
        const first = components[0]
        if (first) {
          const nested = reduceNested({ ...first, children: [] } as unknown as NestedNode, { catalogId })
          expect(nested.root === null || typeof nested.root.id === `string`).toBe(true)
        }
      }),
      params
    )
  })
  test(`prototype-named components, calls and functions find nothing (never Object.prototype)`, () => {
    for (const name of [`constructor`, `__proto__`, `valueOf`, `toString`, `hasOwnProperty`, `__defineGetter__`]) {
      // Both catalogs: the basic one maps through its own table (basic-map.ts).
      for (const catalogId of [CORE_CATALOG_ID, A2UI_BASIC_CATALOG_ID]) {
        const out = reduceSurface([{ id: `root`, component: name }] as FlatComponent[], { catalogId })
        expect(out.root?.component, `${catalogId} ${name}`).toBe(`Unknown`)
        expect(reduceNested({ id: `root`, component: name, children: [] } as unknown as NestedNode, { catalogId }).root?.component, `${catalogId} ${name}`).toBe(`Unknown`)
      }
      expect(() => resolveDynamic({ call: name, args: { a: 1 } }, {})).not.toThrow()
      expect(resolveDynamic({ call: name, args: { a: 1 } }, {}), name).toBeUndefined()
      const host = new ExponentialHost()
      expect(host.decide(`s`, name), name).toBe(`not_found`)
    }
  })
  test(`arbitrary JSON as a component list never throws`, () => {
    fc.assert(
      fc.property(json, (value) => {
        if (!Array.isArray(value)) return
        expect(() => reduceSurface(value as FlatComponent[], { catalogId: CORE_CATALOG_ID })).not.toThrow()
      }),
      params
    )
  })
})

describe(`VAPP-107 property: the expression language`, () => {
  test(`any string evaluates (to something or undefined), any JSON as a value or condition`, () => {
    fc.assert(
      fc.property(fc.string({ maxLength: 64 }), fc.dictionary(fc.string({ maxLength: 6 }), json, { maxKeys: 4 }), json, (expr, props, value) => {
        const ctx = { id: `p`, props, vars: { ...props, item: expr } }
        expect(() => evalExpr(expr, ctx)).not.toThrow()
        expect(() => evalValue(value, ctx)).not.toThrow()
        expect(typeof evalCondition(value, ctx)).toBe(`boolean`)
      }),
      params
    )
  })
})

describe(`VAPP-107 property: JSON pointers`, () => {
  const pointer = fc.oneof(fc.string({ maxLength: 24 }), fc.array(fc.string({ maxLength: 4 }), { maxLength: 5 }).map((segs) => segs.map((s) => `/${s.replace(/~/g, `~0`).replace(/\//g, `~1`)}`).join(``)))
  test(`a write lands and reads back, or is refused and changes nothing`, () => {
    fc.assert(
      fc.property(json, pointer, json, (data, ptr, value) => {
        const before = JSON.stringify(data)
        const written = writePointer(data, ptr, value)
        if (written.error) {
          expect(JSON.stringify(data)).toBe(before)
          expect(typeof written.error).toBe(`string`)
        } else if (ptr !== `` && !ptr.split(`/`).includes(`-`)) {
          // (`-` appends: JSON Pointer has no way to read the element it named.)
          expect(readPointer(written.data, ptr)).toEqual(value)
        } else if (ptr === ``) expect(written.data).toEqual(value)
        expect(() => readPointer(data, ptr)).not.toThrow()
      }),
      params
    )
  })
})

describe(`VAPP-107 property: themes`, () => {
  test(`validate and load agree on arbitrary JSON; the default always stands in`, () => {
    fc.assert(
      fc.property(json, (value) => {
        const issues = validateTheme(value, { themes: BUILTIN_THEMES })
        const loaded = tryLoadTheme(value, { themes: BUILTIN_THEMES })
        expect(issues.length === 0).toBe(loaded.theme !== null)
        const { theme } = themeOrDefault(value)
        expect(typeof theme.id).toBe(`string`)
      }),
      params
    )
  })
})

describe(`VAPP-107 property: the host decoders and router`, () => {
  const feed = (dec: { push(c: string): Decoded; end(): Decoded }, chunks: string[]): Decoded => {
    const out: Decoded = { messages: [], issues: [] }
    for (const c of chunks) {
      const d = dec.push(c)
      out.messages.push(...d.messages)
      out.issues.push(...d.issues)
    }
    const e = dec.end()
    out.messages.push(...e.messages)
    out.issues.push(...e.issues)
    return out
  }
  const cut = (text: string, cuts: number[]): string[] => {
    const out: string[] = []
    let at = 0
    for (const c of cuts) {
      const next = Math.min(text.length, at + c)
      if (next > at) out.push(text.slice(at, next))
      at = next
    }
    out.push(text.slice(at))
    return out
  }
  test(`JSONL pushed in arbitrary chunks decodes the same messages as one push (and decodeJsonl never throws)`, () => {
    fc.assert(
      fc.property(fc.array(json, { maxLength: 6 }), fc.array(fc.nat({ max: 40 }), { maxLength: 12 }), fc.string({ maxLength: 12 }), (values, cuts, noise) => {
        const text = [...values.map((v) => JSON.stringify(v)), noise].join(`\n`)
        const once = feed(new JsonlDecoder(), [text])
        const streamed = feed(new JsonlDecoder(), cut(text, cuts))
        expect(streamed.messages).toEqual(once.messages)
        expect(streamed.issues.length).toBe(once.issues.length)
        // A whole document: one JSON array is its messages, else the lines (a different, documented reading).
        expect(() => decodeJsonl(text)).not.toThrow()
        expect(() => feed(new SseDecoder(), cut(text, cuts))).not.toThrow()
      }),
      params
    )
  })
  test(`arbitrary JSON routes to ops or a send, validates as a package, decodes as an MCP result`, () => {
    fc.assert(
      fc.property(fc.array(json, { maxLength: 6 }), (messages) => {
        const router = new HostRouter()
        for (const m of messages) {
          for (const op of router.route(m)) expect(typeof op.op).toBe(`string`)
          expect(Array.isArray(validatePackage(m))).toBe(true)
          expect(Array.isArray(messagesFromMcpResult(m).messages)).toBe(true)
          expect(mcpActionCall(m as never).method).toBe(`tools/call`)
        }
      }),
      params
    )
  })
})

describe(`VAPP-107 property: media headers and the URL policy`, () => {
  test(`any byte string has readable-or-null dimensions, >= 1 frame, and a pixel verdict`, () => {
    fc.assert(
      fc.property(fc.uint8Array({ maxLength: 96 }), (bytes) => {
        const dims = imageDimensions(bytes)
        expect(dims === null || (dims.length === 2 && dims.every((n) => Number.isInteger(n) && n >= 0))).toBe(true)
        expect(imageFrames(bytes)).toBeGreaterThanOrEqual(1)
        const verdict = mediaImageWithinLimits(bytes)
        expect(verdict === null || typeof verdict === `string`).toBe(true)
      }),
      params
    )
  })
  test(`an allowed url is absolute, a private literal never passes without an opt-in, media never widens the url policy`, () => {
    const policy = fc.record(
      {
        schemes: fc.array(fc.constantFrom(`https`, `http`, `mailto`, `tel`, `data`, `file`, `exponential`), { maxLength: 4 }),
        hosts: fc.array(fc.string({ maxLength: 12 }), { maxLength: 3 }),
        allowPrivate: fc.array(fc.constantFrom(`localhost`, `*`, `*.local`, `127.0.0.1`), { maxLength: 2 }),
        baseUrl: fc.constantFrom(`https://app.exponential.at`, `http://localhost:3000`, `nope`),
      },
      { requiredKeys: [] }
    )
    const url = fc.oneof(
      fc.tuple(fc.constantFrom(`https://`, `http://`, `HTTP://`, `mailto:`, `data:`, `file:///`, `exponential://`, ``, `//`), fc.string({ maxLength: 24 }), fc.constantFrom(``, `/x`, `:3000/y?z=1`)).map(([a, b, c]) => `${a}${b}${c}`),
      fc.constantFrom(`http://127.0.0.1/`, `http://[::1]/`, `http://10.1.1.1/x`, `http://2130706433/`, `http://printer.local/`, `javascript:alert(1)`, `/api/attachments/a`, `https://exponential.at/`)
    )
    fc.assert(
      fc.property(policy, url, (p, u) => {
        const d = decideUrl(p, u)
        if (d.allowed) {
          expect(typeof d.url).toBe(`string`)
          const parsed = new URL(d.url!)
          if (parsed.protocol === `http:` || parsed.protocol === `https:`) {
            if (isPrivateHost(parsed.hostname)) expect((p.allowPrivate ?? []).length).toBeGreaterThan(0)
          }
        } else expect([`invalid`, `scheme`, `host`, `private`]).toContain(d.reason ?? `none`)
        const m = mediaRequest(u, p)
        if (m) expect(decideUrl({ ...p, schemes: p.schemes ?? [`https`, `http`, `data`] }, m.url).allowed).toBe(true)
      }),
      params
    )
  }, 60_000)
})
