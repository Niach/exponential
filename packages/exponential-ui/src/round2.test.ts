// Round 2 (docs/round-2-contract.md): the TS reference replays every round-2
// fixture, and the decisions are pinned (the Rust core replays the same files
// with these test names).

import { describe, expect, test } from "bun:test"
import formatFixture from "../fixtures/format.json" with { type: "json" }
import itemsFixture from "../fixtures/template-items.json" with { type: "json" }
import directionFixture from "../fixtures/text-direction.json" with { type: "json" }
import resizableFixture from "../fixtures/resizable.json" with { type: "json" }
import listFixture from "../fixtures/virtual-list.json" with { type: "json" }
import animationsFixture from "../fixtures/animations.json" with { type: "json" }
import benchFixture from "../fixtures/bench-list.json" with { type: "json" }
import kitchenExpanded from "../fixtures/kitchen-sink.expanded.json" with { type: "json" }
import knownFixture from "../fixtures/conformance-known.json" with { type: "json" }
import manifest from "../conformance/manifest.json" with { type: "json" }
import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import { bindSectionHeader, bindTree, resolveDynamic } from "./dynamic"
import { displayString, englishFormatter, fixedOffset, formatFunctions, intlFormatter, intlZoneOffset, type Formatter } from "./format"
import { itemExtents, itemOffsets, listSections, scrollOffsetForIndex, scrollOffsetForItem, sectionRows, stickyHeader, tableRowKeys, templateInstances, templateItemKeys, virtualWindow, type ScrollAlign } from "./list"
import { dragDelta, keyboardResize, normalizeSizes, panelExtents, resizePanels, type PanelLimits } from "./resizable"
import { nodeDirections, nodeTextAlign, physicalTextAlign, type Direction } from "./direction"
import { ANIMATION_NAMES, ANIMATION_PROPERTIES_CSS, animationFrame, animationTiming, keyframesCss, paintedOpacity } from "./animation"
import { preorder, reduceNested, reduceSurface } from "./reducer"
import { validateStyle } from "./style"
import { styleToCss } from "./css"
import { builtinTheme } from "./themes"
import { validateTheme } from "./theme"
import { DEFAULT_STRINGS } from "./strings"
import { A11Y_COMMANDS, COMPONENT_A11Y } from "./a11y"
import { catalogPrompt } from "./prompt"
import { LAYOUT_CONSTANTS } from "./layout"
import type { ChildTemplate, FlatComponent, NestedNode, UiNode } from "./types"

const canon = (v: unknown) => JSON.stringify(v)
const close = (a: number, b: number, eps: number) => Math.abs(a - b) <= eps

