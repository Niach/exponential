import { describe, expect, it } from "vitest"
import { resolveDynamic, sourceWithParams } from "@exponential_app/ui"
import { parseSource } from "@exponential_app/ui"
import { parseRecordsQuery } from "@exp/domain-contract/app-records"
import template from "@exp/ui/exponential-ui/templates/working-hours.json"
import { validateAppPackage } from "@/lib/apps/package"

// VAPP-153 (the #1221 review): the Working hours template's month switcher
// and member picker re-bind the entries through `bind`; with nobody picked
// (`/who` = "") the query must stay the viewer's own rows (`mine=1`), with a
// member picked it must be that member's (`user=<id>`, `mine=0`).
type Component = { id: string; on?: Record<string, { functionCall: { call: string; args: Record<string, unknown> } }> }
const components = (template as { templates: { month: { components: Component[] } } }).templates.month.components
const data = (template as { templates: { month: { data: Record<string, unknown> } } }).templates.month.data

function rebind(id: string, event: string, who: string) {
  const call = components.find((c) => c.id === id)!.on![event]!.functionCall
  expect(call.call).toBe(`bind`)
  const args = resolveDynamic(call.args, { ...data, who, entries: { ...(data.entries as object), month: { key: `2026-10`, prev: `2026-09`, next: `2026-11` } } }) as { source: string; params: Record<string, unknown> }
  const uri = sourceWithParams(args.source, args.params)
  return parseRecordsQuery(parseSource(uri)!.params)
}

describe(`the Working hours template (VAPP-119 / VAPP-153)`, () => {
  it(`is a valid app package`, () => {
    expect(() => validateAppPackage(template)).not.toThrow()
  })
  it(`keeps the viewer's rows while nobody is picked`, () => {
    for (const [id, event] of [[`prev`, `press`], [`next`, `press`], [`who`, `change`]] as const) {
      const q = rebind(id, event, ``)
      expect(q.mine, id).toBe(true)
      expect(q.user, id).toBeNull()
      expect(q.kind).toBe(`entry`)
    }
    expect(rebind(`prev`, `press`, ``).month).toBe(`2026-09`)
    expect(rebind(`next`, `press`, ``).month).toBe(`2026-11`)
  })
  it(`reads the picked member's rows`, () => {
    for (const [id, event] of [[`prev`, `press`], [`who`, `change`]] as const) {
      const q = rebind(id, event, `u2`)
      expect(q.mine, id).toBe(false)
      expect(q.user, id).toBe(`u2`)
    }
    expect(rebind(`who`, `change`, `u2`).month).toBe(`2026-10`)
  })
})
