// Round 1 (docs/round-1-contract.md): the renderer-hardening contract as
// tests — bound macro inputs, two-way `set`, responsive props, the bind pass,
// built-in strings, locale, the CodeBlock tokenizer, the a11y table, the
// theme's density / contrast / mode / breakpoints, and the reducer additions.

import { describe, expect, test } from "bun:test"
import macrosFixture from "../fixtures/catalog-macros.json" with { type: "json" }
import bindFixture from "../fixtures/bind-time.json" with { type: "json" }
import codeFixture from "../fixtures/code-tokens.json" with { type: "json" }
import macrosJson from "../catalog/macros.json" with { type: "json" }
import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import { COMPONENT_A11Y, A11Y_COMMANDS } from "./a11y"
import { CODE_LANGUAGE_NAMES, CODE_LANGUAGES, tokenizeCode } from "./code"
import { chartExtent, niceTicks, seriesColor } from "./chart"
import { styleToCss, gradientCss } from "./css"
import { absolutePath, bindTree, readPointer, resolveDynamic, runAction, writePointer } from "./dynamic"
import { parseLocale, textDirection, weekStart } from "./locale"
import { expandMacros, propsAt, responsiveAt } from "./macros"
import { reduceNested, reduceSurface } from "./reducer"
import { coreSchema } from "./schema"
import { DEFAULT_STRINGS, STRING_IDS, formatString, isStringRef, resolveString, stringTable } from "./strings"
import { applyContrast, applyDensity, loadTheme, resolveConditionKey, resolveMode, resolveRecipe, resolveStyleValues, resolveToken, validateTheme } from "./theme"
import { BUILTIN_THEMES, builtinTheme } from "./themes"
import { validateProps } from "./validate"
import type { Action, FlatComponent, NestedNode, UiNode } from "./types"

const canon = (v: unknown) => JSON.stringify(v)
const expand = (node: NestedNode) => {
  const { root, issues } = reduceNested(node, { catalogId: CORE_CATALOG_ID })
  expect(issues).toEqual([])
  return root
}
const byId = (root: UiNode, id: string): UiNode | undefined => {
  if (root.id === id) return root
  for (const c of [...root.children, ...Object.values(root.slots ?? {})]) {
    const r = byId(c, id)
    if (r) return r
  }
}

