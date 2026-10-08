// Round 1 review fixes (docs/round-1-contract.md §1, §3, §4, §6): row-scoped
// Table slot cells, DATA props, bound accessible names, the macro parts'
// accessibility channel ($a11y), the `$set` collision, numeric equality and
// clamping, the bind function table, RTL glyph mirroring, CLDR likely
// regions and language aliases, chart extents over large series.

import { describe, expect, test } from "bun:test"
import macrosFixture from "../fixtures/catalog-macros.json" with { type: "json" }
import bindFixture from "../fixtures/bind-time.json" with { type: "json" }
import kitchenExpanded from "../fixtures/kitchen-sink.expanded.json" with { type: "json" }
import macrosJson from "../catalog/macros.json" with { type: "json" }
import localeJson from "../catalog/locale.json" with { type: "json" }
import { CORE_CATALOG_ID, coreCatalog } from "./catalog"
import { A11Y_ROLES, COMPONENT_A11Y, isA11yRole } from "./a11y"
import { chartExtent, chartSummaryParams, sparklineSummaryParams } from "./chart"
import { BIND_FUNCTIONS, BIND_FUNCTION_NAMES, bindRowSlot, bindTree, hasRowSlots, isDataSchema, readPath, resolveDynamic, rowScope, runAction } from "./dynamic"
import { CORE_FUNCTIONS, clampNumber, evalValue, valuesEqual } from "./expr"
import { RTL_MIRRORED_ICONS, mirrorsInRtl, parseLocale, textDirection, weekStart } from "./locale"
import { reduceNested, reduceSurface } from "./reducer"
import { DEFAULT_STRINGS, formatString } from "./strings"
import { A2UI_BASIC_CATALOG_ID } from "./catalog"
import { ICON_NAMES } from "./catalog.generated"
import type { NestedNode, UiNode } from "./types"

