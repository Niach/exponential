// VAPP-217: fixtures/record-macros.json replayed (the Rust core replays the
// same file in host::record_macros).
import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/record-macros.json" with { type: "json" }
import { reduceSurface } from "../reducer"
import { CORE_CATALOG_ID } from "../catalog"
import type { FlatComponent } from "../types"
import { fillRecordMacros } from "./record-macros"
import { templateMessages, validatePackage } from "./package"

const records = fixture.records as never

describe(`record macros: the host fill (fixtures/record-macros.json)`, () => {
  for (const c of fixture.fill) {
    test(c.name, () => {
      const got = fillRecordMacros(c.components as never, ((c as { bindings?: unknown }).bindings ?? []) as never, records, ((c as { options?: object }).options ?? {}) as never)
      expect(got).toEqual(c.expected as never)
    })
    test(`${c.name}: the filled template reduces clean`, () => {
      expect(reduceSurface(c.expected.components as unknown as FlatComponent[], { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
    })
  }
  for (const c of fixture.install) test(`install: ${c.name}`, () => expect(validatePackage(c.package)).toEqual(c.issues as never))

  test(`a date-time field expands to DatePicker + TimePicker in a FieldGroup`, () => {
    const form = fixture.fill[1]!.expected.components.find((c) => c.component === `RecordForm`)!
    const { root } = reduceSurface(fixture.fill[1]!.expected.components as unknown as FlatComponent[], { catalogId: CORE_CATALOG_ID })
    const index = (form as unknown as { fields: { widget: string }[] }).fields.findIndex((f) => f.widget === `datetime`)
    const find = (n: { id: string; children?: unknown[] }, id: string): { component: string; props: Record<string, unknown> } | null => {
      if (n.id === id) return n as never
      for (const c of (n.children ?? []) as { id: string }[]) {
        const hit = find(c, id)
        if (hit) return hit
      }
      return null
    }
    const at = `f.fields.field.${index}.datetime`
    expect(find(root as never, at)?.component).toBe(`Box`)
    expect(find(root as never, `${at}.day`)?.props.value).toEqual({ path: `/recordForm/fParts/loggedAt/date` })
    expect(find(root as never, `${at}.clock`)?.props.value).toEqual({ path: `/recordForm/fParts/loggedAt/time` })
  })

  test(`templateMessages sends the filled components and the rows binding`, () => {
    const pkg = fixture.install[0]!.package as never
    const messages = templateMessages(pkg, `main`, `s`) ?? []
    const components = (messages[1] as { updateComponents: { components: { id: string; cols?: unknown }[] } }).updateComponents.components
    expect(components.map((c) => c.id)).toEqual([`root`, `m`, `m__row`, `m__rowDelete`])
    expect(messages.some((m) => (m as { bindDataModel?: { path: string; source: string } }).bindDataModel?.source === `records:deal?sort=updatedAt&order=desc`)).toBe(true)
  })
})