describe(`bound macro inputs (§1)`, () => {
  const cases = macrosFixture.cases as { name: string; input: NestedNode; expected: UiNode }[]

  test(`catalog-macros.json carries a bound case per bindable macro prop and a responsive case per responsive prop`, () => {
    for (const [name, def] of Object.entries(coreCatalog.components)) {
      if (def.kind !== `macro`) continue
      for (const [prop, schema] of Object.entries(def.props)) {
        if (schema.bindable) expect(cases.some((c) => c.name === `${name}/bound:${prop}`), `${name}/bound:${prop}`).toBe(true)
        if (schema.responsive) expect(cases.some((c) => c.name === `${name}/responsive:${prop}`), `${name}/responsive:${prop}`).toBe(true)
      }
    }
    expect(canon(cases)).toContain(`"path":"/value"`)
  })

  test(`the audit's bugs: Progress width, Pagination label, Collapsible body, Chip label`, () => {
    const progress = expand({ id: `p`, component: `Progress`, props: { value: { path: `/upload` } } })
    expect(byId(progress, `p.track.fill`)!.style!.width).toEqual({ call: `percent`, args: { value: { path: `/upload` }, max: 100 } })
    const pager = expand({ id: `g`, component: `Pagination`, props: { totalPages: 5, page: { path: `/page` } } })
    expect(byId(pager, `g.label`)!.props.text).toEqual({ call: `concat`, args: { values: [{ call: `fallback`, args: { value: { path: `/page` }, default: 1 } }, ` / `, 5] } })
    const collapsible = expand({ id: `c`, component: `Collapsible`, props: { title: `More`, open: { path: `/open` } }, children: [{ id: `c-x`, component: `Text`, props: { text: `x` } }] })
    expect(byId(collapsible, `c.body`)!.visible).toEqual({ path: `/open` })
    const pill = expand({ id: `pl`, component: `Chip`, props: { label: { path: `/name` } } })
    expect(byId(pill, `pl.label`)!.props.text).toEqual({ path: `/name` })
  })

  test(`resolved against data, the bound expansions show the right values`, () => {
    const progress = expand({ id: `p`, component: `Progress`, props: { value: { path: `/upload` }, max: 200 } })
    expect(bindTree(progress, { upload: 50 })!.children[0].children[0].style!.width).toBe(`25%`)
    const pager = expand({ id: `g`, component: `Pagination`, props: { totalPages: 5, page: { path: `/page` } } })
    const bound = bindTree(pager, { page: 1 })!
    expect(bound.children[1].props.text).toBe(`1 / 5`)
    expect(bound.children[0].props.disabled).toBe(true)
    expect(bindTree(pager, {})!.children[1].props.text).toBe(`1 / 5`)
    const collapsible = expand({ id: `c`, component: `Collapsible`, props: { title: `More`, open: { path: `/open` } }, children: [{ id: `c-x`, component: `Text`, props: { text: `x` } }] })
    expect(bindTree(collapsible, { open: false })!.children.map((n) => n.id)).toEqual([`c.trigger`])
    expect(bindTree(collapsible, { open: true })!.children.map((n) => n.id)).toEqual([`c.trigger`, `c.body`])
  })

  test(`$set: a bound prop gets the write-back, the author's event keeps its routed context`, () => {
    const withHandler = expand({ id: `c`, component: `Collapsible`, props: { title: `More`, open: { path: `/open` } }, on: { change: { event: { name: `toggled` } } } })
    const press = byId(withHandler, `c.trigger`)!.on!.press as Action
    expect(Object.keys(press)).toEqual([`event`, `function`])
    expect(press.function).toEqual({ call: `set`, args: { path: `/open`, value: { call: `not`, args: { value: { path: `/open` } } } } })
    const outcome = runAction(press, { open: false })
    expect(outcome.data).toEqual({ open: true })
    expect(outcome.event).toEqual({ name: `toggled`, context: { open: true } })
    const literal = expand({ id: `c`, component: `Collapsible`, props: { title: `More`, open: false }, on: { change: { event: { name: `toggled` } } } })
    expect(byId(literal, `c.trigger`)!.on!.press).toEqual({ event: { name: `toggled`, context: { open: true } } })
    // Round 3: a collapsible Section's header writes `open` back like Collapsible's trigger.
    const section = expand({ id: `s`, component: `Section`, props: { title: `More`, collapsible: true, open: { path: `/open` } } })
    expect(runAction(byId(section, `s.header`)!.on!.press, { open: true }).data).toEqual({ open: false })
    const alert = expand({ id: `a`, component: `AlertDialog`, props: { title: `Sure?`, open: { path: `/ask` } } })
    expect(runAction(alert.slots!.footer.children[1].on!.press, { ask: true }).data).toEqual({ ask: false })
  })

  test(`$each over a bound array emits nothing; a bound $any emits the condition (an or of several)`, () => {
    // expandMacros directly: these props are not bindable in the catalog, the
    // expander's rules hold for any input (extensions may bind them).
    const bc = expandMacros({ id: `b`, component: `Breadcrumb`, props: { items: { path: `/items` } }, children: [] })
    expect(bc.children).toEqual([])
    const one = expandMacros({ id: `m`, component: `Meter`, props: { segments: [], label: { path: `/l` } }, children: [] })
    expect(one.children[0].visible).toEqual({ path: `/l` })
    const both = expandMacros({ id: `m`, component: `Meter`, props: { segments: [], label: { path: `/l` }, value: { path: `/v` } }, children: [] })
    expect(both.children[0].visible).toEqual({ call: `or`, args: { values: [{ path: `/l` }, { path: `/v` }] } })
    const literal = expandMacros({ id: `m`, component: `Meter`, props: { segments: [], label: `Disk`, value: { path: `/v` } }, children: [] })
    expect(literal.children[0].visible).toBeUndefined()
  })

  test(`every macros.json $set names a prop of its macro`, () => {
    const walk = (tpl: Record<string, unknown>, macro: string) => {
      const set = tpl.$set as Record<string, { prop: string }> | undefined
      for (const spec of Object.values(set ?? {})) expect(coreCatalog.components[macro].props[spec.prop], `${macro}.$set ${spec.prop}`).toBeDefined()
      for (const c of (tpl.children as unknown[]) ?? []) if (typeof c === `object`) walk(c as Record<string, unknown>, macro)
      for (const c of Object.values((tpl.slots as Record<string, unknown>) ?? {})) if (typeof c === `object`) walk(c as Record<string, unknown>, macro)
    }
    for (const [macro, def] of Object.entries(macrosJson.macros)) walk(def.root as unknown as Record<string, unknown>, macro)
  })
})

