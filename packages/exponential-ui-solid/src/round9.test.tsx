// Round 9: Solid's half of §6.6 (severities), §5.1 (live + hidden), §9.1 (the
// Markdown inline profile) and §1.5 (`createSurface` over the core's store).

import { render } from "@solidjs/testing-library"
import { describe, expect, it } from "vitest"
import { A2UI_VERSION, CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { createSurface } from "./create-surface"
import { ExponentialSurface } from "./surface"

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root!

describe(`checkSeverities`, () => {
  for (const c of fixture.checkSeverities) {
    it(c.name, () => {
      const actions: { name: string; context: Record<string, unknown> }[] = []
      const root = tree({
        id: `f`,
        component: `Form`,
        props: { name: `f` },
        on: { submit: { event: { name: `submit` } }, invalid: { event: { name: `invalid` } } },
        children: [
          { id: `x`, component: `Input`, props: { label: `X`, name: `x`, value: `v`, validateOn: `submit`, checks: c.checks } },
          { id: `go`, component: `Button`, props: { label: `Send`, submit: true } },
        ],
      } as NestedNode)
      const { container } = render(() => <ExponentialSurface root={root} data={c.data} theme="neutral" id="sv" host={{ onAction: (e: { name: string; context: Record<string, unknown> }) => void actions.push(e) }} />)
      ;(container.querySelector(`[data-xui-id="go"]`) as HTMLElement).click()
      const invalid = actions.find((a) => a.name === `invalid`)
      expect(((invalid?.context.errors ?? []) as { message: string }[]).map((e) => e.message)).toEqual(c.errors)
      expect(actions.some((a) => a.name === `submit`)).toBe(c.submitted)
      const part = container.querySelector(`[data-xui-id="x.error"]`)
      if (!c.shown) expect(part).toBeNull()
      else {
        expect(part!.textContent).toBe(c.shown.message)
        expect(part!.getAttribute(`role`)).toBe(c.shown.severity === `error` ? `alert` : `status`)
        expect(part!.getAttribute(`data-xs`)).toBe(c.shown.severity === `error` ? null : c.shown.severity)
      }
    })
  }
  for (const c of fixture.buttonChecks) {
    it(`Button: ${c.name}`, () => {
      const root = tree({ id: `b`, component: `Button`, props: { label: `Go`, checks: c.checks } } as NestedNode)
      const { container } = render(() => <ExponentialSurface root={root} data={c.data} theme="neutral" />)
      expect((container.querySelector(`[data-xui-id="b"]`) as HTMLButtonElement).disabled).toBe(c.disabled)
    })
  }
})

describe(`accessibility + inline Markdown`, () => {
  const paint = (node: Record<string, unknown>, data: Record<string, unknown> = {}) => render(() => <ExponentialSurface root={tree({ id: `n`, ...node } as NestedNode)} data={data} theme="neutral" />).container
  it(`live and hidden`, () => {
    const c = paint({ component: `Text`, props: { text: `t`, live: `polite` }, accessibility: { live: `assertive`, hidden: { "@path": `/h` } } }, { h: `true` })
    const n = c.querySelector(`[data-xui-id="n"]`)!
    expect(n.getAttribute(`aria-live`)).toBe(`assertive`)
    expect(n.getAttribute(`aria-hidden`)).toBe(`true`)
  })
  it(`inline paragraph and heading`, () => {
    const c = paint({ component: `Markdown`, props: { text: `## Two\n\nplain`, profile: `inline`, variant: `caption` } })
    expect(c.querySelector(`p`)!.getAttribute(`data-r-variant`)).toBe(`caption`)
    expect(c.querySelector(`h2`)!.getAttribute(`data-m-level`)).toBe(`h2`)
  })
})

describe(`createSurface goes through the core`, () => {
  it(`refuses a bad version, applies, deletes, resets`, () => {
    const s = createSurface({ data: { t: `seed` } })
    const create = { version: A2UI_VERSION, createSurface: { surfaceId: `main`, catalogId: CORE_CATALOG_ID, components: [{ id: `root`, component: `Text`, text: `x` }] } }
    s.apply({ ...create, version: `2.0` })
    expect(s.root).toBeNull()
    s.apply(create)
    expect(s.root?.id).toBe(`root`)
    expect(s.data).toEqual({ t: `seed` })
    s.apply({ version: A2UI_VERSION, deleteSurface: { surfaceId: `main` } })
    expect(s.deleted).toBe(true)
    s.reset()
    expect(s.deleted).toBe(false)
    expect(s.root).toBeNull()
  })
})
