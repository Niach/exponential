// VAPP-87 acceptance: every component of the catalog fixture renders (a DOM
// snapshot per case), the macro, basic-map and extension fixtures render,
// and the kitchen sink renders under `exponential`, `neutral` and the
// `playful` test theme with no code change — the recipes do the work.

import { describe, expect, it } from "vitest"
import { render } from "@testing-library/react"
import { CORE_CATALOG_ID, A2UI_BASIC_CATALOG_ID, BUILTIN_THEME_IDS, defineExtension, reduceNested, reduceSurface, expandMacros, builtinTheme } from "@exponential-at/ui"
import type { ExtensionDef, FlatComponent, NestedNode, UiNode } from "@exponential-at/ui"
import componentsFixture from "@exponential-at/ui/fixtures/catalog-components.json"
import macrosFixture from "@exponential-at/ui/fixtures/catalog-macros.json"
import basicFixture from "@exponential-at/ui/fixtures/catalog-basic-map.json"
import extensionFixture from "@exponential-at/ui/fixtures/catalog-extension.json"
import kitchenSink from "@exponential-at/ui/fixtures/kitchen-sink.json"
import { ExponentialSurface } from "./surface"
import { defineReactExtension } from "./extensions"
import { compileTheme } from "./theme-css"
import type { ExtensionComponentProps } from "./host"

interface ComponentCase {
  name: string
  node: NestedNode
}
interface MacroCase {
  name: string
  input: NestedNode
  expected: UiNode
}
interface BasicCase {
  name: string
  components: FlatComponent[]
  expected: { root: UiNode; issues: unknown[] }
}
interface ExtensionCase {
  name: string
  catalogId: string
  components: FlatComponent[]
  expected: { root: UiNode; issues: unknown[] }
}

const cases = (componentsFixture as { cases: ComponentCase[] }).cases
const macroCases = (macrosFixture as { cases: MacroCase[] }).cases
const basicCases = (basicFixture as { cases: BasicCase[] }).cases
const ext = extensionFixture as unknown as { extension: ExtensionDef; cases: ExtensionCase[] }

/** The surface's DOM minus the two stylesheets (the theme sheet is locked
 *  by its own test; node ids and part classes stay). */
function markup(container: HTMLElement): string {
  const clone = container.cloneNode(true) as HTMLElement
  for (const s of clone.querySelectorAll(`style`)) s.remove()
  // The theme scope hash and React ids would churn the snapshot.
  return clone.innerHTML.replace(/xui-t-[a-z0-9-]+-[a-z0-9]+/g, `xui-t-THEME`).replace(/xui-s-[a-zA-Z0-9_]+/g, `xui-s-SURFACE`).replace(/\b(id|for|aria-labelledby|aria-describedby|aria-controls)="[^"]*"/g, `$1="ID"`)
}

describe(`catalog fixture → every component renders`, () => {
  for (const c of cases) {
    it(c.name, () => {
      const { root, issues } = reduceNested(c.node, { catalogId: CORE_CATALOG_ID })
      expect(issues).toEqual([])
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="fx" />)
      const el = container.querySelector(`[data-xui-id="${root.id}"]`)
      // A closed Toast paints nothing (it lives in the toast layer when open).
      if (!(root.component === `Toast` && root.props.open === false)) expect(el, `root element of ${c.name}`).not.toBeNull()
      // Every component has a painter: no Unknown placeholder anywhere.
      expect(container.querySelector(`[data-xui-unknown]`), `${c.name} must not fall back to Unknown`).toBeNull()
      expect(container.querySelector(`[data-xui-c="Unknown"]`), `${c.name} must not fall back to Unknown`).toBeNull()
      expect(markup(container)).toMatchSnapshot()
    })
  }
})

describe(`macro fixture renders`, () => {
  for (const c of macroCases) {
    it(c.name, () => {
      const root = expandMacros(reduceNested(c.input, { catalogId: CORE_CATALOG_ID, expand: false }).root)
      expect(root).toEqual(c.expected)
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="mx" />)
      expect(container.querySelector(`[data-xui-id="${root.id}"]`)).not.toBeNull()
    })
  }
})

