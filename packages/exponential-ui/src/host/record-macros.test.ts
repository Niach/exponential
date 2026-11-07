// VAPP-217: fixtures/record-macros.json replayed (the Rust core replays the
// same file in host::record_macros).
import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/record-macros.json" with { type: "json" }
import { reduceSurface } from "../reducer"
import { CORE_CATALOG_ID } from "../catalog"
import type { FlatComponent } from "../types"
import { RecordRefusal, fillRecordMacros, isRecordMacroBinding } from "./record-macros"
import { ExponentialHost } from "./runtime"
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
    expect(find(root as never, `${at}.day`)?.props.value).toEqual({ "@path": `/recordForm/fParts/loggedAt/date` })
    expect(find(root as never, `${at}.clock`)?.props.value).toEqual({ "@path": `/recordForm/fParts/loggedAt/time` })
  })

  test(`the runtime answers a record form's write`, async () => {
    const pkg = fixture.install[0]!.package as never
    let refuse = true
    const host = new ExponentialHost({
      packages: [pkg],
      policy: { functions: { allow: [`*`] } },
      functions: {
        "records_upsert": () => {
          if (refuse) throw new RecordRefusal(`bad`)
          return { id: `r` }
        },
      },
    } as never)
    host.receive({ version: `1.0`, applyTemplate: { surfaceId: `s`, templateId: `main`, packageId: (pkg as { id: string }).id } } as never)
    const call = { surfaceId: `s`, componentId: `f`, name: `records_upsert`, args: { form: { error: `/e`, close: `/open` } } }
    host.surface(`s`)!.setData(`/open`, true)
    await host.callFunction(call)
    expect(host.surface(`s`)!.data).toMatchObject({ e: `bad`, open: true })
    refuse = false
    await host.callFunction(call)
    expect(host.surface(`s`)!.data).toMatchObject({ e: null, open: false })
  })

  test(`a macro's own source passes sources, nothing else does`, () => {
    expect(isRecordMacroBinding(`/recordMembers`, `exp:members`)).toBe(true)
    expect(isRecordMacroBinding(`/recordIssues`, `exp:issues`)).toBe(true)
    expect(isRecordMacroBinding(`/x`, `exp:members`)).toBe(false)
    expect(isRecordMacroBinding(`/recordMembers`, `exp:issues`)).toBe(false)
  })

  test(`templateMessages sends the filled components and the rows binding`, () => {
    const pkg = fixture.install[0]!.package as never
    const messages = templateMessages(pkg, `main`, `s`) ?? []
    const components = (messages[1] as { updateComponents: { components: { id: string; cols?: unknown }[] } }).updateComponents.components
    expect(components.map((c) => c.id)).toEqual([`root`, `m`, `m__row`, `m__rowDelete`])
    expect(messages.some((m) => (m as { bindDataModel?: { path: string; source: string } }).bindDataModel?.source === `records:deal?sort=updatedAt&order=desc`)).toBe(true)
  })
})
