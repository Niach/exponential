import { describe, expect, test } from "bun:test"
import host from "../../catalog/host.json" with { type: "json" }
// The eMCP contract's exposes fixture (packages/emcp in the Exponential repo):
// the SDK replays its format half; the host-specific codes stay with the host.
import emcp from "../../../emcp/fixtures/exposes.json" with { type: "json" }
import { EXPOSES_LIMITS, exposeActionAccess, exposeIssues, exposeNameDestructive, validatePackage } from "."

// VAPP-180: a package's `exposes` against the eMCP contract.

/** Codes only an Exponential host can tell: whether an action is offered
 *  (`action`), owner-only (`ownerOnly`), or destructive by its arguments. */
function hostOnly(expose: { run: Record<string, unknown> }, code: string): boolean {
  if (code === `action` || code === `ownerOnly`) return true
  return code === `destructive` && typeof expose.run.action === `string` && !exposeNameDestructive(expose.run.action)
}

describe(`exposes (VAPP-180)`, () => {
  test(`the limits and words are the eMCP contract's`, () => {
    const { toolNameMax: _max, slugHashLength: _hash, ...limits } = emcp.limits
    expect(EXPOSES_LIMITS).toEqual(limits)
    expect(host.package.exposes.recordOps).toEqual(emcp.recordOps)
    expect(host.package.exposes.readRecordOps).toEqual(emcp.readRecordOps)
    expect(host.package.exposes.readOpWords).toEqual(emcp.readOpWords)
    expect(host.package.exposes.destructiveWords).toEqual(emcp.destructiveWords)
    for (const code of new Set(Object.values(exposeIssues([`x`, {}], {}, [])).map((i) => i.code))) expect(emcp.errors).toContain(code)
  })

  test.each(emcp.validateCases)(`$name`, (c) => {
    const records = Object.fromEntries(c.context.kinds.map((k) => [k, { schema: { type: `object`, properties: {} } }]))
    const got = exposeIssues(c.exposes, records, c.context.upstreams).map(({ index, code }) => ({ index, code }))
    const want = c.expected.filter((e) => !hostOnly(c.exposes[e.index!] as never, e.code))
    expect(got).toEqual(want)
  })

  test(`more than maxTools`, () => {
    const one = { name: `t`, title: `T`, description: `T.`, input: { type: `object` }, access: `read`, run: { records: { op: `list`, kind: `entry` } } }
    const list = Array.from({ length: emcp.tooManyCase.count }, (_, i) => ({ ...one, name: `t${i}` }))
    expect(exposeIssues(list, { entry: {} }, []).map(({ index, code }) => ({ index, code }))).toEqual(emcp.tooManyCase.expected)
  })

  test.each(emcp.actionAccessCases)(`action access $name`, (c) => {
    expect(exposeActionAccess(c.name)).toBe(c.expected as never)
  })

  test.each(emcp.upstreamCases)(`upstream tool $tool destructive by name`, (c) => {
    // The name rule; a destructiveHint is the host's live check.
    if (c.annotations?.destructiveHint !== true) expect(exposeNameDestructive(c.tool)).toBe(c.destructive)
  })

  test(`a package without exposes stays valid; with them, issues are package issues`, () => {
    const pkg = { id: `a`, name: `A`, version: `1.0.0`, catalogId: `https://ui.exponential.at/catalogs/core/v1`, templates: { t: { components: [{ id: `root`, component: `Text` }] } } }
    expect(validatePackage(pkg)).toEqual([])
    expect(validatePackage({ ...pkg, upstreams: [`m5`], exposes: [{ ...emcp.validateCases[0]!.exposes[2] }] })).toEqual([])
    expect(validatePackage({ ...pkg, exposes: [{ ...emcp.validateCases[0]!.exposes[2] }] })).toEqual([{ path: `/exposes/0/run/upstream`, message: `upstream names none of the package's upstreams` }])
  })
})