describe(`responsive props (§2)`, () => {
  test(`responsiveAt inherits the last smaller key; propsAt flattens`, () => {
    const v = { base: 1, md: 2, xl: 4 }
    expect(responsiveAt(v, null)).toBe(1)
    expect(responsiveAt(v, `sm`)).toBe(1)
    expect(responsiveAt(v, `lg`)).toBe(2)
    expect(responsiveAt(v, `xl`)).toBe(4)
    expect(propsAt({ a: v, b: `x` }, `md`)).toEqual({ a: 2, b: `x` })
  })

  test(`Stack direction and Grid columns become media blocks on the root`, () => {
    const stack = expand({ id: `s`, component: `Stack`, props: { direction: { base: `vertical`, md: `horizontal` }, gap: { base: `sm`, lg: `lg` } } })
    expect(stack.style).toEqual({
      display: `flex`, flexDirection: `column`, flexWrap: `nowrap`, gap: `$spacing.sm`, alignItems: `stretch`, justifyContent: `start`,
      "@media (min-width: $breakpoint.md)": { flexDirection: `row` },
      "@media (min-width: $breakpoint.lg)": { gap: `$spacing.lg` },
    })
    const grid = expand({ id: `g`, component: `Grid`, props: { columns: { base: 1, sm: 2, lg: 3 } } })
    expect(grid.style).toEqual({
      display: `grid`, gridTemplateColumns: `repeat(1, minmax(0, 1fr))`, gap: `$spacing.md`,
      "@media (min-width: $breakpoint.sm)": { gridTemplateColumns: `repeat(2, minmax(0, 1fr))` },
      "@media (min-width: $breakpoint.lg)": { gridTemplateColumns: `repeat(3, minmax(0, 1fr))` },
    })
  })

  test(`a responsive value validates per breakpoint and refuses unknown keys`, () => {
    const stack = coreCatalog.components.Stack
    expect(validateProps(stack, { direction: { base: `vertical`, md: `horizontal` } })).toEqual([])
    expect(validateProps(stack, { direction: { base: `vertical`, md: `diagonal` } })[0].path).toBe(`props.direction.md`)
    expect(validateProps(stack, { direction: { base: `vertical`, tablet: `horizontal` } })[0].message).toContain(`responsive value`)
    expect(validateProps(coreCatalog.components.Drawer, { side: { base: `bottom`, md: `right` } })).toEqual([])
  })

  test(`Sidebar hides its column below a breakpoint, never when told so, and when collapsed is bound`, () => {
    const md = expand({ id: `sb`, component: `Sidebar`, props: { collapseBelow: `md` } })
    expect(md.children[0].style!["@media (max-width: $breakpoint.md)"]).toEqual({ display: `none` })
    const never = expand({ id: `sb`, component: `Sidebar`, props: { collapseBelow: `never` } })
    expect(Object.keys(never.children[0].style!).some((k) => k.startsWith(`@media`))).toBe(false)
    const bound = expand({ id: `sb`, component: `Sidebar`, props: { collapsed: { path: `/c` } } })
    expect(bound.children[0].visible).toEqual({ call: `not`, args: { value: { path: `/c` } } })
  })
})