describe(`format.json (§3)`, () => {
  test(`every format call resolves to the fixture through the English fallback`, () => {
    for (const c of formatFixture.calls) expect(resolveDynamic(c.call, {}), c.name).toEqual(c.expected)
  })

  // CLDR 42+ ICUs (macOS 27, Safari) join a medium date and its time with " at ", older ones (Node, Chromium) with ", ":
  // one expectation, so the fixture's spelling is not a platform lottery.
  const dateTimeJoiner = (v: unknown) => (typeof v === `string` ? v.replace(` at `, `, `) : v)

  test(`the English fallback equals Intl en-US (the platform ICU) on every call`, () => {
    const intl = formatFunctions(intlFormatter(`en-US`, `UTC`)) as Record<string, (args: Record<string, unknown>) => unknown>
    for (const c of formatFixture.calls) expect(dateTimeJoiner(intl[c.call.call](c.call.args)), c.name).toEqual(dateTimeJoiner(c.expected))
  })

  test(`zoned: the fallback at the host's offset = Intl in the IANA zone`, () => {
    for (const c of formatFixture.zoned) {
      const call = c.call.call as `formatDate`
      expect(formatFunctions(englishFormatter(fixedOffset(c.offsetMinutes)))[call](c.call.args), c.name).toBe(c.expected)
      expect(dateTimeJoiner(formatFunctions(intlFormatter(`en-US`, c.timeZone))[call](c.call.args)), `${c.name} (Intl)`).toBe(dateTimeJoiner(c.expected))
      const at = typeof c.call.args.value === `number` ? c.call.args.value : Date.parse(String(c.call.args.value))
      if (!/^\d{4}-\d{2}-\d{2}$/.test(String(c.call.args.value))) expect(intlZoneOffset(c.timeZone)(at), `${c.name} offset`).toBe(c.offsetMinutes)
    }
    // Decided by hand (not by the reference): the zone moves an instant, never a calendar day.
    const berlin = formatFunctions(englishFormatter(intlZoneOffset(`Europe/Berlin`)))
    expect(berlin.formatDate({ value: `2026-10-14T22:30:00Z`, format: `HH:mm` })).toBe(`00:30`)
    expect(berlin.formatDate({ value: `2026-01-14T22:30:00Z`, format: `HH:mm` })).toBe(`23:30`)
    expect(berlin.formatDate({ value: `2026-10-14`, format: `yyyy-MM-dd HH:mm` })).toBe(`2026-10-14 00:00`)
  })

  test(`numeric strings: decimal literals only (decided by hand)`, () => {
    const fn = formatFunctions(englishFormatter())
    const cases: [unknown, string][] = [[` 12 `, `12`], [`1e3`, `1,000`], [`-.5`, `-0.5`], [`+7`, `7`], [`0x10`, ``], [`Infinity`, ``], [`1,000`, ``], [``, ``], [`1_000`, ``], [`12px`, ``]]
    for (const [value, want] of cases) expect(fn.formatNumber({ value }), String(value)).toBe(want)
  })

  test(`a surface Formatter replaces the fallback (locale + time zone)`, () => {
    const de = intlFormatter(`de-DE`, `Europe/Berlin`)
    expect(resolveDynamic({ call: `formatNumber`, args: { value: 1234.5678 } }, {}, { formatter: de })).toBe(`1.234,568`)
    expect(resolveDynamic({ call: `formatDate`, args: { value: `2026-10-14T22:30:00Z`, format: `HH:mm` } }, {}, { formatter: de })).toBe(`00:30`)
    expect(resolveDynamic({ call: `formatRelativeTime`, args: { value: 1000 } }, {}, { now: () => 61_000 })).toBe(`1 minute ago`)
    const loud: Formatter = { ...englishFormatter(), number: (v) => `#${v}` }
    expect(resolveDynamic({ call: `formatNumber`, args: { value: 3 } }, {}, { formatter: loud })).toBe(`#3`)
  })

  test(`display strings: ECMAScript Number→String, booleans, nothing else`, () => {
    for (const d of formatFixture.display) expect(displayString(d.value), d.name).toBe(d.expected)
    expect(displayString(-0)).toBe(`0`)
    expect(displayString(Number.NaN)).toBe(``)
  })
})

