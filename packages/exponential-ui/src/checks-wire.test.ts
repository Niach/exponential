// #AXC-19, round 9 §5.1: the envelope mixins ride the wire under `@`
// (`@checks`, `@accessibility`, `@metadata`, `@catalogId`); a bare one is an
// ordinary prop or refused; inside, the check prop is `checks`.
// apps/desktop/crates/eui-core/tests/checks_wire.rs twins it.
import { describe, expect, it } from "bun:test"
import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID, checkEntry, flattenSurface, reduceNested, reduceSurface } from "./index"
import type { FlatComponent } from "./types"

const required = (path: string, message: string) => [{ condition: { "@call": `required`, args: { value: { "@path": path } } }, message }]
const input = (extra: Record<string, unknown>): FlatComponent[] => [{ id: `root`, component: `Input`, label: `Title`, name: `title`, value: { "@path": `/t` }, ...extra }]
const refusal = { id: `root`, message: `checks is not a declared prop (write @checks)`, code: `INVALID_MESSAGE`, path: `/components/root/checks` }

describe(`@checks on the wire`, () => {
  it(`reads @checks as the checks prop`, () => {
    const { root, issues } = reduceSurface(input({ "@checks": required(`/t`, `Needed`) }), { catalogId: CORE_CATALOG_ID, expand: false })
    expect(issues).toEqual([])
    expect(root?.props.checks).toEqual(required(`/t`, `Needed`))
    expect(root?.props[`@checks`]).toBeUndefined()
  })
  it(`refuses the bare checks`, () => {
    const { root, issues } = reduceSurface(input({ checks: required(`/t`, `Old`) }), { catalogId: CORE_CATALOG_ID, expand: false })
    expect(issues).toEqual([refusal])
    expect(root?.props.checks).toBeUndefined()
    expect(checkEntry(input({ checks: [] })[0], { catalogId: CORE_CATALOG_ID })).toEqual({ pointer: `/checks`, message: refusal.message })
  })
  it(`refuses the bare checks beside @checks too`, () => {
    const entry = input({ checks: required(`/t`, `Old`), "@checks": required(`/t`, `New`) })
    const { root, issues } = reduceSurface(entry, { catalogId: CORE_CATALOG_ID, expand: false })
    expect(issues).toEqual([refusal])
    expect(root?.props.checks).toEqual(required(`/t`, `New`))
    expect(checkEntry(entry[0], { catalogId: CORE_CATALOG_ID })).toEqual({ pointer: `/checks`, message: refusal.message })
  })
  it(`a nested node carries @checks next to props; props.checks is refused`, () => {
    const props = { label: `Title`, name: `title` }
    const good = reduceNested({ id: `root`, component: `Input`, props, "@checks": required(`/t`, `Needed`) }, { catalogId: CORE_CATALOG_ID, expand: false })
    expect(good.issues).toEqual([])
    expect(good.root.props.checks).toEqual(required(`/t`, `Needed`))
    const bad = reduceNested({ id: `root`, component: `Input`, props: { ...props, checks: required(`/t`, `Old`) } }, { catalogId: CORE_CATALOG_ID, expand: false })
    expect(bad.issues).toEqual([refusal])
    expect(bad.root.props.checks).toBeUndefined()
  })
  it(`a basic-catalog TextField maps @checks and refuses the bare one`, () => {
    const flat = (key: string): FlatComponent[] => [{ id: `root`, component: `TextField`, label: `Email`, value: { "@path": `/e` }, [key]: required(`/e`, `Needed`) }]
    expect(reduceSurface(flat(`@checks`), { catalogId: A2UI_BASIC_CATALOG_ID, expand: false }).root?.props.checks).toEqual(required(`/e`, `Needed`))
    const bare = reduceSurface(flat(`checks`), { catalogId: A2UI_BASIC_CATALOG_ID, expand: false })
    expect(bare.issues).toEqual([refusal])
    expect(bare.root?.props.checks).toBeUndefined()
  })
  it(`flattenSurface writes the @ keys`, () => {
    const [flat] = flattenSurface({ component: `Input`, props: { label: `Title`, name: `title`, checks: required(`/t`, `Needed`) }, accessibility: { label: `T` } } as never)
    expect(flat[`@checks`]).toEqual(required(`/t`, `Needed`))
    expect(flat.checks).toBeUndefined()
    expect(flat[`@accessibility`]).toEqual({ label: `T` })
    expect(flat.accessibility).toBeUndefined()
  })
})
