// VAPP-196: fixtures/package-tests.json replayed through the TS reference
// (the Rust core replays the same file; Swift and Kotlin its `validate`).

import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/package-tests.json"
import host from "../../catalog/host.json"
import { validatePackage } from "./package"
import { PACKAGE_TEST_DEFAULTS, PACKAGE_TEST_KINDS, PACKAGE_TEST_LIMITS, runPackageTests, testMatches } from "./tests"

const base = fixture.package as unknown as Record<string, unknown>

describe(`package-tests.json`, () => {
  test(`limits, defaults and the host contract`, () => {
    expect(fixture.limits).toEqual({ ...PACKAGE_TEST_LIMITS })
    expect(fixture.defaults).toEqual({ ...PACKAGE_TEST_DEFAULTS })
    expect(host.package.tests.limits).toEqual({ ...PACKAGE_TEST_LIMITS })
    expect(host.package.tests.defaults).toEqual({ ...PACKAGE_TEST_DEFAULTS })
    expect(host.package.tests.kinds).toEqual([...PACKAGE_TEST_KINDS])
  })

  fixture.matches.forEach((m, i) => test(`matches ${i}`, () => expect(testMatches(m.expect, m.got)).toBe(m.match)))

  test(`runs`, () => {
    expect(runPackageTests({ ...base, tests: fixture.runs.tests })).toEqual(fixture.runs.results as never)
  })

  for (const c of fixture.validate as unknown as { name: string; with: Record<string, unknown>; issues: unknown[] }[])
    test(`validate: ${c.name}`, () => expect(validatePackage({ ...base, ...c.with })).toEqual(c.issues as never))
})