describe(`template-items.json (§4)`, () => {
  test(`item keys and row keys`, () => {
    for (const c of itemsFixture.keys) expect(templateItemKeys(c.items, c.key), c.name).toEqual(c.expected)
    for (const c of itemsFixture.rowKeys) expect(tableRowKeys(c.rows, c.rowKey), c.name).toEqual(c.expected)
  })

  test(`instances and accumulated suffixes`, () => {
    for (const c of itemsFixture.instances as { name: string; data: unknown; template: ChildTemplate; inner?: ChildTemplate; scope: string; instance: string; expected: unknown[] }[]) {
      if (!c.inner) {
        expect(templateInstances(c.data, c.template, c.scope, c.instance), c.name).toEqual(c.expected as never)
        continue
      }
      const nested = templateInstances(c.data, c.template, c.scope, c.instance).flatMap((o) =>
        templateInstances(c.data, c.inner!, o.path, o.instance).map((i) => ({ outer: o.key, ...i, ids: { issue: `issue${i.instance}`, title: `issue.title${i.instance}` } }))
      )
      expect(nested, c.name).toEqual(c.expected as never)
    }
  })

  test(`reduce lifts template nodes out of the tree (nested and flat)`, () => {
    for (const c of itemsFixture.reduce as { name: string; catalogId: string; nested?: NestedNode; components?: FlatComponent[]; expected: unknown }[]) {
      const got = c.nested ? reduceNested(c.nested, { catalogId: c.catalogId }) : reduceSurface(c.components!, { catalogId: c.catalogId })
      expect(canon(got), c.name).toBe(canon(c.expected))
      const ids = new Set(preorder(got.root))
      for (const id of Object.keys(got.templates ?? {})) expect(ids.has(id), `${c.name}: ${id} in place`).toBe(false)
    }
  })

  test(`the kitchen sink's list template renders only per item`, () => {
    const result = kitchenExpanded as unknown as { root: UiNode; templates?: Record<string, UiNode> }
    expect(Object.keys(result.templates ?? {})).toEqual([`list-item`])
    expect(preorder(result.root)).not.toContain(`list-item`)
  })
})

