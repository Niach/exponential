// #AXC-19: A2UI 1.0's check mixin rides the wire as `@checks` (what the
// Express compiler emits); the legacy bare `checks` still reads; inside, the
// prop is `checks`. apps/desktop/crates/eui-core/tests/checks_wire.rs twins it.
import { describe, expect, it } from "bun:test"
import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, flattenSurface, reduceSurface } from "./index"
import type { FlatComponent } from "./types"

const required = (path: string, message: string) => [{ condition: { "@call": `required`, args: { value: { "@path": path } } }, message }]
const input = (extra: Record<string, unknown>): FlatComponent[] => [{ id: `root`, component: `Input`, label: `Title`, name: `title`, value: { "@path": `/t` }, ...extra }]

describe(`@checks on the wire`, () => {
  it(`reads @checks as the checks prop`, () => {
    const { root, issues } = reduceSurface(input({ "@checks": required(`/t`, `Needed`) }), { catalogId: CORE_CATALOG_ID, expand: false })
    expect(issues).toEqual([])
    expect(root?.props.checks).toEqual(required(`/t`, `Needed`))
    expect(root?.props[`@checks`]).toBeUndefined()
  })
  it(`still reads the legacy bare checks`, () => {
    const { root, issues } = reduceSurface(input({ checks: required(`/t`, `Old`) }), { catalogId: CORE_CATALOG_ID, expand: false })
    expect(issues).toEqual([])
    expect(root?.props.checks).toEqual(required(`/t`, `Old`))
  })
  it(`@checks wins over checks`, () => {
    const { root } = reduceSurface(input({ checks: required(`/t`, `Old`), "@checks": required(`/t`, `New`) }), { catalogId: CORE_CATALOG_ID, expand: false })
    expect(root?.props.checks).toEqual(required(`/t`, `New`))
  })
  it(`a basic-catalog TextField maps @checks`, () => {
    const flat: FlatComponent[] = [{ id: `root`, component: `TextField`, label: `Email`, value: { "@path": `/e` }, "@checks": required(`/e`, `Needed`) }]
    const { root } = reduceSurface(flat, { catalogId: A2UI_BASIC_CATALOG_ID, expand: false })
    expect(root?.props.checks).toEqual(required(`/e`, `Needed`))
  })
  it(`flattenSurface writes @checks`, () => {
    const [flat] = flattenSurface({ component: `Input`, props: { label: `Title`, name: `title`, checks: required(`/t`, `Needed`) } } as never)
    expect(flat[`@checks`]).toEqual(required(`/t`, `Needed`))
    expect(flat.checks).toBeUndefined()
  })
})