describe(`the bind pass (§1)`, () => {
  test(`pointers: absolute, relative to a scope, escaped; writes copy`, () => {
    const data = { a: { "b/c": [1, { d: 2 }] } }
    expect(readPointer(data, `/a/b~1c/1/d`)).toBe(2)
    expect(absolutePath(`d`, { base: `/a/b~1c/1` })).toBe(`/a/b~1c/1/d`)
    expect(resolveDynamic({ path: `d` }, data, { scope: { base: `/a/b~1c/1` } })).toBe(2)
    const written = writePointer(data, `/a/x/y`, 3) as Record<string, Record<string, unknown>>
    expect(written.a.x).toEqual({ y: 3 })
    expect((data.a as Record<string, unknown>).x).toBeUndefined()
  })

  test(`values resolve at any depth; unknown functions are undefined`, () => {
    expect(resolveDynamic({ series: [{ values: { path: `/v` } }], n: { call: `len`, args: { value: { path: `/v` } } } }, { v: [1, 2] })).toEqual({ series: [{ values: [1, 2] }], n: 2 })
    expect(resolveDynamic({ call: `nope` }, {})).toBeUndefined()
    expect(resolveDynamic({ call: `and`, args: { values: [true, { path: `/x` }] } }, { x: 0 })).toBe(true)
  })

  test(`fixtures/bind-time.json replays: bound trees and presses`, () => {
    const cases = bindFixture.cases as unknown as { name: string; expanded: UiNode; datasets: { data: unknown; bound: UiNode | null; presses: { id: string; outcome: unknown }[] }[] }[]
    expect(cases.length).toBeGreaterThan(20)
    for (const c of cases) {
      for (const d of c.datasets) {
        expect(canon(bindTree(c.expanded, d.data, { strings: DEFAULT_STRINGS })), c.name).toBe(canon(d.bound))
        for (const p of d.presses) expect(canon(runAction(byId(c.expanded, p.id)!.on!.press, d.data, { strings: DEFAULT_STRINGS })), `${c.name} ${p.id}`).toBe(canon(p.outcome))
      }
      // Three datasets: a sample, its falsy twin and the path MISSING.
      expect(c.datasets.length, c.name).toBe(3)
      expect(c.datasets[2].data, c.name).toEqual({})
    }
  })
})

describe(`built-in strings (§4)`, () => {
  test(`every $string reference in the catalog and the macros names a known id`, () => {
    const refs = (JSON.stringify(coreCatalog) + JSON.stringify(macrosJson)).match(/\$string\.[a-zA-Z0-9]+/g) ?? []
    expect(refs.length).toBeGreaterThan(5)
    for (const ref of refs) expect(isStringRef(ref), ref).toBe(true)
  })

  test(`overrides, formatting, resolution, validation`, () => {
    expect(STRING_IDS).toContain(`search`)
    expect(stringTable({ search: `Suchen…` }).search).toBe(`Suchen…`)
    expect(formatString(DEFAULT_STRINGS.pageOf, { page: 2, total: 5 })).toBe(`Page 2 of 5`)
    expect(formatString(`{count} of {total}`, { count: 1 })).toBe(`1 of {total}`)
    expect(resolveString(`$string.cancel`, stringTable({ cancel: `Abbrechen` }))).toBe(`Abbrechen`)
    expect(resolveString(`plain`)).toBe(`plain`)
    expect(validateProps(coreCatalog.components.AlertDialog, { title: `x`, confirmLabel: `$string.nope` })[0].message).toContain(`unknown built-in string`)
    expect(validateProps(coreCatalog.components.AlertDialog, { title: `x`, confirmLabel: `$string.confirm` })).toEqual([])
  })
})

describe(`locale (§4)`, () => {
  test(`week start by region, likely regions, CLDR's exceptions`, () => {
    expect(weekStart(`en-US`)).toBe(0)
    expect(weekStart(`en-GB`)).toBe(1)
    expect(weekStart(`de-AT`)).toBe(1)
    expect(weekStart(`en`)).toBe(0)
    expect(weekStart(`de`)).toBe(1)
    expect(weekStart(`pt`)).toBe(0)
    expect(weekStart(`pt-PT`)).toBe(0)
    expect(weekStart(`ar-EG`)).toBe(6)
    expect(weekStart(`fa`)).toBe(6)
    expect(weekStart(`dv-MV`)).toBe(5)
    expect(weekStart(`zh-Hant-TW`)).toBe(0)
    expect(weekStart(`xx`)).toBe(1)
  })

  test(`parsing and direction`, () => {
    expect(parseLocale(`zh_Hant_TW`)).toEqual({ language: `zh`, region: `TW` })
    expect(parseLocale(`es-419`)).toEqual({ language: `es`, region: `419` })
    expect(textDirection(`ar-EG`)).toBe(`rtl`)
    expect(textDirection(`he`)).toBe(`rtl`)
    expect(textDirection(`de-DE`)).toBe(`ltr`)
  })
})