const canon = (v: unknown) => JSON.stringify(v)
const STRINGS = { strings: DEFAULT_STRINGS }
const reduce = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID })
const expand = (node: NestedNode) => {
  const { root, issues } = reduce(node)
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
const walk = (value: unknown, visit: (v: Record<string, unknown>) => void) => {
  if (Array.isArray(value)) value.forEach((v) => walk(v, visit))
  else if (typeof value === `object` && value !== null) {
    visit(value as Record<string, unknown>)
    Object.values(value).forEach((v) => walk(v, visit))
  }
}

describe(`row-scoped Table slot cells (§3, review blocker)`, () => {
  const table: NestedNode = {
    id: `t`,
    component: `Table`,
    props: { columns: [{ key: `s`, label: `S`, type: `slot`, slot: `status` }], rows: { path: `/rows` } },
    slots: { status: { id: `cell`, component: `Badge`, props: { text: { path: `s` } }, visible: { path: `show` } } },
  }

  test(`the bind pass leaves the slot unbound; each row binds it with the row as its scope`, () => {
    const root = expand(table)
    expect(hasRowSlots(coreCatalog.components.Table)).toBe(true)
    const data = { rows: [{ s: `ok`, show: true }, { s: `late`, show: false }] }
    const bound = bindTree(root, data, STRINGS)!
    // The reviewer's repro: the slot survives, its relative bindings intact.
    expect(bound.slots?.status).toEqual(root.slots!.status)
    const rows = bound.props.rows as unknown[]
    const row0 = bindRowSlot(bound.slots!.status, root.props.rows, rows, 0, data, STRINGS)!
    expect(byId(row0, `cell.label`)!.props.text).toBe(`ok`)
    expect(bindRowSlot(bound.slots!.status, root.props.rows, rows, 1, data, STRINGS)).toBeNull()
    expect(rowScope(root.props.rows, rows, 1)).toEqual({ base: `/rows/1` })
  })

  test(`literal rows bind relative paths inside the row; absolute ones read the data; a relative set writes nothing`, () => {
    const scope = rowScope([{ s: `x` }], [{ s: `x` }], 0)
    expect(scope).toEqual({ item: { s: `x` } })
    expect(readPath({ s: `root` }, `s`, scope)).toBe(`x`)
    expect(readPath({ s: `root` }, `/s`, scope)).toBe(`root`)
    expect(readPath({}, ``, scope)).toEqual({ s: `x` })
    expect(runAction({ function: { call: `set`, args: { path: `s`, value: 1 } } }, { a: 1 }, { scope }).data).toEqual({ a: 1 })
    expect(runAction({ function: { call: `set`, args: { path: `/b`, value: 1 } } }, { a: 1 }, { scope }).data).toEqual({ a: 1, b: 1 })
    // Bound rows: a relative set writes into the row.
    expect(runAction({ function: { call: `set`, args: { path: `s`, value: `y` } } }, { rows: [{ s: `x` }] }, { scope: { base: `/rows/0` } }).data).toEqual({ rows: [{ s: `y` }] })
  })

  test(`bind-time.json extra cases replay, rowSlots included`, () => {
    const extra = bindFixture.extra as unknown as { name: string; input: NestedNode; expanded: UiNode; issues: unknown[]; datasets: { data: unknown; bound: UiNode | null; presses: { id: string; outcome: unknown }[]; rowSlots?: { id: string; rows: { index: number; slots: Record<string, UiNode | null> }[] }[] }[] }[]
    expect(extra.map((c) => c.name)).toEqual([`Table/slot-cell:bound-rows`, `Table/slot-cell:literal-rows`, `Text/accessibility:bound-label`, `TabBar/set:author-function`, `Pagination/page:string-data`])
    for (const c of extra) {
      const { root, issues } = reduce(c.input)
      expect(canon(root), c.name).toBe(canon(c.expanded))
      expect(issues, c.name).toEqual(c.issues as typeof issues)
      for (const d of c.datasets) {
        const bound = bindTree(c.expanded, d.data, STRINGS)
        expect(canon(bound), c.name).toBe(canon(d.bound))
        for (const p of d.presses) expect(canon(runAction(byId(c.expanded, p.id)!.on!.press, d.data, STRINGS))).toBe(canon(p.outcome))
        for (const t of d.rowSlots ?? []) {
          const node = byId(bound!, t.id)!
          const rowsProp = byId(c.expanded, t.id)!.props.rows
          for (const r of t.rows)
            for (const [name, slot] of Object.entries(r.slots)) expect(canon(bindRowSlot(node.slots![name], rowsProp, node.props.rows as unknown[], r.index, d.data, STRINGS)), `${c.name} row ${r.index}`).toBe(canon(slot))
        }
      }
    }
    const bound = extra[0].datasets[0]
    expect(bound.rowSlots![0].rows.map((r) => (r.slots.status ? byId(r.slots.status, `cell.label`)!.props.text : null))).toEqual([`ok`, null, 0])
  })
})

describe(`DATA props are copied verbatim (§1, review minor)`, () => {
  test(`Table rows: a {path} row, a {call} row and a "$string.x" cell stay data`, () => {
    expect(isDataSchema(coreCatalog.components.Table.props.rows)).toBe(true)
    expect(isDataSchema(coreCatalog.components.Table.props.columns)).toBe(false)
    const rows = [{ path: `/etc/hosts` }, { call: `not`, args: { value: true } }, { note: `$string.cancel` }]
    const root = expand({ id: `t`, component: `Table`, props: { columns: [{ key: `path`, label: `P` }], rows } })
    const bound = bindTree(root, { etc: { hosts: `PWNED` } }, STRINGS)!
    expect(bound.props.rows).toEqual(rows)
    // Data read through a binding is never descended either.
    expect(bindTree(expand({ id: `t`, component: `Table`, props: { columns: [{ key: `path`, label: `P` }], rows: { path: `/r` } } }), { r: rows, etc: { hosts: `x` } }, STRINGS)!.props.rows).toEqual(rows)
  })

  test(`authored strings still resolve $string; shaped props still resolve nested bindings (Sparkline series)`, () => {
    const root = expand({ id: `s`, component: `Sparkline`, props: { values: { path: `/v` } } })
    expect((bindTree(root, { v: [1, 2] }, STRINGS)!.props.series as { values: unknown }[])[0].values).toEqual([1, 2])
    expect(resolveDynamic(`$string.cancel`, {}, STRINGS)).toBe(`Cancel`)
  })
})

describe(`accessibility channel (§6, review majors)`, () => {
  test(`bindTree resolves a bound accessible name and $string copy in accessibility`, () => {
    const root = expand({ id: `x`, component: `Text`, props: { text: `hi` }, accessibility: { label: { path: `/l` }, description: `$string.close` } })
    expect(bindTree(root, { l: `Hello` }, STRINGS)!.accessibility).toEqual({ label: `Hello`, description: `Close` })
    expect(bindTree(root, {}, STRINGS)!.accessibility).toEqual({ description: `Close` })
  })

  test(`a11y.json roles are machine-readable; every component role and every $a11y role is one of them`, () => {
    expect(A11Y_ROLES).toContain(`button`)
    for (const [name, entry] of Object.entries(COMPONENT_A11Y)) {
      expect(isA11yRole(entry.role), `${name}: ${entry.role}`).toBe(true)
      expect(typeof entry.notes, name).toBe(`string`)
    }
    let roles = 0
    walk(macrosJson.macros, (v) => {
      const a11y = v.$a11y as Record<string, unknown> | undefined
      if (!a11y) return
      for (const key of Object.keys(a11y)) expect([`role`, `label`, `description`, `level`, `expanded`, `current`, `selected`, `pressed`, `checked`, `valueNow`, `valueMin`, `valueMax`, `hidden`, `autoFocus`]).toContain(key)
      walk(a11y.role, () => {})
      const literal = typeof a11y.role === `string` ? [a11y.role] : []
      walk(a11y.role, (o) => {
        for (const x of Object.values(o)) if (Array.isArray(x)) for (const y of x) if (typeof y === `string` && !y.includes(`{`) && !y.startsWith(`props.`)) literal.push(y)
        for (const x of Object.values((o.cases as Record<string, unknown>) ?? {})) if (typeof x === `string`) literal.push(x)
        if (typeof o.default === `string`) literal.push(o.default)
      })
      for (const r of literal) {
        expect(isA11yRole(r), r).toBe(true)
        roles++
      }
    })
    expect(roles).toBeGreaterThan(20)
  })

  test(`macro parts carry role, states and a name: Rating, Collapsible, TabBar, Stepper, Breadcrumb, AlertDialog, AppBar, Sidebar`, () => {
    const rating = expand({ id: `r`, component: `Rating`, props: { value: 3, max: 5 } })
    const star = byId(rating, `r.star.2`)!
    expect(star.accessibility).toMatchObject({ role: `button`, pressed: true })
    expect(bindTree(star, {}, STRINGS)!.accessibility).toEqual({ role: `button`, label: `3 of 5`, pressed: true })
    expect(bindTree(byId(rating, `r.star.0`)!, {}, STRINGS)!.accessibility).toEqual({ role: `button`, label: `1 of 5`, pressed: false })
    const readOnly = expand({ id: `r`, component: `Rating`, props: { value: 4, readOnly: true } })
    expect(bindTree(readOnly, {}, STRINGS)!.accessibility).toEqual({ role: `img`, label: `Rated 4 of 5` })
    expect(byId(readOnly, `r.star.0`)!.accessibility!.role).toBe(`hidden`)

    const bound = expand({ id: `c`, component: `Collapsible`, props: { title: `More`, open: { path: `/open` } }, children: [{ id: `b`, component: `Text`, props: { text: `x` } }] })
    expect(byId(bound, `c.trigger`)!.accessibility).toEqual({ role: `button`, expanded: { path: `/open` } })
    expect(byId(bindTree(bound, { open: true }, STRINGS)!, `c.trigger`)!.accessibility).toEqual({ role: `button`, expanded: true })
    expect(byId(expand({ id: `c`, component: `Collapsible`, props: { title: `More` } }), `c.trigger`)!.accessibility).toEqual({ role: `button`, expanded: false })

    const tabs = bindTree(expand({ id: `tb`, component: `TabBar`, props: { items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: { path: `/tab` } } }), { tab: `b` }, STRINGS)!
    expect(tabs.accessibility).toEqual({ role: `navigation` })
    expect(tabs.children.map((c) => c.accessibility?.current)).toEqual([false, `page`])

    const stepper = bindTree(expand({ id: `s`, component: `Stepper`, props: { steps: [{ label: `Account` }, { label: `Team` }, { label: `Done` }], current: { path: `/c` } } }), { c: 1 }, STRINGS)!
    expect(stepper.children.map((c) => c.accessibility)).toEqual([
      { role: `listitem`, current: false, label: `Step 1 of 3: Account, completed` },
      { role: `listitem`, current: `step`, label: `Step 2 of 3: Team, current` },
      { role: `listitem`, current: false, label: `Step 3 of 3: Done` },
    ])
    expect(byId(stepper, `s.step.0.marker`)!.accessibility).toEqual({ hidden: true })

    const crumbs = bindTree(expand({ id: `bc`, component: `Breadcrumb`, props: { items: [{ label: `Home`, value: `h` }, { label: `Doc`, value: `d` }] } }), {}, STRINGS)!
    expect(crumbs.accessibility).toEqual({ role: `navigation`, label: `Breadcrumb` })
    expect(byId(crumbs, `bc.item.1.current`)!.accessibility).toEqual({ current: `page` })
    expect(byId(crumbs, `bc.item.1.separator`)!.accessibility).toEqual({ hidden: true })

    const alert = expand({ id: `ad`, component: `AlertDialog`, props: { title: `Delete?`, open: true } })
    expect(alert.accessibility).toEqual({ role: `alertdialog` })
    expect(byId(alert, `ad.footer.cancel`)!.accessibility).toEqual({ autoFocus: true })

    const bar = expand({ id: `ab`, component: `AppBar`, props: { title: `Inbox` } })
    expect(bar.accessibility).toEqual({ role: `banner` })
    expect(byId(bar, `ab.body.title`)!.accessibility).toEqual({ role: `heading`, level: 1 })
    const shell = expand({ id: `sb`, component: `Sidebar`, props: {}, slots: { sidebar: { id: `nav`, component: `Text`, props: { text: `n` } } } })
    expect([byId(shell, `sb.sidebar`)!.accessibility, byId(shell, `sb.main`)!.accessibility]).toEqual([{ role: `navigation` }, { role: `main` }])

    // The author's label wins over a root's $a11y label.
    expect(expand({ id: `g`, component: `Group`, props: { title: `Prefs` }, accessibility: { label: `Settings` } }).accessibility).toEqual({ role: `group`, label: `Settings` })
  })

  test(`the expanded kitchen sink carries only vocabulary roles`, () => {
    walk(kitchenExpanded.root, (v) => {
      const a11y = v.accessibility as Record<string, unknown> | undefined
      if (a11y && typeof a11y.role === `string`) expect(isA11yRole(a11y.role), a11y.role).toBe(true)
    })
  })
})

describe(`$set meets an author function (§1, review minor)`, () => {
  test(`the author's function is kept, no set is added, the reducer reports it once`, () => {
    const { root, issues } = reduce({ id: `tb`, component: `TabBar`, props: { items: [{ label: `A`, value: `a` }, { label: `B`, value: `b` }], value: { path: `/tab` } }, on: { change: { function: { call: `openUrl`, args: { url: `https://x` } } } } })
    expect(root.children[0].on!.press).toEqual({ function: { call: `openUrl`, args: { url: `https://x` } } })
    expect(issues).toEqual([{ id: `tb`, message: `on.change: a function action replaces the two-way set of props.value; write it in that function's handler or use an event` }])
    // An event handler still gets the set.
    const ok = reduce({ id: `tb`, component: `TabBar`, props: { items: [{ label: `A`, value: `a` }], value: { path: `/tab` } }, on: { change: { event: { name: `tab` } } } })
    expect(ok.issues).toEqual([])
    expect(ok.root.children[0].on!.press).toEqual({ event: { name: `tab`, context: { value: `a` } }, function: { call: `set`, args: { path: `/tab`, value: `a` } } })
  })
})

describe(`numeric equality and clamping (§1, review minor)`, () => {
  test(`eq: a number equals a string spelling it, nothing else coerces`, () => {
    expect(valuesEqual(5, `5`)).toBe(true)
    expect(valuesEqual(` 2.5 `, 2.5)).toBe(true)
    expect(valuesEqual(0, ``)).toBe(false)
    expect(valuesEqual(1, true)).toBe(false)
    expect(valuesEqual(`a`, `a`)).toBe(true)
    expect(valuesEqual(NaN, `NaN`)).toBe(false)
    expect(CORE_FUNCTIONS.eq({ a: `3`, b: 3 })).toBe(true)
    expect(evalValue({ $eq: [`5`, 5] }, { id: `x`, props: {}, vars: {} })).toBe(true)
  })

  test(`clamp holds a value inside [min, max]; a missing bound does not clamp`, () => {
    expect(clampNumber(6, 1, 5)).toBe(5)
    expect(clampNumber(-1, 1, 5)).toBe(1)
    expect(clampNumber(`4`, 1, undefined)).toBe(4)
    expect(clampNumber(9, undefined, null)).toBe(9)
    expect(clampNumber(3, 4, 2)).toBe(4)
  })

  test(`Pagination over string data: next disabled on the last page, writes clamped`, () => {
    const root = expand({ id: `p`, component: `Pagination`, props: { totalPages: 5, page: { path: `/page` } } })
    const bound = bindTree(root, { page: `5` }, STRINGS)!
    expect(byId(bound, `p.next`)!.props.disabled).toBe(true)
    expect(byId(bound, `p.prev`)!.props.disabled).toBe(false)
    expect(runAction(byId(root, `p.next`)!.on!.press, { page: `5` }).data).toEqual({ page: 5 })
    expect(runAction(byId(root, `p.prev`)!.on!.press, { page: 1 }).data).toEqual({ page: 1 })
    expect(byId(bound, `p.label`)!.accessibility).toEqual({ role: `status`, label: `Page 5 of 5` })
  })
})

describe(`the bind function table (§1, review minor)`, () => {
  test(`every call the expander emits (catalog-macros.json, bind-time.json) is in BIND_FUNCTIONS`, () => {
    const called = new Set<string>()
    walk([macrosFixture, bindFixture], (v) => {
      if (typeof v.call === `string` && (v.args === undefined || typeof v.args === `object`)) called.add(v.call)
    })
    called.delete(`set`)
    called.delete(`openUrl`)
    for (const name of called) expect(BIND_FUNCTIONS[name], name).toBeDefined()
    for (const name of [`not`, `fill`, `clamp`]) expect(called.has(name), name).toBe(true)
    expect(BIND_FUNCTION_NAMES).toEqual(expect.arrayContaining([`not`, `and`, `or`, `required`, ...Object.keys(CORE_FUNCTIONS)]))
    expect(coreCatalog.functions.names).toEqual(expect.arrayContaining(BIND_FUNCTION_NAMES as string[]))
  })
})

describe(`strings (§4, review minor)`, () => {
  test(`sentences are one id with placeholders; the fragment "of" is gone`, () => {
    expect(DEFAULT_STRINGS.of).toBeUndefined()
    for (const id of [`ratingStar`, `ratingValue`, `chartSummary`, `sparklineSummary`, `stepDone`, `stepCurrent`, `stepUpcoming`, `pagination`, `breadcrumb`]) expect(DEFAULT_STRINGS[id], id).toBeDefined()
    expect(formatString(DEFAULT_STRINGS.ratingStar, { value: 2, max: 5 })).toBe(`2 of 5`)
    expect(formatString(DEFAULT_STRINGS.chartSummary, chartSummaryParams([{ name: `Runs`, values: [3, -1, 8] }]))).toBe(`Chart of Runs, values from -1 to 8`)
    expect(formatString(DEFAULT_STRINGS.sparklineSummary, sparklineSummaryParams([4, 1, 9, 6]))).toBe(`Trend from 4 to 6, low 1, high 9`)
  })

  test(`every $fill template names a known id and only its placeholders`, () => {
    let fills = 0
    walk(macrosJson.macros, (v) => {
      if (!(`$fill` in v)) return
      fills++
      const [template, params] = v.$fill as [unknown, Record<string, unknown>]
      const ids: string[] = []
      walk([template], (o) => Object.values(o).forEach((x) => (Array.isArray(x) ? x : [x]).forEach((y) => typeof y === `string` && y.startsWith(`$string.`) && ids.push(y.slice(8)))))
      if (typeof template === `string`) ids.push(template.slice(8))
      for (const id of ids) {
        expect(DEFAULT_STRINGS[id], id).toBeDefined()
        const names = [...DEFAULT_STRINGS[id].matchAll(/\{([a-zA-Z]+)\}/g)].map((m) => m[1]).sort()
        expect(names, id).toEqual(Object.keys(params).sort())
      }
    })
    expect(fills).toBeGreaterThanOrEqual(4)
  })
})

describe(`RTL glyphs (§4, review major)`, () => {
  test(`the mirrored glyphs exist and cover every directional built-in and macro glyph`, () => {
    for (const icon of RTL_MIRRORED_ICONS) expect(ICON_NAMES as readonly string[], icon).toContain(icon)
    for (const [slot, icon] of Object.entries(coreCatalog.builtinIcons)) {
      if (slot.startsWith(`$`)) continue
      if (/-(left|right)$|^ui-back$/.test(icon)) expect(RTL_MIRRORED_ICONS, slot).toContain(icon)
    }
    walk(macrosJson.macros, (v) => {
      const props = v.props as Record<string, unknown> | undefined
      if (typeof props?.icon === `string` && /-(left|right)$|^ui-back$/.test(props.icon)) expect(RTL_MIRRORED_ICONS).toContain(props.icon)
      if (v.component === `Icon` && typeof props?.name === `string` && /-(left|right)$/.test(props.name)) expect(RTL_MIRRORED_ICONS).toContain(props.name)
    })
    expect(mirrorsInRtl(`ui-back`, `rtl`)).toBe(true)
    expect(mirrorsInRtl(`ui-back`, `ltr`)).toBe(false)
    expect(mirrorsInRtl(`media-skip-next`, `rtl`)).toBe(false)
    expect(mirrorsInRtl(`ui-chevron-up`, `rtl`)).toBe(false)
  })
})

describe(`locale (§4, review minor)`, () => {
  test(`bare RTL and common languages take CLDR's likely region`, () => {
    expect(weekStart(`ur`)).toBe(0)
    expect(weekStart(`ps`)).toBe(6)
    expect(weekStart(`dv`)).toBe(5)
    expect(weekStart(`ckb`)).toBe(6)
    expect(weekStart(`bn`)).toBe(0)
    expect(weekStart(`sd`)).toBe(0)
    expect(weekStart(`am`)).toBe(0)
    expect(weekStart(`yi`)).toBe(1)
    for (const lang of localeJson.rtl) expect(Object.keys(localeJson.likelyRegion), lang).toContain(lang)
    expect(Object.keys(localeJson.likelyRegion).length).toBeGreaterThan(60)
  })

  test(`deprecated codes normalise before every lookup`, () => {
    expect(parseLocale(`iw-IL`)).toEqual({ language: `he`, region: `IL` })
    expect(textDirection(`iw`)).toBe(`rtl`)
    expect(textDirection(`ji`)).toBe(`rtl`)
    expect(weekStart(`in`)).toBe(0)
    expect(weekStart(`iw`)).toBe(0)
  })
})

describe(`chart extents (§3, review minor)`, () => {
  test(`a 150k-point series does not overflow the stack`, () => {
    const values = Array.from({ length: 150_000 }, (_, i) => Math.sin(i / 100) * 50)
    const ext = chartExtent(`sparkline`, [{ name: `m`, values }])
    expect(ext.min).toBeCloseTo(-50, 3)
    expect(ext.max).toBeCloseTo(50, 3)
    expect(chartExtent(`line`, [{ name: `m`, values }]).min).toBeCloseTo(-50, 3)
  })

  test(`stackedBar stacks positives up and negatives down; all-negative bars keep the 0 baseline`, () => {
    const series = [{ name: `a`, values: [3, -2] }, { name: `b`, values: [4, -5] }]
    expect(chartExtent(`stackedBar`, series)).toEqual({ min: -7, max: 7 })
    expect(chartExtent(`bar`, [{ name: `a`, values: [-3, -1] }])).toEqual({ min: -3, max: 0 })
    expect(chartExtent(`bar`, [{ name: `a`, values: [1, 5, NaN] }])).toEqual({ min: 0, max: 5 })
  })
})

describe(`reducer: visible on the basic catalog (review minor)`, () => {
  test(`a basic Text keeps its visible and a bad one is reported`, () => {
    const { root } = reduceSurface(
      [
        { id: `root`, component: `Column`, children: [`t`] },
        { id: `t`, component: `Text`, text: `hi`, visible: { path: `/show` } },
      ],
      { catalogId: A2UI_BASIC_CATALOG_ID }
    )
    expect(root.children[0].visible).toEqual({ path: `/show` })
    expect(bindTree(root, { show: false })!.children).toEqual([])
    const bad = reduceSurface([{ id: `root`, component: `Text`, text: `hi`, visible: `yes` as unknown as boolean }], { catalogId: A2UI_BASIC_CATALOG_ID })
    expect(bad.issues).toContainEqual({ id: `root`, message: `visible: expected a boolean, a binding or a function call` })
  })
})
