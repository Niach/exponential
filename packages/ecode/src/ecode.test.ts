import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import contract from "../contract.json"
import * as generated from "./contract.generated"
import { REPLAYS, type FixtureName } from "./replay"

const fixture = (name: string) => JSON.parse(readFileSync(join(import.meta.dir, `..`, `fixtures`, `${name}.json`), `utf8`))

describe(`eCode fixtures replay on the TS reference`, () => {
  for (const name of Object.keys(REPLAYS) as FixtureName[]) {
    test(name, () => {
      const file = fixture(name)
      expect((REPLAYS[name] as (f: unknown) => unknown)(file)).toEqual(file)
    })
  }
})

describe(`the contract`, () => {
  test(`every fixture names its spec`, () => {
    for (const name of Object.keys(REPLAYS)) expect(typeof fixture(name).$comment).toBe(`string`)
  })

  test(`the generated enums are the contract's`, () => {
    expect<string[]>([...generated.providerValues]).toEqual(contract.provider.values)
    expect<string[]>([...generated.capabilityValues]).toEqual(contract.capability.values)
    expect<string[]>([...generated.checkConclusionValues]).toEqual(contract.checkConclusion.values)
    expect<string[]>([...generated.forgeReasonValues]).toEqual(contract.forgeReason.values)
  })

  test(`every forge op refusal maps to an eCore error code`, () => {
    for (const reason of generated.forgeReasonValues) {
      expect([`not_found`, `invalid`, `conflict`, `forbidden`]).toContain(generated.reasonCode(reason))
    }
  })

  test(`the failing and satisfying conclusions never overlap`, () => {
    for (const conclusion of contract.checkConclusion.failing) expect(contract.checkConclusion.satisfies).not.toContain(conclusion)
  })
})