describe(`CodeBlock tokenizer (§3)`, () => {
  test(`the languages are the codeLanguage enum; every kind is a codeToken`, () => {
    expect([...CODE_LANGUAGE_NAMES]).toEqual([...coreCatalog.enums.codeLanguage])
    for (const name of CODE_LANGUAGE_NAMES) expect(CODE_LANGUAGES[name]).toBeDefined()
  })

  test(`fixtures/code-tokens.json replays; lines concatenate to the source`, () => {
    const cases = codeFixture.cases as { name: string; language: string; code: string; expected: { kind: string; text: string }[][] }[]
    for (const c of cases) {
      const tokens = tokenizeCode(c.code, c.language)
      expect(tokens, c.name).toEqual(c.expected)
      expect(tokens.map((line) => line.map((t) => t.text).join(``)), c.name).toEqual(c.code.replace(/\r\n?/g, `\n`).split(`\n`))
      for (const line of tokens) for (const t of line) expect(coreCatalog.enums.codeToken, `${c.name}: ${t.kind}`).toContain(t.kind)
    }
  })

  test(`a few classifications`, () => {
    const [line] = tokenizeCode(`const x = foo.bar(1) // c`, `ts`)
    expect(line.map((t) => t.kind)).toEqual([`keyword`, `plain`, `operator`, `plain`, `punctuation`, `function`, `punctuation`, `number`, `punctuation`, `plain`, `comment`])
    expect(tokenizeCode(`{"a": 1}`, `json`)[0][1]).toEqual({ kind: `property`, text: `"a"` })
    expect(tokenizeCode(``, `ts`)).toEqual([[]])
  })
})

describe(`accessibility (§6)`, () => {
  test(`every component has a role and keys; interactive ones name their keys; entries name real components`, () => {
    for (const [name, def] of Object.entries(coreCatalog.components)) {
      expect(COMPONENT_A11Y[name], name).toBeDefined()
      if ((def.events?.length ?? 0) > 0 && name !== `Chart`) expect(COMPONENT_A11Y[name].keys.length, `${name} keys`).toBeGreaterThan(0)
    }
    for (const name of Object.keys(COMPONENT_A11Y)) expect(coreCatalog.components[name], name).toBeDefined()
    for (const entry of Object.values(COMPONENT_A11Y)) expect(entry.role.length).toBeGreaterThan(2)
    expect(Object.keys(A11Y_COMMANDS)).toEqual([`focus`, `announce`, `scrollIntoView`, `scrollToIndex`])
    for (const name of [`Tabs`, `Radio`, `Segmented`, `Select`, `Menu`, `Accordion`, `DatePicker`]) expect(COMPONENT_A11Y[name].keys.join(` `), name).toMatch(/Arrow/)
    expect(COMPONENT_A11Y.Tooltip.keys.join(` `)).toContain(`focus`)
  })
})