describe(`decisions, by hand (the fixtures above come from the reference)`, () => {
  test(`keys: missing, empty, duplicate → #<index>; suffixes escape . and ~`, () => {
    expect(templateItemKeys([{ id: `a` }, {}, { id: `` }, { id: `a` }], `id`)).toEqual([`a`, `#1`, `#2`, `#3`])
    expect(templateItemKeys([{ id: `#1` }, {}], `id`)).toEqual([`#1`, `##1`])
    expect(templateItemKeys([{ id: 7 }, { id: `7` }], `id`)).toEqual([`7`, `#1`])
    expect(tableRowKeys([{ id: `x` }, { id: `x` }])).toEqual([`x`, `#1`])
    const data = { o: [{ k: `a.b`, i: [{ k: `c` }] }, { k: `a`, i: [{ k: `b.c` }] }] }
    const ids = templateInstances(data, { component: `o`, path: `/o`, key: `k` }).flatMap((o) => templateInstances(data, { component: `i`, path: `i`, key: `k` }, o.path, o.instance).map((x) => x.instance))
    expect(ids).toEqual([`.a~1b.c`, `.a.b~1c`])
    expect(templateInstances({ l: [{ k: `~1` }] }, { component: `x`, path: `/l`, key: `k` })[0].instance).toBe(`.~01`)
  })

  test(`resize: collapse below half min, reopen at max(min, pointer)`, () => {
    const L: PanelLimits[] = [{ min: 20, max: 60 }, { min: 10 }, { min: 15, collapsible: true }]
    expect(resizePanels([40, 30, 30], 1, 25, L)).toEqual([40, 60, 0])
    expect(resizePanels([40, 30, 30], 1, 20, L)).toEqual([40, 45, 15])
    expect(resizePanels([40, 60, 0], 1, -8, L)).toEqual([40, 45, 15])
    expect(resizePanels([40, 60, 0], 1, -5, L)).toEqual([40, 60, 0])
    expect(resizePanels([0, 100], 0, 50, [{ min: 20, collapsible: true }, {}])).toEqual([50, 50])
    expect(resizePanels([100, 0], 0, -50, [{}, { min: 20, collapsible: true }])).toEqual([50, 50])
    expect(resizePanels([0, 100], 0, 80, [{ min: 20, max: 60, collapsible: true }, {}])).toEqual([60, 40])
    expect(resizePanels([0, 100], 0, 9, [{ min: 20, collapsible: true }, {}])).toEqual([0, 100])
  })

  test(`keys: rtl flips Left/Right, Enter toggles collapse`, () => {
    expect(keyboardResize([50, 50], 0, `ArrowRight`, `horizontal`, `rtl`)).toEqual([40, 60])
    expect(keyboardResize([50, 50], 0, `ArrowUp`, `horizontal`, `ltr`)).toEqual([50, 50])
    expect(keyboardResize([30, 70], 0, `Enter`, `horizontal`, `ltr`, [{ min: 20, collapsible: true }, {}])).toEqual([0, 100])
    expect(keyboardResize([0, 100], 0, `Enter`, `horizontal`, `ltr`, [{ min: 20, collapsible: true }, {}])).toEqual([20, 80])
  })

  test(`scrollToIndex on a sectioned list: data index → row, under its own sticky header`, () => {
    // by day: [h Today, 1, 2] [h Yesterday, 3] [h Today, 4] [h '', 5, 6] — data index 3 = the 4th item (id 4), row 6 at 32+40+40+32+40+32 = 216
    const rows = sectionRows(listSections([{ d: `T` }, { d: `T` }, { d: `Y` }, { d: `T` }, {}, {}], `d`))
    const ext = rows.map((r) => (`header` in r ? 32 : 40))
    expect(scrollOffsetForItem(rows, ext, 0, 3, 100, 0, `start`, true)).toBe(216 - 32)
    expect(scrollOffsetForItem(rows, ext, 0, 3, 100, 0, `start`, false)).toBe(216)
    expect(scrollOffsetForItem(rows, ext, 0, 99, 100, 24, `start`, true)).toBe(24)
  })

  test(`sticky: the next header pushes the pinned one back`, () => {
    // rows: header 32, two items 40, header 32, item 40 → offsets 0, 32, 72, 112, 144
    const ext = [32, 40, 40, 32, 40]
    const off = itemOffsets(ext, 0)
    expect(stickyHeader(off, ext, [0, 3], 0)).toEqual({ row: 0, offset: 0 })
    expect(stickyHeader(off, ext, [0, 3], 90)).toEqual({ row: 0, offset: 80 })
    expect(stickyHeader(off, ext, [0, 3], 112)).toEqual({ row: 3, offset: 112 })
  })

  test(`templates: a self-instantiating template is reported and stays in place`, () => {
    const flat: FlatComponent[] = [
      { id: `root`, component: `Box`, children: [`col`] },
      { id: `col`, component: `Box`, children: [`l`] },
      { id: `l`, component: `List`, children: { componentId: `col`, path: `/items` } },
    ]
    const r = reduceSurface(flat, { catalogId: CORE_CATALOG_ID })
    expect(r.issues).toEqual([{ id: `col`, message: `template: cycle through this id` }])
    expect(r.templates).toBeUndefined()
    expect(preorder(r.root)).toEqual([`root`, `col`, `l`])
    const self = reduceNested({ id: `root`, component: `Box`, children: [{ id: `l`, component: `List`, template: { component: `l`, path: `/x` } }] }, { catalogId: CORE_CATALOG_ID })
    expect(self.issues).toEqual([{ id: `l`, message: `template: cycle through this id` }])
    expect(preorder(self.root)).toEqual([`root`, `l`])
  })

  test(`animation opacity multiplies the node's own`, () => {
    const neutral = builtinTheme(`neutral`)
    const d = animationTiming(`pulse`, neutral)!.durationMs
    expect(paintedOpacity(0.5, animationFrame(`pulse`, d / 2, neutral))).toBe(0.25)
    expect(paintedOpacity(0.6, animationFrame(`fade-in`, 10_000, neutral))).toBe(0.6)
    expect(styleToCss({ opacity: 0.6, animation: `fade-in` }, {}, neutral).opacity).toBe(`calc(0.6 * var(--xui-a-opacity, 1))`)
    expect(keyframesCss(`fade-in`)).not.toContain(`;opacity`)
    expect(keyframesCss(`fade-in`)).not.toMatch(/\{opacity/)
  })
})

describe(`text-direction.json (§2)`, () => {
  test(`per-node direction and physical alignment`, () => {
    for (const c of directionFixture.cases) {
      const { root } = reduceNested(c.tree as NestedNode, { catalogId: CORE_CATALOG_ID })
      const dirs = nodeDirections(root, c.surface as Direction)
      const got: Record<string, unknown> = {}
      const walk = (n: UiNode) => {
        got[n.id] = { direction: dirs[n.id], textAlign: nodeTextAlign(n, dirs[n.id]) }
        n.children.forEach(walk)
      }
      walk(root)
      expect(got, c.name).toEqual(c.expected)
    }
    expect(physicalTextAlign(undefined, `rtl`)).toBe(`right`)
    expect(physicalTextAlign(`justify`, `rtl`)).toBe(`justify`)
  })

  test(`direction is valid on any node`, () => {
    expect(validateStyle({ direction: `rtl` }, { root: false })).toEqual([])
  })
})

describe(`resizable.json (§1)`, () => {
  const near = (a: number[], b: number[]) => a.length === b.length && a.every((v, i) => close(v, b[i], 1e-6))
  test(`normalize, resize, keys, extents, drag`, () => {
    for (const c of resizableFixture.normalize) expect(near(normalizeSizes(c.sizes as unknown[] | undefined, c.count, c.panels as PanelLimits[] | undefined), c.expected), c.name).toBe(true)
    for (const c of resizableFixture.resize) expect(near(resizePanels(c.sizes, c.handle, c.delta, c.panels as PanelLimits[] | undefined), c.expected), c.name).toBe(true)
    for (const c of resizableFixture.keys) expect(near(keyboardResize(c.sizes, c.handle, c.key, c.orientation as `horizontal`, c.direction as `ltr`, c.panels as PanelLimits[] | undefined), c.expected), c.name).toBe(true)
    for (const c of resizableFixture.extents) expect(near(panelExtents(c.sizes, c.container, c.handle), c.expected), c.name).toBe(true)
    for (const c of resizableFixture.drag) expect(close(dragDelta(c.px, c.container, c.panels, c.orientation as `horizontal`, c.direction as `ltr`), c.expected, 1e-6), c.name).toBe(true)
  })

  test(`sizes always sum to 100`, () => {
    const sum = (a: number[]) => a.reduce((s, v) => s + v, 0)
    for (const c of [...resizableFixture.normalize, ...resizableFixture.resize, ...resizableFixture.keys]) expect(close(sum(c.expected), 100, 1e-5), c.name).toBe(true)
  })
})

describe(`virtual-list.json (§5)`, () => {
  test(`unmeasured items: \`row\` until one is measured, then the mean`, () => {
    expect(itemExtents([null, null], 36)).toEqual([36, 36])
    expect(itemExtents([40, null, 60, undefined], 36)).toEqual([40, 50, 60, 50])
    expect(itemExtents([], 36)).toEqual([])
  })
  const expand = (e: number[] | { count: number; extent: number }) => (Array.isArray(e) ? e : Array.from({ length: e.count }, () => e.extent))
  test(`windows, scrollToIndex, sections, sticky headers`, () => {
    for (const c of listFixture.windows) expect(virtualWindow(expand(c.extents), c.gap, c.scroll, c.viewport, c.overscan), c.name).toEqual(c.expected)
    for (const c of listFixture.windows as { name: string; measured?: (number | null)[]; row?: number; extents: unknown }[]) if (c.measured) expect(itemExtents(c.measured, c.row ?? 0), c.name).toEqual(c.extents as number[])
    for (const c of listFixture.scrollTo) expect(scrollOffsetForIndex(expand(c.extents), c.gap, c.index, c.viewport, c.scroll, c.align as ScrollAlign, c.inset), c.name).toBe(c.expected)
    for (const c of listFixture.sections) {
      const sections = listSections(c.items, c.sectionBy)
      expect({ sections, rows: sectionRows(sections) }, c.name).toEqual(c.expected)
    }
    for (const c of listFixture.sectionedScrollTo) {
      const rows = sectionRows(listSections(c.items, c.sectionBy))
      const ext = rows.map((r) => (`header` in r ? c.headerExtent : c.itemExtent))
      expect(scrollOffsetForItem(rows, ext, c.gap, c.index, c.viewport, c.scroll, c.align as ScrollAlign, c.stickyHeaders), c.name).toBe(c.expected)
    }
    for (const c of listFixture.sticky) {
      const offsets = itemOffsets(c.rowExtents, 0)
      expect(c.scrolls.map((s) => stickyHeader(offsets, c.rowExtents, c.headerRows, s)), c.name).toEqual(c.expected)
    }
  })

  test(`the bench surface reduces cleanly and its template is lifted`, () => {
    const result = reduceSurface(benchFixture.components as FlatComponent[], { catalogId: benchFixture.catalogId })
    expect(result.issues).toEqual([])
    expect(Object.keys(result.templates ?? {})).toEqual([`bench-row`])
    expect(benchFixture.rows.count).toBe(100_000)
  })

  test(`List sections are row-scoped and a header binds its section`, () => {
    expect(coreCatalog.components.List.slotScope).toBe(`row`)
    const header: UiNode = { id: `h`, component: `Text`, props: { text: { call: `concat`, args: { values: [{ path: `value` }, ` · `, { path: `count` }, ` · `, { path: `/title` }] } } }, children: [] }
    expect(bindSectionHeader(header, { value: `Today`, count: 2 }, 0, { title: `Inbox` })?.props.text).toBe(`Today · 2 · Inbox`)
    const list: UiNode = { id: `l`, component: `List`, props: {}, children: [], slots: { section: header } }
    expect(bindTree(list, {})?.slots?.section).toBe(header)
  })
})

describe(`animations.json (§2)`, () => {
  test(`timing and frames per built-in theme`, () => {
    for (const [id, entries] of Object.entries(animationsFixture.themes as Record<string, Record<string, { timing: unknown; frames: { t: number; frame: Record<string, number | null> }[]; reduced: unknown }>>)) {
      const theme = builtinTheme(id)
      expect(Object.keys(entries)).toEqual([...ANIMATION_NAMES])
      for (const [name, e] of Object.entries(entries)) {
        expect(animationTiming(name, theme), `${id}/${name}`).toEqual(e.timing as never)
        for (const f of e.frames) {
          const got = animationFrame(name, f.t, theme)!
          for (const [k, v] of Object.entries(f.frame)) expect(v === null ? got[k as keyof typeof got] === null : close(got[k as keyof typeof got] as number, v, 1e-3), `${id}/${name}@${f.t}.${k}`).toBe(true)
        }
        expect(animationFrame(name, 5, theme, { reducedMotion: true }), `${id}/${name} reduced`).toEqual(e.reduced as never)
      }
    }
    for (const name of ANIMATION_NAMES) expect(keyframesCss(name)).toBe((animationsFixture.css as Record<string, string>)[name])
    expect(ANIMATION_PROPERTIES_CSS).toBe(animationsFixture.properties)
    const neutral = builtinTheme(`neutral`)
    for (const c of animationsFixture.opacity) {
      expect(close(paintedOpacity(c.own, animationFrame(c.animation, c.t, neutral)), c.expected, 1e-3), c.name).toBe(true)
      expect(styleToCss({ opacity: c.own, animation: c.animation }, {}, neutral), c.name).toEqual(c.css)
    }
  })

  test(`style keys: sticky, backdropBlur (token only), animation (+ duration token)`, () => {
    expect(validateStyle({ position: `sticky`, top: 0, backdropBlur: `$blur.md`, animation: `pulse`, animationDuration: `$motion.slow` })).toEqual([])
    expect(validateStyle({ backdropBlur: 8 }).length).toBe(1)
    expect(validateStyle({ animation: `wobble` }).length).toBe(1)
    expect(validateStyle({ animationDuration: 300 }).length).toBe(1)
    const neutral = builtinTheme(`neutral`)
    expect(styleToCss({ backdropBlur: 12, position: `sticky`, top: 0 })).toEqual({ "backdrop-filter": `blur(12px)`, "-webkit-backdrop-filter": `blur(12px)`, position: `sticky`, top: `0px` })
    expect(styleToCss({ animation: `spin` }, {}, neutral)).toEqual({ animation: `xui-spin 1120ms linear infinite both` })
    expect(styleToCss({ animation: `fade-in`, animationDuration: 120 }, {}, neutral).animation).toBe(`xui-fade-in 120ms cubic-bezier(0, 0, 0.2, 1) 1 both`)
    expect(styleToCss({ animation: `spin` })).toEqual({})
  })
})

describe(`theme, strings, a11y, catalog, prompt (round 2)`, () => {
  test(`every built-in theme resolves the blur group; recipes may blur and animate`, () => {
    for (const id of [`neutral`, `exponential`, `playful`]) expect(Object.keys(builtinTheme(id).tokens.blur)).toEqual([`sm`, `md`, `lg`, `xl`])
    const bad = validateTheme({ id: `x`, name: `X`, extends: `neutral`, recipes: { Box: { root: [{ style: { backdropBlur: 4, animation: `wobble` } }] } } }, { themes: [builtinTheme(`neutral`)] })
    expect(bad.map((i) => i.path).sort()).toEqual([`recipes.Box.root[0].style.animation`, `recipes.Box.root[0].style.backdropBlur`])
  })

  test(`the hard-coded labels have string ids`, () => {
    for (const id of [`invalidValue`, `message`, `codeBlock`, `dialog`, `table`, `carousel`, `slide`, `resize`]) expect(DEFAULT_STRINGS[id], id).toBeDefined()
  })

  test(`Resizable, sections, scrollToIndex, the Switch order`, () => {
    const r = coreCatalog.components.Resizable
    expect(r.kind).toBe(`native`)
    expect(r.props.sizes.bindable).toBe(true)
    expect(r.props.direction.responsive).toBe(true)
    expect(COMPONENT_A11Y.Resizable.role).toBe(`group`)
    expect(Object.keys(A11Y_COMMANDS)).toContain(`scrollToIndex`)
    expect(COMPONENT_A11Y.Switch.notes).toContain(`label`)
    expect(coreCatalog.components.Video.props.aspectRatio.default).toBe(LAYOUT_CONSTANTS.mediaAspectRatio)
    expect(coreCatalog.components.List.props.sectionBy.type).toBe(`string`)
  })

  test(`the prompt teaches the new style keys`, () => {
    const full = catalogPrompt()
    for (const needle of [`sticky`, `backdropBlur $blur.*`, `animation (pulse|spin|fade-in`, `\nResizable: `, `sectionBy: string`, `formatRelativeTime`]) expect(full, needle).toContain(needle)
  })
})

describe(`conformance (round 2)`, () => {
  test(`the manifest counts the round-1 and round-2 fixtures`, () => {
    expect(manifest.version).toBe(2)
    for (const id of [`bind`, `style-conditions`, `code-tokens`, `format`, `template-items`, `text-direction`, `resizable`, `virtual-list`, `animations`]) expect(manifest.suites.some((s) => s.id === id), id).toBe(true)
  })

  test(`every known gpui-vs-web divergence cause carries a decision`, () => {
    const causes = (knownFixture as unknown as { causes?: { id: string; right: string; fix: string[]; reason?: string }[] }).causes ?? []
    expect(causes.length).toBeGreaterThanOrEqual(53)
    for (const c of causes) {
      expect([`web`, `gpui`, `neither`, `contract`], c.id).toContain(c.right)
      expect(c.fix.length, c.id).toBeGreaterThan(0)
    }
    for (const c of causes.filter((x) => x.fix.includes(`survivor`))) expect(c.reason, c.id).toBeTruthy()
  })
})
