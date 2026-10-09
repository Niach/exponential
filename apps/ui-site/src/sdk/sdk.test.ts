import { expect, test } from "bun:test"
import { reduceSurface } from "@exponential-at/ui"
import type { FlatComponent } from "@exponential-at/ui"
import { COMPONENT_DOCS, SHOT_PLATFORMS, shotUrl, specimenById } from "../lib/catalog"
import { CORE_CATALOG, controlFor, flatten, parseInput, replayChunks, specimenSubject, surfaceIds, type Nested, type PropDoc } from "./a2ui"
import { caseAlign, initialProps, studioTree } from "./ComponentStudio"
import { EXAMPLES } from "./playground-examples"
import { PLATFORMS, shotSrc } from "./platforms"
import { decodeShare, encodeShare, shareTokenFromHash } from "./share"

test(`the sdk's platform list mirrors the catalog's`, () => {
  expect(PLATFORMS).toEqual(SHOT_PLATFORMS)
  expect(shotSrc(`exponential-ui-demo`, `ios`)).toBe(shotUrl(`exponential-ui-demo`, `ios`))
})

test(`every component's studio surface flattens into a valid A2UI surface`, () => {
  for (const doc of COMPONENT_DOCS) {
    const specimen = specimenById(doc.specimenId)
    const subject = specimen ? specimenSubject(specimen.node as unknown as Nested, doc.name) : null
    expect(subject?.component, doc.name).toBe(doc.name)
    const flat = flatten(studioTree(doc, subject, initialProps(doc, subject), caseAlign(specimen)))
    expect(flat[0]!.id).toBe(`root`)
    expect(new Set(flat.map((c) => c.id)).size, doc.name).toBe(flat.length)
    const { root, issues } = reduceSurface(flat as FlatComponent[], { catalogId: CORE_CATALOG })
    expect(root.component === `Unknown`, doc.name).toBe(false)
    expect(issues, doc.name).toEqual([])
    // Every prop gets a control or is deliberately left to the JSON.
    for (const p of doc.props) controlFor(p as PropDoc, undefined)
  }
})

test(`modal overlays start closed, non-modal ones open`, () => {
  const dialog = COMPONENT_DOCS.find((d) => d.name === `Dialog`)!
  const popover = COMPONENT_DOCS.find((d) => d.name === `Popover`)!
  expect(initialProps(dialog, null).open).toBe(false)
  expect(initialProps(popover, null).open).toBe(true)
})

test(`controls follow the prop type, unknown types fall back to JSON or nothing`, () => {
  const p = (type: string): PropDoc => ({ name: `x`, type, required: false, bindable: false, description: `` })
  expect(controlFor(p(`sm | default | lg`), undefined)).toEqual({ kind: `enum`, name: `x`, options: [`sm`, `default`, `lg`] })
  expect(controlFor(p(`boolean`), undefined)?.kind).toBe(`boolean`)
  expect(controlFor(p(`number`), undefined)?.kind).toBe(`number`)
  expect(controlFor(p(`string`), { path: `/a` })?.kind).toBe(`json`)
  expect(controlFor(p(`[series]`), [1])?.kind).toBe(`json`)
  expect(controlFor(p(`some-future-type`), undefined)).toBeNull()
})

test(`the replay grows and every chunk is a valid surface`, () => {
  const demo = specimenById(`exponential-ui-demo`)!.node as unknown as Nested
  const flat = flatten(demo)
  const replay = replayChunks(`release`, flat, 1)
  expect(replay[0]!.createSurface?.surfaceId).toBe(`release`)
  expect(replay.length).toBe(flat.length + 1)
  for (const m of replay.slice(1)) {
    const { issues } = reduceSurface(m.updateComponents!.components as FlatComponent[], { catalogId: CORE_CATALOG })
    expect(issues).toEqual([])
  }
  expect(replay.at(-1)!.updateComponents!.components).toEqual(flat)
})

test(`the playground reads every input form`, () => {
  const flat = [{ id: `root`, component: `Text`, text: `hi` }]
  expect(parseInput(JSON.stringify(flat)).form).toBe(`flat`)
  expect(parseInput(JSON.stringify({ id: `a`, component: `Text`, props: { text: `hi` } })).form).toBe(`nested`)
  const msgs = [{ createSurface: { surfaceId: `s`, catalogId: CORE_CATALOG } }, { updateComponents: { surfaceId: `s`, components: flat } }]
  expect(parseInput(JSON.stringify(msgs)).form).toBe(`messages`)
  const jsonl = parseInput(msgs.map((m) => JSON.stringify(m)).join(`\n`))
  expect(jsonl.form).toBe(`jsonl`)
  expect(surfaceIds(jsonl.messages)).toEqual([`s`])
  expect(parseInput(`{ nope`).errors.length).toBe(1)
  expect(parseInput(``).form).toBe(`empty`)
  for (const ex of EXAMPLES) {
    const parsed = parseInput(ex.source)
    expect(parsed.errors, ex.id).toEqual([])
    expect(parsed.messages.length, ex.id).toBeGreaterThan(0)
  }
})

test(`share state round-trips through the hash`, async () => {
  const state = { v: 1, source: EXAMPLES[1]!.source, theme: `playful`, mode: `light` }
  const token = await encodeShare(state)
  expect(token).toMatch(/^[A-Za-z0-9_-]+$/)
  expect(shareTokenFromHash(`#s=${token}`)).toBe(token)
  expect(await decodeShare<typeof state>(token)).toEqual(state)
  expect(await decodeShare(`not-a-token`)).toBeNull()
})