describe(`theming (§5)`, () => {
  const neutral = builtinTheme(`neutral`)

  test(`the built-ins carry breakpoints, easings, densities, contrast overlays, the elevation top`, () => {
    for (const theme of BUILTIN_THEMES.map((t) => builtinTheme(t.id))) {
      expect(Object.keys(theme.tokens.breakpoint)).toEqual([`sm`, `md`, `lg`, `xl`])
      expect(Object.keys(theme.tokens.ease).sort()).toEqual([`accelerate`, `decelerate`, `standard`])
      expect(theme.tokens.density.compact).toBeLessThan(1)
      expect(theme.tokens.density.comfortable).toBeGreaterThan(1)
      expect(theme.modes.light.shadow.xl.length).toBeGreaterThan(0)
      expect(theme.modes.dark.color.chart8).toMatch(/^#[0-9a-f]{6}/)
    }
    expect(builtinTheme(`exponential`).tokens.ease.decelerate).toEqual([0, 0, 0.2, 1])
    expect(resolveToken(neutral, `$ease.standard`, `light`)).toEqual([0.2, 0, 0, 1])
    expect(resolveToken(neutral, `$breakpoint.md`, `light`)).toBe(768)
  })

  test(`mode, density and contrast derive the theme a surface runs with`, () => {
    expect(resolveMode(`system`, true)).toBe(`dark`)
    expect(resolveMode(`system`, false)).toBe(`light`)
    expect(resolveMode(`light`, true)).toBe(`light`)
    const compact = applyDensity(neutral, `compact`)
    expect(compact.tokens.control.buttonMd).toBe(Math.round(neutral.tokens.control.buttonMd * neutral.tokens.density.compact))
    expect(compact.tokens.spacing.lg).toBe(Math.round(neutral.tokens.spacing.lg * neutral.tokens.density.compact))
    expect(compact.tokens.radius).toEqual(neutral.tokens.radius)
    expect(applyDensity(neutral, `default`)).toBe(neutral)
    const high = applyContrast(neutral)
    expect(high.modes.light.color.foreground).toBe(`#000000`)
    expect(high.modes.light.color.primary).toBe(neutral.modes.light.color.primary)
    expect(neutral.modes.light.color.foreground).not.toBe(`#000000`)
  })

  test(`media keys resolve their breakpoint tokens with the style values`, () => {
    expect(resolveConditionKey(neutral, `@media (min-width: $breakpoint.lg)`)).toBe(`@media (min-width: 1024px)`)
    expect(resolveConditionKey(neutral, `:hover`)).toBe(`:hover`)
    const style: Record<string, unknown> = { gap: `$spacing.md`, "@media (max-width: $breakpoint.sm)": { gap: `$spacing.xs` }, backgroundGradient: { angle: 90, stops: [{ color: `$color.primary`, offset: 0 }, { color: `#ffffff`, offset: 1 }] } }
    expect(resolveStyleValues(neutral, style, `light`)).toEqual({
      gap: 12,
      "@media (max-width: 640px)": { gap: 4 },
      backgroundGradient: { angle: 90, stops: [{ color: `#171717`, offset: 0 }, { color: `#ffffff`, offset: 1 }] },
    })
  })

  test(`recipes animate: transition tokens, transforms, the new states`, () => {
    const chevron = resolveRecipe(neutral, { component: `Collapsible`, part: `chevron`, props: { open: true } }, `light`)
    expect(chevron.transform).toBe(`rotate(90deg)`)
    expect(chevron.transition).toBe(neutral.tokens.motion.standard)
    expect(chevron.transitionEasing).toEqual([0.2, 0, 0, 1])
    const dz = resolveRecipe(neutral, { component: `FileUpload`, part: `dropzone`, props: {}, states: [`dragover`] }, `light`)
    expect(dz.borderStyle).toBe(`dashed`)
    expect(dz.borderColor).toBe(neutral.modes.light.color.ring)
    const field = resolveRecipe(neutral, { component: `Input`, part: `field`, props: {}, states: [`invalid`] }, `light`)
    expect(field.borderColor).toBe(neutral.modes.light.color.destructive)
  })

  test(`validation of the new groups and keys`, () => {
    const bad = { id: `bad`, name: `Bad`, extends: `neutral`, tokens: { ease: { standard: [0, 1] }, density: { compact: 9 } }, contrast: { light: { color: { foreground: `black` } } }, recipes: { Button: { root: [{ style: { transition: 120, transform: `skew(4deg)`, borderStyle: `wavy` } }] } } }
    const issues = validateTheme(bad, { themes: BUILTIN_THEMES }).map((i) => i.path)
    for (const path of [`tokens.ease.standard`, `tokens.density.compact`, `contrast.light.color.foreground`, `recipes.Button.root[0].style.transition`, `recipes.Button.root[0].style.transform`, `recipes.Button.root[0].style.borderStyle`]) expect(issues, path).toContain(path)
    const ok = loadTheme({ id: `ok`, name: `Ok`, extends: `neutral`, tokens: { breakpoint: { md: 700 } } }, { themes: BUILTIN_THEMES })
    expect(ok.tokens.breakpoint).toEqual({ sm: 640, md: 700, lg: 1024, xl: 1280 })
  })
})

describe(`reducer and schema additions`, () => {
  test(`visible, template keys and accessibility on the flat form; slots admitted by name or *`, () => {
    const flat: FlatComponent[] = [
      { id: `root`, component: `Stack`, children: [`list`, `t`, `x`] },
      { id: `list`, component: `List`, children: { componentId: `row`, path: `/rows`, key: `id` }, visible: { path: `/show` } },
      { id: `row`, component: `Row`, title: { path: `name` } },
      { id: `t`, component: `Table`, columns: [{ key: `s`, label: `S`, type: `slot`, slot: `status` }], rows: [], slots: { status: `cell` }, accessibility: { label: `Members` } },
      { id: `cell`, component: `Badge`, text: { path: `s` } },
      { id: `x`, component: `Text`, text: `x`, visible: `yes` as unknown as boolean, slots: { nope: `cell` } },
    ]
    const { root, issues } = reduceSurface(flat, { catalogId: CORE_CATALOG_ID })
    const list = root.children[0]
    expect(list.template).toEqual({ component: `row`, path: `/rows`, key: `id` })
    expect(list.visible).toEqual({ path: `/show` })
    expect(root.children[1].accessibility).toEqual({ label: `Members` })
    expect(root.children[1].slots!.status.component).toBe(`Box`)
    expect(issues.map((i) => `${i.id}: ${i.message}`)).toEqual([`x: visible: expected a boolean, a binding or a function call`, `x: slots.nope: Text has no such slot`])
  })

  test(`the core schema knows visible, responsive values, any-slot tables, both-action and the core functions`, () => {
    const schema = coreSchema([`ui-check`]) as Record<string, Record<string, Record<string, unknown>>>
    expect(schema.$defs.Visible).toBeDefined()
    expect(Object.keys(schema.functions)).toContain(`percent`)
    expect(Object.keys(schema.functions)).toContain(`set`)
    const stack = JSON.stringify(schema.components.Stack)
    expect(stack).toContain(`"required":["base"]`)
    expect(JSON.stringify(schema.components.Table)).toContain(`"additionalProperties":{"$ref":"#/$defs/ComponentId"}`)
    expect(JSON.stringify(schema.$defs.Action)).toContain(`anyOf`)
    expect(JSON.stringify(schema.$defs.Style)).toContain(`:focus-visible`)
  })

  test(`css: gradients, transitions with easing, per-side borders get a style`, () => {
    expect(gradientCss({ angle: 90, stops: [{ color: `#000000`, offset: 0 }, { color: `#ffffff`, offset: 0.5 }] })).toBe(`linear-gradient(90deg, #000000 0%, #ffffff 50%)`)
    expect(styleToCss({ transition: 120, transitionEasing: [0.2, 0, 0, 1] as unknown as string, borderTopWidth: 1, transform: `rotate(90deg)` })).toEqual({
      transition: `all 120ms cubic-bezier(0.2, 0, 0, 1)`, "border-top-width": `1px`, transform: `rotate(90deg)`, "border-style": `solid`, "border-color": `transparent`,
    })
    expect(styleToCss({ "@media (min-width: 1px)": { gap: 1 }, ":hover": { gap: 2 }, gap: 3 } as Record<string, unknown>)).toEqual({ gap: `3px` })
  })

  test(`expansion keeps a node's visible and accessibility on the macro root`, () => {
    const root = expandMacros({ id: `b`, component: `Badge`, props: { text: `x` }, children: [], visible: { path: `/v` }, accessibility: { label: `Count` } })
    expect(root.visible).toEqual({ path: `/v` })
    expect(root.accessibility).toEqual({ label: `Count` })
  })
})

describe(`chart numbers (§3)`, () => {
  test(`nice ticks`, () => {
    expect(niceTicks(0, 5)).toEqual({ min: 0, max: 5, step: 1, ticks: [0, 1, 2, 3, 4, 5] })
    expect(niceTicks(0, 97)).toEqual({ min: 0, max: 100, step: 20, ticks: [0, 20, 40, 60, 80, 100] })
    expect(niceTicks(0, 0.7).ticks).toEqual([0, 0.2, 0.4, 0.6, 0.8])
    expect(niceTicks(-3, 12).ticks).toEqual([-5, 0, 5, 10, 15])
    expect(niceTicks(4, 4).ticks).toEqual([4, 4.2, 4.4, 4.6, 4.8, 5])
  })

  test(`extents per kind and the colour order`, () => {
    const series = [{ name: `a`, values: [3, 5, 2] }, { name: `b`, values: [1, 2, 2] }]
    expect(chartExtent(`bar`, series)).toEqual({ min: 0, max: 5 })
    expect(chartExtent(`stackedBar`, series)).toEqual({ min: 0, max: 7 })
    expect(chartExtent(`sparkline`, series)).toEqual({ min: 1, max: 5 })
    expect(chartExtent(`line`, series, undefined, 10)).toEqual({ min: 0, max: 10 })
    expect(chartExtent(`bar`, [])).toEqual({ min: 0, max: 1 })
    expect(seriesColor(0)).toBe(`$color.chart1`)
    expect(seriesColor(9)).toBe(`$color.chart2`)
    expect(seriesColor(0, `danger`)).toBe(`$color.destructive`)
  })
})