describe(`basic-map fixture renders`, () => {
  for (const c of basicCases) {
    it(c.name, () => {
      const { root, issues } = reduceSurface(c.components, { catalogId: A2UI_BASIC_CATALOG_ID })
      expect({ root, issues }).toEqual(c.expected)
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="bx" />)
      expect(container.querySelector(`[data-xui-id="root"]`)).not.toBeNull()
    })
  }
})

describe(`extension fixture renders`, () => {
  const catalog = defineExtension(ext.extension)
  const painted: string[] = []
  // The example extension's native (round 1 renamed it: Sparkline is a core macro now).
  const TrendLine = ({ props, rootProps }: ExtensionComponentProps) => {
    painted.push(`TrendLine`)
    return <svg {...(rootProps as Record<string, unknown>)} data-values={String((props.values as number[] | undefined)?.length ?? 0)} />
  }
  const reactExt = defineReactExtension({ catalog, components: { TrendLine } })
  for (const c of ext.cases) {
    it(c.name, () => {
      const { root, issues } = reduceSurface(c.components, { catalogId: c.catalogId, extensions: [catalog] })
      expect({ root, issues }).toEqual(c.expected)
      const { container } = render(<ExponentialSurface root={root} theme="neutral" id="ex" extensions={[reactExt]} />)
      expect(container.querySelector(`[data-xui-id="root"]`)).not.toBeNull()
      if (JSON.stringify(root).includes(`"TrendLine"`)) expect(container.querySelector(`[data-xui-c="TrendLine"]`)).not.toBeNull()
      expect(markup(container)).toMatchSnapshot()
    })
  }
  it(`the extension component receives resolved props and the theme`, () => {
    let seen: ExtensionComponentProps | null = null
    const Probe = (p: ExtensionComponentProps) => {
      seen = p
      return <div {...(p.rootProps as Record<string, unknown>)} />
    }
    const probeExt = defineReactExtension({ catalog, components: { TrendLine: Probe } })
    const { root } = reduceSurface([{ id: `root`, component: `TrendLine`, values: { path: `/series` } }], { catalogId: catalog.id, extensions: [catalog] })
    render(<ExponentialSurface root={root} theme="playful" data={{ series: [1, 2, 3] }} extensions={[probeExt]} id="px" />)
    expect(seen).not.toBeNull()
    expect(seen!.props.values).toEqual([1, 2, 3])
    expect(seen!.theme.id).toBe(`playful`)
    expect(seen!.tokens.radius.full).toBe(9999)
  })
})

describe(`kitchen sink × built-in themes`, () => {
  const { root, issues } = reduceNested(kitchenSink as unknown as NestedNode, { catalogId: CORE_CATALOG_ID })
  it(`reduces cleanly`, () => expect(issues).toEqual([]))
  for (const id of BUILTIN_THEME_IDS) {
    for (const mode of [`light`, `dark`] as const) {
      it(`${id} / ${mode}`, () => {
        const { container } = render(<ExponentialSurface root={root} theme={id} mode={mode} id={`ks-${id}`} />)
        const surface = container.firstElementChild as HTMLElement
        expect(surface.dataset.xuiTheme).toBe(id)
        expect(surface.dataset.xuiMode).toBe(mode)
        expect(container.querySelector(`[data-xui-c="Unknown"]`)).toBeNull()
        // Every node of the tree is in the DOM, overlays excepted (closed).
        const ids = new Set(Array.from(container.querySelectorAll(`[data-xui-id]`)).map((e) => (e as HTMLElement).dataset.xuiId))
        for (const id of [`root`, `header`, `grid`, `nav-card`, `row-1`, `echo-field`, `form-send`, `flex-demo`, `stats`]) expect(ids.has(id), id).toBe(true)
        const sheet = compileTheme(builtinTheme(id))
        expect(surface.classList.contains(sheet.scope)).toBe(true)
        expect(container.querySelector(`style[data-xui-style="theme"]`)!.textContent).toContain(sheet.scope)
      })
    }
  }
  it(`the three themes produce three different recipe sheets from one tree`, () => {
    const sheets = BUILTIN_THEME_IDS.map((id) => compileTheme(builtinTheme(id)).css)
    expect(new Set(sheets).size).toBe(3)
    expect(sheets[2]).toContain(`.xui-Button-root{`)
    expect(sheets[2]).toContain(`border-radius:var(--xui-radius-full)`)
  })
})
